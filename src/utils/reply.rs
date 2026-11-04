// src/utils/reply.rs
//! 助手回复组装。
//!
//! reply 阶段接收 plugins 结果，优先检索 `plugins_reply`，按命中的插件调用
//! 供应商（cloud）或业务插件；对话 id 使用命中记录的 `plugins_uid`，不存在时创建。
//! 未命中或调用失败时回退到 reply_base 清洗。

use anyhow::Result;
use serde_json::{json, Value};

use crate::utils::cloud::{self, Supplier};
use crate::utils::dialogue;
use crate::utils::kv;
use crate::utils::markparse;
use crate::utils::message;
use crate::utils::plugins;
use crate::utils::reply_base;
use crate::utils::think;

/// reply 插件检索集合。
pub const REPLY_COLLECTION: &str = "plugins_reply";
/// 检索返回条数。
pub const TOP_K: i64 = 5;

/// 一条命中的 reply 插件记录。
#[derive(Debug, Clone, Default)]
pub struct ReplyHit {
    pub key: Option<String>,
    /// 对话 id，兼容 `plugins_uid` / `plugin_uuid`。
    pub plugins_uid: Option<String>,
    /// 资源包名，用于读取 `./plugins/<name>/<name>.md`。
    pub plugin_name: Option<String>,
    /// 插件名或供应商库名。
    pub plugin: String,
    /// 插件 FFI 符号，默认 `chat`。
    pub ffi_symbol: String,
}

/// 一条组装好的助手回复。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// 回复正文。
    pub content: String,
    /// 思考内容；没有时为 `None`。
    pub think: Option<String>,
    /// 正文的 HTML 版本。
    pub html: String,
    /// 是否由 `plugins_reply` 命中插件生成。
    pub used_reply_plugin: bool,
}

impl Reply {
    /// 从正文与可选思考内容构造。
    pub fn new(content: &str, think: Option<String>) -> Self {
        let content = content.trim().to_string();
        let html = markparse::to_html(&content).unwrap_or_default();
        println!(
            "[reply] Reply::new: content={:?}, html_len={}, think={:?}",
            preview(&content, 300),
            html.len(),
            think.as_deref().map(|v| preview(v, 200))
        );
        Self {
            content,
            think,
            html,
            used_reply_plugin: false,
        }
    }

    /// 使用 plugins 结果生成回复。
    ///
    /// 先尝试 `plugins_reply`；未命中或失败时使用 reply_base 清洗。
    pub fn from_result(result: Option<String>, input: &str) -> Self {
        println!(
            "[reply] from_result 输入: prompt={:?}, result={:?}",
            preview(input, 300),
            result.as_deref().map(|v| preview(v, 500))
        );
        let content = match result {
            Some(value) if !value.trim().is_empty() => {
                if let Some(reply) = reply_with_plugin(&value) {
                    return reply;
                }
                match reply_base::process(&value) {
                    Some(cleaned) => cleaned,
                    None => {
                        println!("[reply] reply_base 不可用，回退原始 plugins 结果");
                        value
                    }
                }
            }
            _ => fallback(None, input),
        };
        println!("[reply] 最终 content: {:?}", preview(&content, 500));
        Self::new(&content, None)
    }

    /// 兼容入口：语义与 [`Reply::from_result`] 相同。
    pub fn from_think(think: Option<String>, input: &str) -> Self {
        Self::from_result(think, input)
    }
}

/// 检索 `plugins_reply`，取 Top1。
pub fn find(input: &str, top_k: i64) -> Result<Option<ReplyHit>> {
    println!(
        "[reply] find 输入: query={:?}, collection={}, top_k={}",
        preview(input, 300),
        REPLY_COLLECTION,
        top_k
    );
    let data = kv::search(REPLY_COLLECTION, input, top_k, 0.0, None, true)?;
    let list = data
        .get("list")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    println!("[reply] plugins_reply 返回 {} 条", list.len());
    for (index, hit) in list.iter().enumerate() {
        println!(
            "[reply] hit[{}] key={:?}, value={:?}, meta={}",
            index,
            hit.get("key").and_then(Value::as_str),
            hit.get("value").and_then(Value::as_str),
            hit.get("meta")
                .map(|v| preview(&v.to_string(), 300))
                .unwrap_or_else(|| "null".to_string())
        );
    }
    Ok(list.first().and_then(parse_hit))
}

/// 解析命中记录；至少需要插件名。
pub fn parse_hit(hit: &Value) -> Option<ReplyHit> {
    let meta = hit.get("meta");
    let value = hit
        .get("value")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str::<Value>(text).ok());

    let str_at = |value: Option<&Value>, key: &str| -> Option<String> {
        value
            .and_then(|v| v.get(key))
            .and_then(Value::as_str)
            .map(str::to_string)
    };

    let plugin = str_at(meta, "plugin")
        .or_else(|| str_at(meta, "lib_name"))
        .or_else(|| str_at(value.as_ref(), "lib_name"))
        .or_else(|| str_at(value.as_ref(), "plugin"))
        .or_else(|| str_at(value.as_ref(), "name"))?;

    let plugins_uid = str_at(meta, "plugins_uid")
        .or_else(|| str_at(meta, "plugin_uuid"))
        .or_else(|| str_at(Some(hit), "plugins_uid"))
        .or_else(|| str_at(Some(hit), "plugin_uuid"))
        .or_else(|| str_at(value.as_ref(), "plugins_uid"))
        .or_else(|| str_at(value.as_ref(), "plugin_uuid"));

    let plugin_name = str_at(meta, "plugin_name")
        .or_else(|| str_at(value.as_ref(), "plugin_name"))
        .or_else(|| str_at(value.as_ref(), "name"));

    let ffi_symbol = str_at(meta, "ffi_symbol")
        .or_else(|| str_at(value.as_ref(), "ffi_symbol"))
        .unwrap_or_else(|| "chat".to_string());

    Some(ReplyHit {
        key: str_at(Some(hit), "key"),
        plugins_uid,
        plugin_name,
        plugin,
        ffi_symbol,
    })
}

