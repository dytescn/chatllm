//! 分类向量分片存储。
//!
//! 元数据在 `dialogue.db`：
//!   - `embedding_category`：分类清单
//!   - `embedding_shard`：分片注册（category + shard_no → 逻辑库名）
//!
//! 向量数据在 `database/embedding_<category>_<n>.db` 的 `prompts_embedding` 表。
//! 单分片默认 16 MB；超过阈值后封存当前片并创建下一片。检索遍历分类下全部分片，
//! 合并 top_k，调用方不感知分片。

use crate::utils::{db, embed};
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

const META_DB: &str = "dialogue";
const SHARD_TABLE: &str = "prompts_embedding";
const DEFAULT_SHARD_MAX_MB: i64 = 16;
const READ_PAGE: usize = 2; // 4096 字节向量经 JSON 数组返回后接近 64 KiB 上限
const CANDIDATE_BATCH: usize = 500; // SQLite IN 变量上限内

static LEGACY_MIGRATED: OnceLock<()> = OnceLock::new();

pub const CATEGORY_PLUGIN: &str = "plugin";

#[derive(Debug, Clone)]
struct Shard {
    path: String,
    shard_no: i64,
    state: i64,
}

/// 建分类 / 分片元数据表，写入初始分类。幂等。
pub fn ensure_meta() -> Result<()> {
    db::db_execute(
        META_DB,
        "CREATE TABLE IF NOT EXISTS embedding_category (
            code        TEXT PRIMARY KEY NOT NULL,
            name        TEXT NOT NULL DEFAULT '',
            description TEXT NOT NULL DEFAULT '',
            sort        INTEGER NOT NULL DEFAULT 0,
            created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .context("创建 embedding_category 失败")?;

    db::db_execute(
        META_DB,
        "CREATE TABLE IF NOT EXISTS embedding_shard (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            category   TEXT NOT NULL,
            shard_no   INTEGER NOT NULL,
            path       TEXT NOT NULL UNIQUE,
            row_count  INTEGER NOT NULL DEFAULT 0,
            size_bytes INTEGER NOT NULL DEFAULT 0,
            state      INTEGER NOT NULL DEFAULT 0,
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(category, shard_no)
        )",
    )
    .context("创建 embedding_shard 失败")?;

    db::db_execute(
        META_DB,
        "CREATE INDEX IF NOT EXISTS idx_embedding_shard_active
         ON embedding_shard(category, state)",
    )
    .context("创建 embedding_shard 索引失败")?;

    for (code, name, sort) in [
        ("plugin", "插件能力", 10),
        ("prompt", "提示词", 20),
        ("skill", "技能", 30),
        ("material", "素材", 40),
    ] {
        db::db_execute(
            META_DB,
            &format!(
                "INSERT OR IGNORE INTO embedding_category (code, name, sort) VALUES ({}, {}, {sort})",
                sql_str(code),
                sql_str(name)
            ),
        )
        .with_context(|| format!("写入分类 {code} 失败"))?;
    }

    Ok(())
}

/// 首次调用时把旧 `dialogue.plugin_embeddings` 迁移到分类分片。
/// 仅当分片注册表为空时尝试；旧表不存在则跳过。
pub fn migrate_legacy_once() -> Result<()> {
    if LEGACY_MIGRATED.get().is_some() {
        return Ok(());
    }

    let count = db::db_query(META_DB, "SELECT COUNT(*) AS n FROM embedding_shard")?
        .first()
        .and_then(|row| row.get("n"))
        .and_then(Value::as_i64)
        .unwrap_or(0);

    if count == 0 {
        match db::db_query(
            META_DB,
            "SELECT function_uuid, dim, vector, text_hash FROM plugin_embeddings \
             WHERE model = 'qwen3-embed'",
        ) {
            Ok(rows) => {
                let mut migrated = 0usize;
                for row in rows {
                    let Some(uuid) = row.get("function_uuid").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(dim) = row.get("dim").and_then(Value::as_i64) else {
                        continue;
                    };
                    let Some(hash) = row.get("text_hash").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(vector) = value_to_vector(row.get("vector")) else {
                        continue;
                    };
                    if dim <= 0 {
                        continue;
                    }
                    upsert(CATEGORY_PLUGIN, uuid, dim as usize, &vector, hash)?;
                    migrated += 1;
                }
                if migrated > 0 {
                    eprintln!("[embedding_store] 已迁移旧 plugin_embeddings {migrated} 条");
                }
            }
            Err(error) => {
                eprintln!("[embedding_store] 旧 plugin_embeddings 不可读，跳过迁移: {error:#}");
            }
        }
    }

    let _ = LEGACY_MIGRATED.set(());
    Ok(())
}

