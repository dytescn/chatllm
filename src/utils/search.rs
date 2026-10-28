use crate::utils::db::db_query;
use crate::utils::embedding::{embed_text, DIM};
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const DB_NAME: &str = "design_plugins";
const BIT_WIDTH: usize = 4;
/// 每条 embedding 的 hex 约 8KB，6 条约 49KB，低于 database.so 单次 64KB 上限。
/// 这里内部翻页读取，对外不做条数限制。
const PAGE_SIZE: usize = 6;
const DEFAULT_TOP_K: usize = 5;
const MAX_TOP_K: usize = 100;

struct SearchIndex {
    index: turbovec::IdMapIndex,
    id_to_uuid: HashMap<u64, String>,
    count: usize,
}

static SEARCH_INDEX: OnceLock<Mutex<SearchIndex>> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct PromptRow {
    uuid: String,
    embedding: String,
}

#[derive(Debug, Deserialize)]
struct PluginRow {
    id: String,
    name: Option<String>,
    soft: Option<String>,
    description: Option<String>,
}

fn decode_hex_to_f32(hex: &str) -> Result<Vec<f32>> {
    let hex = hex.trim();
    if hex.is_empty() || hex.len() % 2 != 0 {
        anyhow::bail!("非法 embedding hex 长度: {}", hex.len());
    }

    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<std::result::Result<_, _>>()
        .context("embedding hex 解析失败")?;

    if bytes.len() % 4 != 0 {
        anyhow::bail!("embedding 字节数不是 f32 的整数倍: {}", bytes.len());
    }

    let vec: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    if vec.len() != DIM {
        anyhow::bail!("embedding 维度异常: 期望 {}，实际 {}", DIM, vec.len());
    }

    Ok(vec)
}

fn parse_rows<T: for<'de> Deserialize<'de>>(rows: Vec<Value>) -> Result<Vec<T>> {
    rows.into_iter()
        .map(|r| serde_json::from_value::<T>(r).context("解析数据库行失败"))
        .collect()
}

fn load_vectors() -> Result<(Vec<f32>, Vec<u64>, HashMap<u64, String>)> {
    let mut flat: Vec<f32> = Vec::new();
    let mut external_ids: Vec<u64> = Vec::new();
    let mut id_to_uuid: HashMap<u64, String> = HashMap::new();
    let mut next_id: u64 = 0;
    let mut offset: usize = 0;

    loop {
        let sql = format!(
            "SELECT uuid, hex(embedding) AS embedding FROM prompts ORDER BY uuid LIMIT {PAGE_SIZE} OFFSET {offset}"
        );
        let rows = db_query(DB_NAME, &sql)
            .with_context(|| format!("查询 prompts 失败，offset={}", offset))?;
        let page = parse_rows::<PromptRow>(rows)?;
        let n = page.len();

        if n == 0 {
            break;
        }

        for row in page {
            let vector = decode_hex_to_f32(&row.embedding)?;
            flat.extend_from_slice(&vector);
            external_ids.push(next_id);
            id_to_uuid.insert(next_id, row.uuid);
            next_id += 1;
        }

        if n < PAGE_SIZE {
            break;
        }
        offset += PAGE_SIZE;
    }

    Ok((flat, external_ids, id_to_uuid))
}

fn get_search_index() -> Result<&'static Mutex<SearchIndex>> {
    if let Some(index) = SEARCH_INDEX.get() {
        return Ok(index);
    }

    let (flat_vectors, external_ids, id_to_uuid) = load_vectors()?;
    let count = id_to_uuid.len();
    let mut index = turbovec::IdMapIndex::new(DIM, BIT_WIDTH).context("创建 turbovec 索引失败")?;
    index
        .add_with_ids(&flat_vectors, &external_ids)
        .context("构建 turbovec 索引失败")?;

    Ok(SEARCH_INDEX.get_or_init(|| {
        Mutex::new(SearchIndex {
            index,
            id_to_uuid,
            count,
        })
    }))
}

fn fetch_plugin_info(ids: &[String]) -> Result<HashMap<String, (String, String, String)>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    let in_clause = ids
        .iter()
        .map(|id| format!("'{}'", id.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT id, name, soft, description FROM design_plugins WHERE id IN ({})",
        in_clause
    );

    let rows = db_query(DB_NAME, &sql).context("查询插件信息失败")?;
    let plugins = parse_rows::<PluginRow>(rows)?;

    Ok(plugins
        .into_iter()
        .map(|p| {
            (
                p.id,
                (
                    p.name.unwrap_or_default(),
                    p.soft.unwrap_or_default(),
                    p.description.unwrap_or_default(),
                ),
            )
        })
        .collect())
}

/// 语义检索入口：返回与 query 最相似的 top_k 条 design plugin。
pub fn search_query(query: &str, top_k: usize) -> Result<Value> {
    let query = query.trim();
    if query.is_empty() {
        anyhow::bail!("query 不能为空");
    }

    let top_k = if top_k == 0 {
        DEFAULT_TOP_K
    } else {
        top_k.min(MAX_TOP_K)
    };

    let index_ref = get_search_index()?;
    let ranked: Vec<(String, f32)> = {
        let guard = index_ref.lock().unwrap_or_else(|e| e.into_inner());
        if guard.count == 0 {
            return Ok(json!({
                "query": query,
                "top_k": top_k,
                "results": [],
            }));
        }

        let query_vector = embed_text(query)?;
        let (scores, result_ids) = guard.index.search(&query_vector, top_k);

        // 按 uuid 去重，保留最高分。
        let mut best: HashMap<String, f32> = HashMap::new();
        for (score, id) in scores.iter().zip(result_ids.iter()) {
            if let Some(uuid) = guard.id_to_uuid.get(id) {
                let entry = best.entry(uuid.clone()).or_insert(f32::MIN);
                if *score > *entry {
                    *entry = *score;
                }
            }
        }

        let mut ranked: Vec<(String, f32)> = best.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked
    };

    let uuids: Vec<String> = ranked.iter().map(|(u, _)| u.clone()).collect();
    let info = fetch_plugin_info(&uuids)?;

    let results: Vec<Value> = ranked
        .into_iter()
        .map(|(uuid, score)| {
            let (name, soft, description) = info.get(&uuid).cloned().unwrap_or_default();
            json!({
                "uuid": uuid,
                "score": score,
                "name": name,
                "soft": soft,
                "description": description,
            })
        })
        .collect();

    Ok(json!({
        "query": query,
        "top_k": top_k,
        "results": results,
    }))
}
