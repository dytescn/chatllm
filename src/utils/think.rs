// src/utils/think.rs
//! 思考阶段：查询 kv_lib 的 `chat_think` 分类。
//!
//! 只有 `think` 阶段访问 kv_lib；`reply` 阶段只消费这里返回的结果。

use crate::utils::kv;
use serde_json::Value;

/// `chat_think` 分类：思考阶段查询的 kv 集合。
pub const COLLECTION: &str = "chat_think";

/// 在 `chat_think` 分类中检索，返回首条命中的 `value`。
pub fn query(input: &str, top_k: i64) -> anyhow::Result<Option<String>> {
    let data = kv::search(COLLECTION, input, top_k, 0.0, None, true)?;
    Ok(data
        .get("list")
        .and_then(Value::as_array)
        .and_then(|list| list.first())
        .and_then(|hit| hit.get("value"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|text| !text.trim().is_empty()))
}