/// 写入 / 更新一条向量。自动选择 active 分片；跨 active / sealed 查重。
pub fn upsert(
    category: &str,
    uuid: &str,
    dim: usize,
    vector: &[f32],
    text_hash: &str,
) -> Result<()> {
    validate_category(category)?;
    ensure_meta()?;

    let mut target = match active_shard(category)? {
        Some(shard) => shard,
        None => create_shard(category, next_shard_no(category)?)?,
    };
    let mut exists = row_exists(&target.path, uuid)?;

    if !exists {
        for shard in sealed_shards(category)? {
            if row_exists(&shard.path, uuid)? {
                target = shard;
                exists = true;
                break;
            }
        }
    }

    let blob = embed::hex(&embed::to_blob(vector));
    if exists {
        db::db_update(
            &target.path,
            &format!(
                "UPDATE {SHARD_TABLE} SET category = {}, dim = {dim}, vector = X'{}', text_hash = {} WHERE uuid = {}",
                sql_str(category),
                blob,
                sql_str(text_hash),
                sql_str(uuid)
            ),
        )
        .with_context(|| format!("更新 {} 失败", target.path))?;
    } else {
        db::db_execute(
            &target.path,
            &format!(
                "INSERT INTO {SHARD_TABLE} (uuid, category, dim, vector, text_hash) \
                 VALUES ({}, {}, {dim}, X'{}', {})",
                sql_str(uuid),
                sql_str(category),
                blob,
                sql_str(text_hash)
            ),
        )
        .with_context(|| format!("写入 {} 失败", target.path))?;
    }

    update_stats(&target.path)?;

    if target.state == 0 {
        maybe_roll(category, &target)?;
    }

    Ok(())
}

/// 读取一条向量的 text_hash（active / sealed 中查找）。
pub fn existing_hash(category: &str, uuid: &str) -> Result<Option<String>> {
    validate_category(category)?;
    ensure_meta()?;

    for shard in all_readable_shards(category)? {
        let rows = db::db_query(
            &shard.path,
            &format!(
                "SELECT text_hash FROM {SHARD_TABLE} WHERE uuid = {} LIMIT 1",
                sql_str(uuid)
            ),
        )
        .with_context(|| format!("读取 {} 失败", shard.path))?;
        if let Some(hash) = rows
            .first()
            .and_then(|row| row.get("text_hash"))
            .and_then(Value::as_str)
        {
            return Ok(Some(hash.to_string()));
        }
    }
    Ok(None)
}

/// 在分类下检索。分片遍历顺序固定，结果按分数降序、同分 uuid 升序。
/// `candidate_uuids` 非空时只在这些 uuid 中检索。
pub fn search(
    category: &str,
    query: &[f32],
    top_k: usize,
    min_score: f32,
    candidate_uuids: Option<&[String]>,
) -> Result<Vec<(String, f32)>> {
    validate_category(category)?;
    ensure_meta()?;

    let shards = all_readable_shards(category)?;
    if shards.is_empty() {
        return Ok(Vec::new());
    }

    let mut best: HashMap<String, f32> = HashMap::new();
    let mut failed = 0usize;

    for shard in &shards {
        match search_shard(category, shard, query, candidate_uuids) {
            Ok(rows) => {
                for (uuid, score) in rows {
                    let entry = best.entry(uuid).or_insert(score);
                    if score > *entry {
                        *entry = score;
                    }
                }
            }
            Err(error) => {
                failed += 1;
                eprintln!("[embedding_store] {} 读取失败: {error:#}", shard.path);
                if !shard_file_exists(&shard.path) {
                    let _ = mark_broken(&shard.path);
                }
            }
        }
    }

    if failed == shards.len() {
        anyhow::bail!("database 读取分片失败：{failed}/{}", shards.len());
    }

    let mut scored: Vec<(String, f32)> = best.into_iter().collect();
    scored.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    scored.retain(|(_, score)| *score >= min_score);
    scored.truncate(top_k);
    Ok(scored)
}

