// src/utils/taskflow.rs
//! taskflow 对象集：读取插件对应的任务流元素。
//!
//! `plugin_lib` 把所有插件的任务流合并在 designdb 的一条文档里
//! （`path = taskflow`、`collection = taskflow`、`_id = taskflow`），
//! 元素通过 `plugin_uuid` 归属到具体插件。chat 流程在 `chat_plugins`
//! 命中后，用命中记录的 `plugin_uuid` 取出对应元素，再执行元素里的
//! 回调方法。

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::utils::designdb;
use crate::utils::chat_plugins::{ChatPluginCall, ChatPluginsOutcome};

/// taskflow 对象集的 path。
pub const PATH: &str = "taskflow";
/// taskflow 对象集的 collection。
pub const COLLECTION: &str = "taskflow";
/// 唯一文档的 `_id`。
pub const DOC_ID: &str = "taskflow";
/// 元素中回调方法的候选字段，按优先级排列。
pub const CALLBACK_FIELDS: &[&str] = &["callback", "ffi_symbol", "method", "uid"];

/// 读取归属于 `plugin_uuid` 的任务流元素。
///
/// 文档或 `taskflows` 字段缺失时返回空数组；读取失败向上返回错误，
/// 由调用方决定是否跳过 plugins 阶段。
pub fn for_plugin(plugin_uuid: &str) -> Result<Vec<Value>> {
    let uuid = plugin_uuid.trim();
    if uuid.is_empty() {
        return Ok(Vec::new());
    }

    let docs = designdb::query(
        PATH,
        COLLECTION,
        Some(json!({ "_id": DOC_ID })),
        None,
        None,
        Some(1),
    )
    .context("读取 taskflow 文档失败")?;

    let taskflows = docs
        .first()
        .and_then(|doc| doc.get("taskflows"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let matched = filter_for_plugin(&taskflows, uuid);
    println!(
        "[taskflow] plugin_uuid={:?}, 任务流 {} 条，命中 {} 条",
        uuid,
        taskflows.len(),
        matched.len()
    );
    Ok(matched)
}

/// 筛选 `plugin_uuid` 等于给定 uuid 的任务流元素。
pub fn filter_for_plugin(items: &[Value], plugin_uuid: &str) -> Vec<Value> {
    let uuid = plugin_uuid.trim();
    if uuid.is_empty() {
        return Vec::new();
    }
    items
        .iter()
        .filter(|item| plugin_uuid_of(item).as_deref() == Some(uuid))
        .cloned()
        .collect()
}

/// 从任务流元素中读取回调方法。
pub fn callback_of(item: &Value) -> Option<String> {
    CALLBACK_FIELDS.iter().find_map(|field| {
        item.get(*field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

/// 从任务流元素中读取所属插件 uuid。
pub fn plugin_uuid_of(item: &Value) -> Option<String> {
    ["plugin_uuid", "uuid"].iter().find_map(|field| {
        item.get(*field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

impl ChatPluginsOutcome {
    /// 生成轮次记录里的 `plugins` 段。
    pub fn to_json(&self) -> Value {
        json!({
            "start_time": self.start_time,
            "inputs": self.inputs,
            "calls": self.calls.iter().map(ChatPluginCall::to_json).collect::<Vec<_>>(),
            "outputs": self.outputs,
            "end_time": self.end_time,
            "status": self.status,
        })
    }
}
