//! 素材内容向量：material_embed / material_search。
//!
//! 与 plugin_embed 共用 embedding_store 的分类分片、16 MB 滚动与跨分片合并；
//! 素材侧不依赖 plugin_lib，只接收调用方给出的 uuid / text。

use crate::utils::{embed, embedding_store};
use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const CATEGORY: &str = "material";
const MAX_ITEMS: usize = 500;

#[derive(Debug, Default, Deserialize)]
pub struct EmbedItem {
    #[serde(default)]
    pub uuid: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub text_hash: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct EmbedReq {
    #[serde(default)]
    pub items: Vec<EmbedItem>,
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

#[derive(Debug, Default, Deserialize)]
pub struct DeleteReq {
    #[serde(default)]
    pub uuids: Vec<String>,
}

pub fn embed(req: EmbedReq) -> Result<Value> {
    embedding_store::ensure_meta()?;

    if req.items.is_empty() {
        anyhow::bail!("items 不能为空");
    }

    let force = req.force.unwrap_or(false);
    let mut processed = 0;
    let mut skipped = 0;
    let mut failed = 0;

    for item in req.items.into_iter().take(MAX_ITEMS) {
        let uuid = item.uuid.trim();
        if uuid.is_empty() || item.text.trim().is_empty() {
            failed += 1;
            continue;
        }

        let hash = item
            .text_hash
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| sha256_hex(&item.text));

        if !force && embedding_store::existing_hash(CATEGORY, uuid)? == Some(hash.clone()) {
            skipped += 1;
            continue;
        }

        match embed_and_store(uuid, &item.text, &hash) {
            Ok(()) => processed += 1,
            Err(error) => {
                eprintln!("[material_embed] {uuid}: {error:#}");
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
    let scored = embedding_store::search(CATEGORY, &query_vector, top_k, min_score, None)?;

    let list = scored
        .into_iter()
        .map(|(uuid, score)| json!({ "uuid": uuid, "score": score }))
        .collect::<Vec<_>>();

    Ok(json!({ "model": model, "dim": embed::DIM, "list": list }))
}

pub fn delete(req: DeleteReq) -> Result<Value> {
    embedding_store::ensure_meta()?;

    let uuids = req
        .uuids
        .into_iter()
        .filter(|uuid| !uuid.trim().is_empty())
        .collect::<Vec<_>>();
    if uuids.is_empty() {
        anyhow::bail!("uuids 不能为空");
    }

    let requested = uuids.len();
    let deleted = embedding_store::delete(CATEGORY, &uuids)?;
    Ok(json!({ "deleted": deleted, "requested": requested }))
}

fn embed_and_store(uuid: &str, text: &str, hash: &str) -> Result<()> {
    let vector = embed::embed_text(text)?;
    embedding_store::upsert(CATEGORY, uuid, embed::DIM, &vector, hash)
}

fn sha256_hex(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    format!("{digest:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_distinct() {
        assert_eq!(sha256_hex("素材"), sha256_hex("素材"));
        assert_ne!(sha256_hex("素材"), sha256_hex("素材2"));
        assert_eq!(sha256_hex("素材").len(), 64);
    }

    #[test]
    fn embed_request_parses_items() {
        let req: EmbedReq = serde_json::from_str(
            r#"{"items":[{"uuid":"m1","text":"蓝色背景"},{"uuid":"m2","text":"logo","text_hash":"h"}]}"#,
        )
        .unwrap();
        assert_eq!(req.items.len(), 2);
        assert_eq!(req.items[1].text_hash.as_deref(), Some("h"));
    }

    #[test]
    fn delete_request_parses_uuids() {
        let req: DeleteReq = serde_json::from_str(r#"{"uuids":["m1","m2"]}"#).unwrap();
        assert_eq!(req.uuids, vec!["m1".to_string(), "m2".to_string()]);
    }
}