/// 删除分类下指定 uuid 的向量（active / sealed 全部分片）。
pub fn delete(category: &str, uuids: &[String]) -> Result<usize> {
    validate_category(category)?;
    ensure_meta()?;

    if uuids.is_empty() {
        return Ok(0);
    }

    let mut removed = 0usize;
    for shard in all_readable_shards(category)? {
        for chunk in uuids.chunks(CANDIDATE_BATCH) {
            let in_list = chunk
                .iter()
                .map(|uuid| sql_str(uuid))
                .collect::<Vec<_>>()
                .join(",");
            let count_sql = format!(
                "SELECT COUNT(*) AS n FROM {SHARD_TABLE} \
                 WHERE category = {} AND uuid IN ({in_list})",
                sql_str(category)
            );
            let count = db::db_query(&shard.path, &count_sql)?
                .first()
                .and_then(|row| row.get("n"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            db::db_execute(
                &shard.path,
                &format!(
                    "DELETE FROM {SHARD_TABLE} WHERE category = {} AND uuid IN ({in_list})",
                    sql_str(category)
                ),
            )?;
            removed += count.max(0) as usize;
        }
        update_stats(&shard.path)?;
    }
    Ok(removed)
}

fn search_shard(
    category: &str,
    shard: &Shard,
    query: &[f32],
    candidates: Option<&[String]>,
) -> Result<Vec<(String, f32)>> {
    let mut rows: Vec<Value> = Vec::new();

    match candidates {
        Some(uuids) if !uuids.is_empty() => {
            for chunk in uuids.chunks(CANDIDATE_BATCH) {
                let in_list = chunk
                    .iter()
                    .map(|uuid| sql_str(uuid))
                    .collect::<Vec<_>>()
                    .join(",");
                let mut offset = 0usize;
                loop {
                    let page = db::db_query(
                        &shard.path,
                        &format!(
                            "SELECT uuid, vector FROM {SHARD_TABLE} \
                             WHERE category = {} AND uuid IN ({in_list}) \
                             LIMIT {READ_PAGE} OFFSET {offset}",
                            sql_str(category)
                        ),
                    )?;
                    let count = page.len();
                    rows.extend(page);
                    if count < READ_PAGE {
                        break;
                    }
                    offset += count;
                }
            }
        }
        _ => {
            let mut offset = 0usize;
            loop {
                let page = db::db_query(
                    &shard.path,
                    &format!(
                        "SELECT uuid, vector FROM {SHARD_TABLE} WHERE category = {} \
                         LIMIT {READ_PAGE} OFFSET {offset}",
                        sql_str(category)
                    ),
                )?;
                let count = page.len();
                rows.extend(page);
                if count < READ_PAGE {
                    break;
                }
                offset += count;
            }
        }
    }

    let mut scored = Vec::new();
    for row in &rows {
        let Some(uuid) = row.get("uuid").and_then(Value::as_str) else {
            continue;
        };
        let Some(vector) = value_to_vector(row.get("vector")) else {
            continue;
        };
        scored.push((uuid.to_string(), embed::cosine(query, &vector)));
    }
    Ok(scored)
}

fn active_shard(category: &str) -> Result<Option<Shard>> {
    let rows = db::db_query(
        META_DB,
        &format!(
            "SELECT path, shard_no, state FROM embedding_shard \
             WHERE category = {} AND state = 0 ORDER BY shard_no DESC LIMIT 1",
            sql_str(category)
        ),
    )?;
    Ok(rows.first().and_then(row_to_shard))
}

fn sealed_shards(category: &str) -> Result<Vec<Shard>> {
    let rows = db::db_query(
        META_DB,
        &format!(
            "SELECT path, shard_no, state FROM embedding_shard \
             WHERE category = {} AND state = 1 ORDER BY shard_no",
            sql_str(category)
        ),
    )?;
    Ok(rows.iter().filter_map(row_to_shard).collect())
}

fn all_readable_shards(category: &str) -> Result<Vec<Shard>> {
    let rows = db::db_query(
        META_DB,
        &format!(
            "SELECT path, shard_no, state FROM embedding_shard \
             WHERE category = {} AND state != 2 ORDER BY shard_no",
            sql_str(category)
        ),
    )?;
    Ok(rows.iter().filter_map(row_to_shard).collect())
}

fn row_to_shard(row: &Value) -> Option<Shard> {
    Some(Shard {
        path: row.get("path")?.as_str()?.to_string(),
        shard_no: row.get("shard_no")?.as_i64()?,
        state: row.get("state").and_then(Value::as_i64).unwrap_or(0),
    })
}

fn next_shard_no(category: &str) -> Result<i64> {
    let rows = db::db_query(
        META_DB,
        &format!(
            "SELECT COALESCE(MAX(shard_no), -1) + 1 AS next FROM embedding_shard WHERE category = {}",
            sql_str(category)
        ),
    )?;
    Ok(rows
        .first()
        .and_then(|row| row.get("next"))
        .and_then(Value::as_i64)
        .unwrap_or(0))
}

fn create_shard(category: &str, shard_no: i64) -> Result<Shard> {
    let path = format!("embedding_{category}_{shard_no}");
    db::db_execute(
        META_DB,
        &format!(
            "INSERT INTO embedding_shard (category, shard_no, path, row_count, size_bytes, state) \
             VALUES ({}, {shard_no}, {}, 0, 0, 0)",
            sql_str(category),
            sql_str(&path)
        ),
    )
    .with_context(|| format!("注册分片 {path} 失败"))?;

    ensure_shard_table(&path)?;
    Ok(Shard {
        path,
        shard_no,
        state: 0,
    })
}

fn ensure_shard_table(path: &str) -> Result<()> {
    db::db_execute(
        path,
        &format!(
            "CREATE TABLE IF NOT EXISTS {SHARD_TABLE} (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                uuid      TEXT NOT NULL,
                category  TEXT NOT NULL,
                dim       INTEGER NOT NULL,
                vector    BLOB NOT NULL,
                text_hash TEXT NOT NULL,
                UNIQUE(uuid)
            )"
        ),
    )
    .with_context(|| format!("创建 {path}.{SHARD_TABLE} 失败"))?;
    db::db_execute(
        path,
        &format!(
            "CREATE INDEX IF NOT EXISTS idx_prompts_embedding_hash ON {SHARD_TABLE}(text_hash)"
        ),
    )?;
    db::db_execute(
        path,
        &format!(
            "CREATE INDEX IF NOT EXISTS idx_prompts_embedding_cat ON {SHARD_TABLE}(category, id)"
        ),
    )?;
    Ok(())
}

fn row_exists(path: &str, uuid: &str) -> Result<bool> {
    let rows = db::db_query(
        path,
        &format!(
            "SELECT id FROM {SHARD_TABLE} WHERE uuid = {} LIMIT 1",
            sql_str(uuid)
        ),
    )?;
    Ok(!rows.is_empty())
}

fn update_stats(path: &str) -> Result<()> {
    let rows = db::db_query(path, &format!("SELECT COUNT(*) AS n FROM {SHARD_TABLE}"))?;
    let row_count = rows
        .first()
        .and_then(|row| row.get("n"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let size_bytes = shard_file_size(path);
    db::db_update(
        META_DB,
        &format!(
            "UPDATE embedding_shard SET row_count = {row_count}, size_bytes = {size_bytes}, \
             updated_at = CURRENT_TIMESTAMP WHERE path = {}",
            sql_str(path)
        ),
    )?;
    Ok(())
}

fn maybe_roll(category: &str, active: &Shard) -> Result<()> {
    let max_mb = shard_max_mb();
    if max_mb <= 0 {
        return Ok(());
    }
    let size_bytes = shard_file_size(&active.path);
    if size_bytes < max_mb.saturating_mul(1024 * 1024) {
        return Ok(());
    }

    db::db_update(
        META_DB,
        &format!(
            "UPDATE embedding_shard SET state = 1, updated_at = CURRENT_TIMESTAMP WHERE path = {}",
            sql_str(&active.path)
        ),
    )?;

    if let Err(error) = create_shard(category, active.shard_no + 1) {
        // 回滚封存状态，保留原片可写。
        let _ = db::db_update(
            META_DB,
            &format!(
                "UPDATE embedding_shard SET state = 0, updated_at = CURRENT_TIMESTAMP WHERE path = {}",
                sql_str(&active.path)
            ),
        );
        return Err(error).context("滚动分片失败");
    }
    Ok(())
}

fn mark_broken(path: &str) -> Result<()> {
    db::db_update(
        META_DB,
        &format!(
            "UPDATE embedding_shard SET state = 2, updated_at = CURRENT_TIMESTAMP WHERE path = {}",
            sql_str(path)
        ),
    )?;
    Ok(())
}

fn shard_max_mb() -> i64 {
    std::env::var("CHATLLM_EMBEDDING_SHARD_MAX_MB")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_SHARD_MAX_MB)
        .max(0)
}

fn shard_file_size(path: &str) -> i64 {
    std::fs::metadata(shard_file_path(path))
        .map(|meta| meta.len() as i64)
        .unwrap_or(0)
}

fn shard_file_exists(path: &str) -> bool {
    Path::new(&shard_file_path(path)).exists()
}

fn shard_file_path(path: &str) -> String {
    format!("./database/{path}.db")
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

fn validate_category(category: &str) -> Result<()> {
    if category.is_empty()
        || !category
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        anyhow::bail!("非法 category: {category}");
    }
    Ok(())
}

fn sql_str(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
