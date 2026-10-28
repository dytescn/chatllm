use crate::utils::db::db_query;
use crate::utils::embedding::{embed_text, DIM};
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::time::Instant;

const DB_NAME: &str = "design_plugins";
const PAGE_SIZE: usize = 10;

#[derive(Debug, Deserialize)]
struct PluginRow {
    id: String,
    description: String,
}

fn escape_sql(s: &str) -> String {
    s.replace('\'', "''")
}

fn stable_id(uuid: &str, text: &str) -> String {
    let mut h = Sha1::new();
    h.update(uuid.as_bytes());
    h.update(b":");
    h.update(text.as_bytes());
    format!("{:x}", h.finalize())[..32].to_string()
}

fn vector_to_hex(vector: &[f32]) -> String {
    vector
        .iter()
        .flat_map(|x| x.to_le_bytes())
        .map(|b| format!("{:02x}", b))
        .collect()
}

fn parse_rows<T: for<'de> Deserialize<'de>>(rows: Vec<Value>) -> Result<Vec<T>> {
    rows.into_iter()
        .map(|r| serde_json::from_value::<T>(r).context("解析数据库行失败"))
        .collect()
}

/// 把 design_plugins 的 description 编码成 embedding 并写入 prompts。
/// 可传入 limit/offset 做分批处理；都不传时全量处理。
pub fn embed_prompts(limit: Option<usize>, offset: Option<usize>) -> Result<Value> {
    let started = Instant::now();
    let mut offset = offset.unwrap_or(0);
    let mut processed = 0usize;
    let mut inserted = 0usize;
    let mut failed = 0usize;

    loop {
        let sql = format!(
            "SELECT id, description FROM design_plugins \
             WHERE description IS NOT NULL AND description != '' \
             ORDER BY id LIMIT {PAGE_SIZE} OFFSET {offset}"
        );
        let rows = db_query(DB_NAME, &sql)
            .with_context(|| format!("查询 design_plugins 失败，offset={}", offset))?;
        let page = parse_rows::<PluginRow>(rows)?;

        let n = page.len();
        if n == 0 {
            break;
        }

        let mut reached_limit = false;
        for row in page {
            if let Some(limit) = limit {
                if processed >= limit {
                    reached_limit = true;
                    break;
                }
            }

            let vector = match embed_text(&row.description) {
                Ok(v) => v,
                Err(e) => {
                    failed += 1;
                    eprintln!("[embed_prompts] 编码失败 uuid={}: {:#}", row.id, e);
                    continue;
                }
            };

            if vector.len() != DIM {
                failed += 1;
                eprintln!(
                    "[embed_prompts] 维度异常 uuid={}: 期望 {}，实际 {}",
                    row.id,
                    DIM,
                    vector.len()
                );
                continue;
            }

            processed += 1;

            let vec_id = stable_id(&row.id, &row.description);
            let hex = vector_to_hex(&vector);
            let insert_sql = format!(
                "INSERT OR REPLACE INTO prompts (id, uuid, embedding) VALUES ('{}', '{}', X'{}')",
                escape_sql(&vec_id),
                escape_sql(&row.id),
                hex
            );

            match db_query(DB_NAME, &insert_sql) {
                Ok(_) => inserted += 1,
                Err(e) => {
                    failed += 1;
                    eprintln!("[embed_prompts] 写入失败 uuid={}: {:#}", row.id, e);
                }
            }
        }

        offset += n;

        if reached_limit {
            break;
        }

        if n < PAGE_SIZE {
            break;
        }
    }

    Ok(json!({
        "processed": processed,
        "inserted": inserted,
        "failed": failed,
        "elapsed_ms": started.elapsed().as_millis() as u64,
    }))
}
