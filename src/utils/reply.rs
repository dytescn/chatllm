// src/utils/reply.rs
//! 助手回复组装。
//!
//! reply 阶段接收 plugins 结果，优先检索 `chat_reply`，按命中的插件调用
//! 业务插件；对话 id 使用命中记录的 `plugins_uid`，不存在时创建。
//! 未命中或调用失败时原样输出 plugins 结果。

use anyhow::Result;
use serde_json::{json, Value};

use crate::utils::chat_plugins;
use crate::utils::dialogue;
use crate::utils::kv;
use crate::utils::message;
use crate::utils::plugins_callback;
use crate::utils::plugins_lib;
use crate::utils::skills;

/// reply 插件检索集合。
pub const REPLY_COLLECTION: &str = "chat_reply";
/// reply 阶段的默认分类检索关键词：先查 `base`，未命中再按输入检索。
pub const BASE_KEYWORD: &str = "base";
/// 检索返回条数。
pub const TOP_K: i64 = 5;

/// 一条命中的 reply 插件记录。
///
/// kv 命中统一提供 `plugins_uid`，插件名与 FFI 符号在调用前通过
/// [`plugins_lib::plugin_call_params`] 查询。
#[derive(Debug, Clone, Default)]
pub struct ReplyHit {
    pub plugins_uid: Option<String>,
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
    /// 是否命中 `chat_reply` 资源（无插件）。
    pub resource_hit: bool,
}

impl Reply {
    /// 从正文与可选思考内容构造。
    pub fn new(content: &str, think: Option<String>) -> Self {
        let content = content.trim().to_string();
        let html = String::new();
        println!(
            "[reply] Reply::new: content={:?}, html_len={}, think={:?}",
            content,
            html.len(),
            think
        );
        Self {
            content,
            think,
            html,
            used_reply_plugin: false,
            resource_hit: false,
        }
    }

    /// 生成轮次记录里的 `reply` 段。
    pub fn to_round_json(&self, start_time: i64, end_time: i64, input: &str) -> Value {
        json!({
            "start_time": start_time,
            "input": { "content": input },
            "calls": [],
            "outputs": [self.content.clone()],
            "end_time": end_time,
            "status": "ok",
        })
    }
}

/// 执行 reply 阶段：先尝试 `chat_reply`，未命中或失败时回退到上游结果。
pub fn reply_run(result: Option<String>, input: &str) -> Reply {
    println!(
        "[reply] reply_run 输入: prompt={:?}, result={:?}",
        input, result
    );
    let (content, resource_hit) = match result {
        Some(value) if !value.trim().is_empty() => {
            let (plugin_reply, resource_hit) = reply_with_plugin(&value);
            if let Some(reply) = plugin_reply {
                return reply;
            }
            (value, resource_hit)
        }
        _ => (fallback(None, input), false),
    };
    println!("[reply] 最终 content: {:?}", content);
    let mut reply = Reply::new(&content, None);
    reply.resource_hit = resource_hit;
    reply
}

/// 用 plugins 阶段的 `contents` 生成回复。
pub fn from_contents(contents: &[String], input: &str) -> Reply {
    reply_run(chat_plugins::join_results(contents), input)
}

/// 兼容入口：语义与 [`reply_run`] 相同。
pub fn from_result(result: Option<String>, input: &str) -> Reply {
    reply_run(result, input)
}

/// 兼容入口：语义与 [`reply_run`] 相同。
pub fn from_think(think: Option<String>, input: &str) -> Reply {
    reply_run(think, input)
}

/// 检索 `chat_reply`，取 Top1。
pub fn find(input: &str, top_k: i64) -> Result<Option<ReplyHit>> {
    Ok(find_hit(input, top_k)?.0)
}

