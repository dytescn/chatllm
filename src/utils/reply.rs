// src/utils/reply.rs
//! 助手回复组装。
//!
//! reply 阶段接收 plugins 结果，优先检索 `chat_reply`，按命中的插件调用
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
pub const REPLY_COLLECTION: &str = "chat_reply";
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
    /// 是否由 `chat_reply` 命中插件生成。
    pub used_reply_plugin: bool,
    /// 是否命中 `chat_reply` 资源（reply_base），但未命中可调用插件。
    pub resource_hit: bool,
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
            resource_hit: false,
        }
    }

    /// 使用 plugins 结果生成回复。
    ///
    /// 先尝试 `chat_reply`；未命中或失败时使用 reply_base 清洗。
    pub fn from_result(result: Option<String>, input: &str) -> Self {
        println!(
            "[reply] from_result 输入: prompt={:?}, result={:?}",
            preview(input, 300),
            result.as_deref().map(|v| preview(v, 500))
        );
        let (content, resource_hit) = match result {
            Some(value) if !value.trim().is_empty() => {
                let (plugin_reply, resource_hit) = reply_with_plugin(&value);
                if let Some(reply) = plugin_reply {
                    return reply;
                }
                let content = match reply_base::process(&value) {
                    Some(cleaned) => cleaned,
                    None => {
                        println!("[reply] reply_base 不可用，回退原始 plugins 结果");
                        value
                    }
                };
                (content, resource_hit)
            }
            _ => (fallback(None, input), false),
        };
        println!("[reply] 最终 content: {:?}", preview(&content, 500));
        let mut reply = Self::new(&content, None);
        reply.resource_hit = resource_hit;
        reply
    }

    /// 兼容入口：语义与 [`Reply::from_result`] 相同。
    pub fn from_think(think: Option<String>, input: &str) -> Self {
        Self::from_result(think, input)
    }
}

/// 检索 `chat_reply`，取 Top1。
pub fn find(input: &str, top_k: i64) -> Result<Option<ReplyHit>> {
    Ok(find_hit(input, top_k)?.0)
}

/// 检索 `chat_reply`，同时区分「插件命中」与「资源命中」。
fn find_hit(input: &str, top_k: i64) -> Result<(Option<ReplyHit>, bool)> {
    println!(
        "[reply] find 输入: query={:?}, collection={}, top_k={}",
        preview(input, 300),
        REPLY_COLLECTION,
        top_k
    );
    let data = kv::search(REPLY_COLLECTION, input, top_k, kv::match_min_score(), None, true)?;
    let list = data
        .get("list")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    println!("[reply] chat_reply 返回 {} 条", list.len());
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
    let resource_hit = list.iter().any(is_resource_hit);
    Ok((list.first().and_then(parse_hit), resource_hit))
}

fn str_at(value: Option<&Value>, key: &str) -> Option<String> {
    value
        .and_then(|v| v.get(key))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn parsed_value(hit: &Value) -> Option<Value> {
    hit.get("value")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
}

/// 资源命中：有 plugin_name / kind，但没有可调用插件。
fn is_resource_hit(hit: &Value) -> bool {
    let meta = hit.get("meta");
    let value = parsed_value(hit);
    let plugin = str_at(meta, "plugin")
        .or_else(|| str_at(meta, "lib_name"))
        .or_else(|| str_at(value.as_ref(), "lib_name"))
        .or_else(|| str_at(value.as_ref(), "plugin"))
        .or_else(|| str_at(value.as_ref(), "name"));
    let resource = str_at(meta, "plugin_name")
        .or_else(|| str_at(value.as_ref(), "plugin_name"))
        .or_else(|| str_at(meta, "kind"))
        .or_else(|| str_at(value.as_ref(), "kind"));
    plugin.is_none() && resource.is_some()
}

/// 解析命中记录；至少需要插件名。
pub fn parse_hit(hit: &Value) -> Option<ReplyHit> {
    let meta = hit.get("meta");
    let value = parsed_value(hit);

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

/// 命中插件后执行；返回 `(Reply, resource_hit)`；`None` 表示未命中或执行失败。
fn reply_with_plugin(input: &str) -> (Option<Reply>, bool) {
    let (hit, resource_hit) = match find_hit(input, TOP_K) {
        Ok((Some(hit), resource_hit)) => (hit, resource_hit),
        Ok((None, true)) => {
            println!("[reply] chat_reply 资源命中（reply_base），回退 reply_base");
            return (None, true);
        }
        Ok((None, false)) => {
            println!("[reply] chat_reply 未命中");
            return (None, false);
        }
        Err(error) => {
            println!("[reply] chat_reply 检索失败: {:#}", error);
            return (None, false);
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
                    return (None, resource_hit);
                }
            }
        }
        None => match plugins::call_plugin(&hit.plugin, &hit.ffi_symbol, input) {
            Ok(content) => content,
            Err(error) => {
                println!("[reply] 插件调用失败: {:#}", error);
                return (None, resource_hit);
            }
        },
    };

    if content.trim().is_empty() {
        println!("[reply] 插件返回空内容，回退 reply_base");
        return (None, resource_hit);
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
    reply.resource_hit = resource_hit;
    (Some(reply), resource_hit)
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
    fn parses_chat_reply_from_meta() {
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
    fn parses_chat_reply_from_value() {
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
    fn rejects_chat_reply_without_plugin() {
        assert!(parse_hit(&json!({ "key": "empty" })).is_none());
    }

    #[test]
    fn reply_collection_is_chat_reply() {
        assert_eq!(REPLY_COLLECTION, "chat_reply");
    }

    #[test]
    fn detects_resource_hit_without_plugin() {
        let hit = json!({
            "key": "reply-base:skill:main",
            "meta": {
                "plugin_name": "reply_base",
                "kind": "skill",
                "plugin_uuid": "6f2b8d41-9c3a-4e7b-8d5f-1a2c3e4b5d60"
            }
        });
        assert!(is_resource_hit(&hit));
        assert!(parse_hit(&hit).is_none());
    }
}
