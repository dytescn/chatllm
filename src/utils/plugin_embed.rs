//! 插件能力向量化与语义检索（分类分片）。
//!
//! 数据链路：plugin_lib.get_functions → qwen3-embed
//! → embedding_store.upsert('plugin', ...) → plugin_lib.patch_function；
//! 检索时先从 plugin_lib 过滤候选，再由 embedding_store 遍历分片并合并 top_k。

use crate::utils::{embed, embedding_store, plugin_lib};
use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

const CATEGORY: &str = embedding_store::CATEGORY_PLUGIN;

#[derive(Debug, Default, Deserialize)]
pub struct EmbedReq {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub force: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SearchReq {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub top_k: Option<i64>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub min_score: Option<f32>,
}

pub fn embed(req: EmbedReq) -> Result<Value> {
    embedding_store::ensure_meta()?;
    embedding_store::migrate_legacy_once()?;

    let limit = req.limit.unwrap_or(100).clamp(1, 500);
    let force = req.force.unwrap_or(false);

    let data = plugin_lib::get_functions(json!({
        "local": 1,
        "embed_status": [0, 2],
        "page": 1,
        "page_size": limit,
    }))?;
    let list = data
        .get("list")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut processed = 0;
    let mut skipped = 0;
    let mut failed = 0;

    for function in list {
        let uuid = function
            .get("uuid")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if uuid.is_empty() {
            failed += 1;
            continue;
        }

        let text = build_text(&function);
        let hash = sha256_hex(&text);

        if !force && embedding_store::existing_hash(CATEGORY, &uuid)? == Some(hash.clone()) {
            skipped += 1;
            continue;
        }

        match embed_and_store(&uuid, &text, &hash) {
            Ok(()) => processed += 1,
            Err(error) => {
                eprintln!("[plugin_embed] {uuid}: {error:#}");
                failed += 1;
            }
        }
    }

    Ok(json!({
        "processed": processed,
        "skipped": skipped,
        "failed": failed,
        "model": embed::MODEL,
        "dim": embed::DIM,
    }))
}

pub fn search(req: SearchReq) -> Result<Value> {
    embedding_store::ensure_meta()?;
    embedding_store::migrate_legacy_once()?;

    let query = req.query.trim();
    if query.is_empty() {
        anyhow::bail!("query 不能为空");
    }

    let model = req.model.unwrap_or_else(|| embed::MODEL.to_string());
    if model != embed::MODEL {
        anyhow::bail!("暂不支持模型: {model}");
    }

    let top_k = req.top_k.unwrap_or(5);
    if top_k <= 0 {
        anyhow::bail!("top_k 必须大于 0");
    }
    let top_k = top_k.clamp(1, 50) as usize;
    let min_score = req.min_score.unwrap_or(0.2);

    let query_vector = embed::embed_text(query)?;
    let candidates = candidate_functions()?;
    let uuids: Vec<String> = candidates
        .iter()
        .filter_map(|item| item.get("uuid").and_then(Value::as_str))
        .map(str::to_string)
        .collect();

    if uuids.is_empty() {
        return Ok(json!({ "model": model, "dim": embed::DIM, "list": [] }));
    }

    let scored = embedding_store::search(CATEGORY, &query_vector, top_k, min_score, Some(&uuids))?;

    let mut by_uuid = HashMap::new();
    for item in candidates {
        if let Some(uuid) = item.get("uuid").and_then(Value::as_str) {
            by_uuid.insert(uuid.to_string(), item);
        }
    }

    let mut list = Vec::new();
    for (uuid, score) in scored {
        let Some(item) = by_uuid.get(&uuid) else {
            continue;
        };
        list.push(json!({
            "uuid": uuid,
            "lib_name": item.get("lib_name").cloned().unwrap_or(Value::Null),
            "symbol": item.get("symbol").cloned().unwrap_or(Value::Null),
            "ffi_symbol": item.get("ffi_symbol").cloned().unwrap_or(Value::Null),
            "title": item.get("title").cloned().unwrap_or(Value::Null),
            "description": item.get("description").cloned().unwrap_or(Value::Null),
            "params_schema": parse_json_field(item.get("params_schema")),
            "return_schema": parse_json_field(item.get("return_schema")),
            "side_effects": item.get("side_effects").cloned().unwrap_or(Value::Null),
            "score": score,
        }));
    }

    Ok(json!({ "model": model, "dim": embed::DIM, "list": list }))
}

fn candidate_functions() -> Result<Vec<Value>> {
    const PAGE: usize = 500;
    let mut out = Vec::new();
    let mut page = 1usize;

    loop {
        let data = plugin_lib::get_functions(json!({
            "local": 1,
            "embed_status": [1],
            "use_ai": 1,
            "page": page,
            "page_size": PAGE,
        }))?;
        let list = data
            .get("list")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let count = list.len();
        out.extend(list);
        if count < PAGE || page >= 100 {
            break;
        }
        page += 1;
    }

    Ok(out)
}

fn embed_and_store(uuid: &str, text: &str, hash: &str) -> Result<()> {
    let vector = embed::embed_text(text)?;
    embedding_store::upsert(CATEGORY, uuid, embed::DIM, &vector, hash)?;

    plugin_lib::patch_function(json!({
        "uuid": uuid,
        "embed_status": 1,
        "embed_model": embed::MODEL,
        "embed_text_hash": hash,
        "embedded_at": now(),
    }))?;

    Ok(())
}

fn build_text(function: &Value) -> String {
    let title = function.get("title").and_then(Value::as_str).unwrap_or("");
    let description = function
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let target = function.get("target").and_then(Value::as_str).unwrap_or("");
    let params = function
        .get("params_schema")
        .map(ToString::to_string)
        .unwrap_or_default();
    let returns = function
        .get("return_schema")
        .map(ToString::to_string)
        .unwrap_or_default();
    format!("{title}\n{description}\n目标: {target}\n参数: {params}\n返回: {returns}")
}

fn parse_json_field(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => {
            serde_json::from_str(text).unwrap_or(Value::String(text.clone()))
        }
        Some(value) => value.clone(),
        None => Value::Null,
    }
}

fn sha256_hex(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    format!("{digest:x}")
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0)
}