/// 检索 `chat_reply`，同时区分「插件命中」与「资源命中」。
///
/// 默认分类优先：先按 [`BASE_KEYWORD`] 检索，未命中时再按输入内容检索。
fn find_hit(input: &str, top_k: i64) -> Result<(Option<ReplyHit>, bool)> {
    let queries: Vec<&str> = if input.trim() == BASE_KEYWORD {
        vec![BASE_KEYWORD]
    } else {
        vec![BASE_KEYWORD, input]
    };

    let mut resource_hit = false;
    for query in queries {
        let (hit, hit_resource) = search_reply(query, top_k)?;
        resource_hit |= hit_resource;
        if hit.is_some() {
            return Ok((hit, resource_hit));
        }
    }
    Ok((None, resource_hit))
}

/// 单次 `chat_reply` 检索。
fn search_reply(query: &str, top_k: i64) -> Result<(Option<ReplyHit>, bool)> {
    println!(
        "[reply] find 输入: query={:?}, collection={}, top_k={}",
        query, REPLY_COLLECTION, top_k
    );
    let data = kv::search(
        REPLY_COLLECTION,
        query,
        top_k,
        kv::match_min_score(),
        None,
        true,
    )?;
    let list = kv::take_list(data);
    println!("[reply] chat_reply 返回 {} 条", list.len());
    for (index, hit) in list.iter().enumerate() {
        println!(
            "[reply] hit[{}] key={:?}, value={:?}, meta={}",
            index,
            hit.get("key").and_then(Value::as_str),
            hit.get("value").and_then(Value::as_str),
            hit.get("meta")
                .map(|v| v.to_string())
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

/// 资源命中：有 plugin_name / kind，但没有 `plugins_uid`。
pub fn is_resource_hit(hit: &Value) -> bool {
    let meta = hit.get("meta");
    let value = parsed_value(hit);
    let has_plugin = str_at(meta, "plugins_uid")
        .or_else(|| str_at(value.as_ref(), "plugins_uid"))
        .is_some();
    let resource = str_at(meta, "plugin_name")
        .or_else(|| str_at(value.as_ref(), "plugin_name"))
        .or_else(|| str_at(meta, "kind"))
        .or_else(|| str_at(value.as_ref(), "kind"));
    !has_plugin && resource.is_some()
}

/// 解析命中记录；只取统一的 `plugins_uid`。
pub fn parse_hit(hit: &Value) -> Option<ReplyHit> {
    let meta = hit.get("meta");
    let value = parsed_value(hit);

    let plugins_uid =
        str_at(meta, "plugins_uid").or_else(|| str_at(value.as_ref(), "plugins_uid"))?;

    Some(ReplyHit {
        plugins_uid: Some(plugins_uid),
    })
}

/// 命中插件后执行；返回 `(Reply, resource_hit)`；`None` 表示未命中或执行失败。
fn reply_with_plugin(input: &str) -> (Option<Reply>, bool) {
    let (hit, resource_hit) = match find_hit(input, TOP_K) {
        Ok((Some(hit), resource_hit)) => (hit, resource_hit),
        Ok((None, true)) => {
            println!("[reply] chat_reply 资源命中但无插件，原样输出");
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

    let plugins_uid = hit.plugins_uid.as_deref();
    let Some(params) = plugins_uid.and_then(plugins_lib::plugin_params) else {
        println!("[reply] 未找到 plugins_uid 对应的插件参数");
        return (None, resource_hit);
    };

    let dialogue_id = ensure_dialogue(plugins_uid, Some(&params.plugin_name));

    let content = match plugins_callback::call_plugin(&params.plugin, &params.ffi_symbol, input) {
        Ok(content) => content,
        Err(error) => {
            println!("[reply] 插件调用失败: {:#}", error);
            return (None, resource_hit);
        }
    };

    if content.trim().is_empty() {
        println!("[reply] 插件返回空内容，原样输出");
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
    let system = plugin_name.and_then(|name| skills::skills_for(None, Some(name)));
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
    println!("[reply] fallback: {:?}", text);
    text
}