/// 命中插件后执行；返回 `None` 表示未命中或执行失败，调用方回退。
fn reply_with_plugin(input: &str) -> Option<Reply> {
    let hit = match find(input, TOP_K) {
        Ok(Some(hit)) => hit,
        Ok(None) => {
            println!("[reply] plugins_reply 未命中");
            return None;
        }
        Err(error) => {
            println!("[reply] plugins_reply 检索失败: {:#}", error);
            return None;
        }
    };

    let dialogue_id = ensure_dialogue(hit.plugins_uid.as_deref(), hit.plugin_name.as_deref());
    let history = dialogue_id
        .as_deref()
        .and_then(|id| match message::message_history(id) {
            Ok(history) => Some(history),
            Err(error) => {
                println!("[reply] 读取对话历史失败: {:#}", error);
                None
            }
        })
        .unwrap_or_default();

    let content = match Supplier::from_exact_code(&hit.plugin) {
        Some(supplier) => {
            let mut messages = Vec::new();
            if let Some(system) = hit.plugin_name.as_deref().and_then(think::skills_for) {
                messages.push(json!({ "role": "system", "content": system }));
            }
            messages.extend(history);
            messages.push(json!({ "role": "user", "content": input }));
            println!(
                "[reply] 供应商调用: supplier={:?}, messages={}",
                supplier,
                messages.len()
            );
            match cloud::chat_with(supplier, input, &messages) {
                Ok(reply) => reply.content,
                Err(error) => {
                    println!("[reply] 供应商调用失败: {:#}", error);
                    return None;
                }
            }
        }
        None => match plugins::call_plugin(&hit.plugin, &hit.ffi_symbol, input) {
            Ok(content) => content,
            Err(error) => {
                println!("[reply] 插件调用失败: {:#}", error);
                return None;
            }
        },
    };

    if content.trim().is_empty() {
        println!("[reply] 插件返回空内容，回退 reply_base");
        return None;
    }

    if let Some(id) = dialogue_id.as_deref() {
        if let Err(error) = message::append_message(id, "user", input) {
            println!("[reply] 写入 user 消息失败: {:#}", error);
        }
        if let Err(error) = message::append_message(id, "assistant", &content) {
            println!("[reply] 写入 assistant 消息失败: {:#}", error);
        }
    }

    let mut reply = Reply::new(&content, None);
    reply.used_reply_plugin = true;
    Some(reply)
}

/// 使用 `plugins_uid` 的对话；不存在或没有 uid 时创建。
fn ensure_dialogue(plugins_uid: Option<&str>, plugin_name: Option<&str>) -> Option<String> {
    let system = plugin_name.and_then(think::skills_for);
    let title = "reply 系统对话";

    if let Some(id) = plugins_uid.map(str::trim).filter(|id| !id.is_empty()) {
        if dialogue::is_valid_uuid(id) {
            match dialogue::get_dialogue(id) {
                Ok(Some(_)) => {
                    println!("[reply] 复用对话: {}", id);
                    return Some(id.to_string());
                }
                Ok(None) => {
                    println!("[reply] 对话不存在，创建: {}", id);
                    return dialogue::create_dialogue_with(Some(id), title, 2, system.as_deref())
                        .ok();
                }
                Err(error) => println!("[reply] 查询对话失败: {:#}", error),
            }
        } else {
            println!("[reply] plugins_uid 非法: {}", id);
        }
    }

    println!("[reply] 未提供 plugins_uid，创建新对话");
    dialogue::create_dialogue_with(None, title, 2, system.as_deref()).ok()
}

fn fallback(result: Option<&str>, input: &str) -> String {
    let text = result
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("我收到你的信息了：{input}"));
    println!("[reply] fallback: {:?}", preview(&text, 300));
    text
}

fn preview(value: &str, max: usize) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plugins_reply_from_meta() {
        let hit = json!({
            "key": "reply-base",
            "meta": {
                "plugin": "deepseek",
                "ffi_symbol": "chat",
                "plugins_uid": "6f2b8d41-9c3a-4e7b-8d5f-1a2c3e4b5d60",
                "plugin_name": "reply_base"
            }
        });
        let parsed = parse_hit(&hit).expect("应解析成功");
        assert_eq!(parsed.plugin, "deepseek");
        assert_eq!(parsed.ffi_symbol, "chat");
        assert_eq!(parsed.plugin_name.as_deref(), Some("reply_base"));
        assert_eq!(
            parsed.plugins_uid.as_deref(),
            Some("6f2b8d41-9c3a-4e7b-8d5f-1a2c3e4b5d60")
        );
    }

    #[test]
    fn parses_plugins_reply_from_value() {
        let hit = json!({
            "key": "reply-value",
            "value": "{\"lib_name\":\"siliconflow_t2t\",\"plugin_uuid\":\"6f2b8d41-9c3a-4e7b-8d5f-1a2c3e4b5d60\",\"plugin_name\":\"reply_base\"}"
        });
        let parsed = parse_hit(&hit).expect("应解析成功");
        assert_eq!(parsed.plugin, "siliconflow_t2t");
        assert_eq!(parsed.ffi_symbol, "chat");
        assert!(parsed.plugins_uid.is_some());
    }

    #[test]
    fn rejects_plugins_reply_without_plugin() {
        assert!(parse_hit(&json!({ "key": "empty" })).is_none());
    }
}
