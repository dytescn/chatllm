//! 插件能力向量化与语义检索。
//!
//! 数据链路：plugin_lib.get_functions → qwen3-embed → dialogue.plugin_embeddings
//! → plugin_lib.patch_function；检索时反向读取向量并回到 plugin_lib 取详情。

use crate::utils::db;
use crate::utils::embed;
use crate::utils::plugin_lib;
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

const DB: &str = "dialogue";
const TABLE: &str = "plugin_embeddings";

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

pub fn ensure_table() -> Result<()> {
    db::db_execute(
        DB,
        &format!(
            "CREATE TABLE IF NOT EXISTS {TABLE} (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                function_uuid TEXT NOT NULL,
                lib_name      TEXT NOT NULL,
                symbol        TEXT NOT NULL,
                use_ai        INTEGER DEFAULT 1,
                model         TEXT NOT NULL,
                dim           INTEGER NOT NULL,
                vector        BLOB NOT NULL,
                text_hash     TEXT NOT NULL,
                created_at    INTEGER,
                updated_at    INTEGER,
                UNIQUE(function_uuid, model)
            )"
        ),
    )?;
    db::db_execute(
        DB,
        &format!("CREATE INDEX IF NOT EXISTS idx_plugin_embeddings_model ON {TABLE}(model)"),
    )?;
    db::db_execute(
        DB,
        &format!("CREATE INDEX IF NOT EXISTS idx_plugin_embeddings_fn ON {TABLE}(function_uuid)"),
    )?;
    db::db_execute(
        DB,
        &format!("CREATE INDEX IF NOT EXISTS idx_plugin_embeddings_ai ON {TABLE}(model, use_ai)"),
    )?;
    Ok(())
}

pub fn embed(req: EmbedReq) -> Result<Value> {
    ensure_table()?;

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

        if !force && existing_hash(&uuid)? == Some(hash.clone()) {
            skipped += 1;
            continue;
        }

        match embed_and_store(&function, &uuid, &text, &hash) {
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
    ensure_table()?;

    let query = req.query.trim();
    if query.is_empty() {
        anyhow::bail!("query 不能为空");
    }

    let model = req.model.unwrap_or_else(|| embed::MODEL.to_string());
    if model != embed::MODEL {
        anyhow::bail!("暂不支持模型: {model}");
    }

    let query_vector = embed::embed_text(query)?;

    // database.dll 的输出缓冲只有 64 KiB，4096 字节向量以 JSON 数组返回会膨胀；
    // 每次只取 2 行，避免单次响应超限。
    const PAGE: i64 = 2;
    let mut rows: Vec<Value> = Vec::new();
    let mut offset = 0;
    loop {
        let batch = db::db_query(
            DB,
            &format!(
                "SELECT function_uuid, vector FROM {TABLE} \
                 WHERE model = {} AND use_ai = 1 LIMIT {PAGE} OFFSET {offset}",
                sql_str(&model)
            ),
        )?;
        let count = batch.len();
        rows.extend(batch);
        if count < PAGE as usize {
            break;
        }
        offset += PAGE;
    }

    let mut scored: Vec<(String, f32)> = Vec::new();
    for row in &rows {
        let uuid = row
            .get("function_uuid")
            .and_then(Value::as_str)
            .unwrap_or("");
        if uuid.is_empty() {
            continue;
        }
        let Some(vector) = value_to_vector(row.get("vector")) else {
            continue;
        };
        scored.push((uuid.to_string(), embed::cosine(&query_vector, &vector)));
    }

    scored.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let min_score = req.min_score.unwrap_or(0.0);
    scored.retain(|(_, score)| *score >= min_score);
    scored.truncate(req.top_k.unwrap_or(5).clamp(1, 50) as usize);

    if scored.is_empty() {
        return Ok(json!({ "model": model, "dim": embed::DIM, "list": [] }));
    }

    let uuids: Vec<String> = scored.iter().map(|(uuid, _)| uuid.clone()).collect();
    let details = plugin_lib::get_functions(json!({ "uuids": uuids }))?;
    let detail_list = details
        .get("list")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut by_uuid = HashMap::new();
    for item in detail_list {
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

fn embed_and_store(function: &Value, uuid: &str, text: &str, hash: &str) -> Result<()> {
    let lib_name = function
        .get("lib_name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let symbol = function
        .get("symbol")
        .and_then(Value::as_str)
        .unwrap_or("");
    let vector = embed::embed_text(text)?;
    let blob = embed::hex(&embed::to_blob(&vector));
    let timestamp = now();

    let sql = format!(
        "INSERT INTO {TABLE} \
         (function_uuid, lib_name, symbol, use_ai, model, dim, vector, text_hash, created_at, updated_at) \
         VALUES ({}, {}, {}, 1, {}, {}, X'{}', {}, {timestamp}, {timestamp}) \
         ON CONFLICT(function_uuid, model) DO UPDATE SET \
         lib_name = excluded.lib_name, symbol = excluded.symbol, dim = excluded.dim, \
         vector = excluded.vector, text_hash = excluded.text_hash, updated_at = excluded.updated_at",
        sql_str(uuid),
        sql_str(lib_name),
        sql_str(symbol),
        sql_str(embed::MODEL),
        embed::DIM,
        blob,
        sql_str(hash),
    );
    db::db_execute(DB, &sql).context("写入 plugin_embeddings 失败")?;

    plugin_lib::patch_function(json!({
        "uuid": uuid,
        "embed_status": 1,
        "embed_model": embed::MODEL,
        "embed_text_hash": hash,
        "embedded_at": timestamp,
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

fn existing_hash(uuid: &str) -> Result<Option<String>> {
    let rows = db::db_query(
        DB,
        &format!(
            "SELECT text_hash FROM {TABLE} WHERE function_uuid = {} AND model = {} LIMIT 1",
            sql_str(uuid),
            sql_str(embed::MODEL)
        ),
    )?;
    Ok(rows
        .first()
        .and_then(|row| row.get("text_hash"))
        .and_then(Value::as_str)
        .map(ToString::to_string))
}

fn value_to_vector(value: Option<&Value>) -> Option<Vec<f32>> {
    let array = value?.as_array()?;
    if array.len() % 4 != 0 {
        return None;
    }
    let bytes: Vec<u8> = array
        .iter()
        .map(|item| item.as_u64().unwrap_or(0) as u8)
        .collect();
    embed::from_blob(&bytes)
}

fn parse_json_field(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => serde_json::from_str(text).unwrap_or(Value::String(text.clone())),
        Some(value) => value.clone(),
        None => Value::Null,
    }
}

fn sql_str(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
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
