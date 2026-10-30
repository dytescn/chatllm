// src/utils/dialogue.rs
//! 对话索引（SQLite `dialogue.db`）+ 每对话消息库（`./designdb/<dialogue_id>`）。
//!
//! - `dialogue.db.dialogue`：一条一条的对话基础信息（标题、模型、session_id、show_status、
//!   last_seq、message_count 等）；
//! - `./designdb/<dialogue_id>` 的 `messages` 集合：该对话的完整消息。
//!
//! 旧实现把对话与消息都放在 `./designdb/chatllm` 单库，本文件已切换到目标结构。
//! 旧数据迁移见 `ts/migrate_dialogues.ts`。

use crate::utils::db::{db_execute, db_query, db_update};
use crate::utils::designdb;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

const MESSAGES: &str = "messages";
const DIALOGUE_DB: &str = "dialogue";

const DEFAULT_PAGE_SIZE: i64 = 20;
const MAX_PAGE_SIZE: i64 = 100;
const DEFAULT_MSG_LIMIT: i64 = 20;
const MAX_MSG_LIMIT: i64 = 100;

static SCHEMA_READY: OnceLock<()> = OnceLock::new();

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn valid_uuid(uuid: &str) -> bool {
    uuid.len() == 36 && uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn norm_page(page: Option<i64>, page_size: Option<i64>) -> (i64, i64, i64) {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let offset = (page - 1) * page_size;
    (page, page_size, offset)
}

fn esc(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn optional_str(value: Option<&str>) -> String {
    match value {
        Some(v) => esc(v),
        None => "NULL".to_string(),
    }
}

fn as_i64(value: Option<&Value>, default: i64) -> i64 {
    match value {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(default),
        Some(Value::String(s)) => s.parse::<i64>().unwrap_or(default),
        _ => default,
    }
}

/// 补齐 `dialogue.db` 的字段；旧库自动 ALTER，新库直接建表。
fn ensure_dialogue_schema() -> Result<()> {
    if SCHEMA_READY.get().is_some() {
        return Ok(());
    }

    let rows = db_query(DIALOGUE_DB, "PRAGMA table_info(dialogue)")?;
    if rows.is_empty() {
        db_update(
            DIALOGUE_DB,
            "CREATE TABLE IF NOT EXISTS dialogue (
                id TEXT PRIMARY KEY NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                keyword TEXT NOT NULL DEFAULT '',
                model_name TEXT NOT NULL DEFAULT '',
                system_prompt TEXT,
                model_config TEXT,
                parent_id TEXT,
                context_json TEXT,
                token_count INTEGER NOT NULL DEFAULT 0,
                status INTEGER NOT NULL DEFAULT 0,
                is_pinned INTEGER NOT NULL DEFAULT 0,
                is_archived INTEGER NOT NULL DEFAULT 0,
                session_id TEXT NOT NULL DEFAULT '',
                show_status INTEGER NOT NULL DEFAULT 1,
                message_count INTEGER NOT NULL DEFAULT 0,
                last_seq INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .context("创建 dialogue 表失败")?;
    } else {
        let mut columns = HashSet::new();
        for row in &rows {
            if let Some(name) = row.get("name").and_then(Value::as_str) {
                columns.insert(name.to_string());
            }
        }
        for (name, ddl) in [
            ("session_id", "TEXT NOT NULL DEFAULT ''"),
            ("show_status", "INTEGER NOT NULL DEFAULT 1"),
            ("message_count", "INTEGER NOT NULL DEFAULT 0"),
            ("last_seq", "INTEGER NOT NULL DEFAULT 0"),
            ("parent_id", "TEXT"),
            ("context_json", "TEXT"),
            ("token_count", "INTEGER NOT NULL DEFAULT 0"),
            ("status", "INTEGER NOT NULL DEFAULT 0"),
        ] {
            if !columns.contains(name) {
                db_update(
                    DIALOGUE_DB,
                    &format!("ALTER TABLE dialogue ADD COLUMN {} {}", name, ddl),
                )
                .with_context(|| format!("添加 dialogue.{} 失败", name))?;
            }
        }
    }

    let _ = SCHEMA_READY.set(());
    Ok(())
}

fn normalize_dialogue_row(mut row: Value) -> Value {
    if let Some(obj) = row.as_object_mut() {
        let model_config = obj.get("model_config").cloned();
        match model_config {
            Some(Value::String(s)) if !s.is_empty() => match serde_json::from_str::<Value>(&s) {
                Ok(v) => {
                    obj.insert("model_config".to_string(), v);
                }
                Err(_) => {
                    obj.insert("model_config_raw".to_string(), Value::String(s));
                    obj.insert("model_config".to_string(), json!({}));
                }
            },
            Some(Value::Null) | None => {
                obj.insert("model_config".to_string(), json!({}));
            }
            _ => {}
        }
        if !obj.contains_key("system_prompt") {
            obj.insert("system_prompt".to_string(), Value::Null);
        }
        if !obj.contains_key("session_id") {
            obj.insert("session_id".to_string(), json!(""));
        }
        if !obj.contains_key("show_status") {
            obj.insert("show_status".to_string(), json!(1));
        }
        if !obj.contains_key("message_count") {
            obj.insert("message_count".to_string(), json!(0));
        }
        if !obj.contains_key("last_seq") {
            obj.insert("last_seq".to_string(), json!(0));
        }
        obj.remove("skills_json");
    }
    row
}

fn normalize_message(mut doc: Value) -> Value {
    if let Some(obj) = doc.as_object_mut() {
        if let Some(uuid) = obj.remove("_id") {
            obj.insert("uuid".to_string(), uuid);
        }
        if let Some(seq) = obj.get("seq").cloned() {
            obj.insert("id".to_string(), seq);
        }
    }
    doc
}

struct DialoguePage {
    page: i64,
    page_size: i64,
    offset: i64,
}

fn dialogue_page(page: Option<i64>, page_size: Option<i64>) -> DialoguePage {
    let (page, page_size, offset) = norm_page(page, page_size);
    DialoguePage {
        page,
        page_size,
        offset,
    }
}

fn page_result(list: Vec<Value>, page: &DialoguePage, total: u64) -> Value {
    json!({
        "list": list,
        "page": page.page,
        "page_size": page.page_size,
        "total": total,
    })
}

fn query_total(where_clause: &str) -> Result<u64> {
    let rows = db_query(
        DIALOGUE_DB,
        &format!("SELECT count(*) AS n FROM dialogue WHERE {}", where_clause),
    )?;
    Ok(rows
        .first()
        .and_then(|row| row.get("n"))
        .map(|v| as_i64(Some(v), 0) as u64)
        .unwrap_or(0))
}

// ---------------------------------------------------------------------------
// dialogues
// ---------------------------------------------------------------------------

pub fn create_dialogue(
    title: &str,
    model_name: &str,
    keyword: &str,
    system_prompt: Option<&str>,
) -> Result<String> {
    create_dialogue_with(None, title, model_name, keyword, system_prompt, 1)
}

/// 新建对话，可指定 `dialogue.id`（如 `plugin_uuid`）与 `show_status`。
///
/// `show_status = 2` 的对话不会出现在 `list_dialogue` 的普通列表中。
pub fn create_dialogue_with(
    id: Option<&str>,
    title: &str,
    model_name: &str,
    keyword: &str,
    system_prompt: Option<&str>,
    show_status: i64,
) -> Result<String> {
    ensure_dialogue_schema()?;
    let uuid = match id {
        Some(id) => {
            if !valid_uuid(id) {
                bail!("非法对话 id: {}", id);
            }
            id.to_string()
        }
        None => uuid::Uuid::new_v4().to_string(),
    };
    let sql = format!(
        "INSERT INTO dialogue
         (id, title, keyword, model_name, system_prompt, model_config,
          is_pinned, is_archived, session_id, show_status, message_count, last_seq,
          created_at, updated_at)
         VALUES ({}, {}, {}, {}, {}, NULL, 0, 0, '', {}, 0, 0, datetime('now'), datetime('now'))",
        esc(&uuid),
        esc(title),
        esc(keyword),
        esc(model_name),
        optional_str(system_prompt),
        show_status,
    );
    db_execute(DIALOGUE_DB, &sql).with_context(|| format!("插入对话 {} 失败", uuid))?;
    Ok(uuid)
}

pub fn get_dialogue(id: &str) -> Result<Option<Value>> {
    if !valid_uuid(id) {
        bail!("非法 uuid: {}", id);
    }
    ensure_dialogue_schema()?;
    let rows = db_query(
        DIALOGUE_DB,
        &format!("SELECT * FROM dialogue WHERE id = {} LIMIT 1", esc(id)),
    )?;
    Ok(rows.into_iter().next().map(normalize_dialogue_row))
}

pub fn append_message(uuid: &str, role: &str, content: &str) -> Result<()> {
    if !valid_uuid(uuid) {
        bail!("非法 uuid: {}", uuid);
    }
    ensure_dialogue_schema()?;

    let rows = db_query(
        DIALOGUE_DB,
        &format!(
            "SELECT last_seq, message_count FROM dialogue WHERE id = {} LIMIT 1",
            esc(uuid)
        ),
    )?;
    let Some(current) = rows.first() else {
        bail!("对话不存在: {}", uuid);
    };

    let last_seq = as_i64(current.get("last_seq"), 0);
    let seq = last_seq + 1;
    let now = now_ms();
    let message_id = uuid::Uuid::new_v4().to_string();

    let doc = json!({
        "_id": message_id,
        "dialogue_id": uuid,
        "seq": seq,
        "role": role,
        "content": content,
        "created_at": now,
    });
    designdb::insert(uuid, MESSAGES, doc).with_context(|| format!("写 {} 消息失败", uuid))?;

    db_update(
        DIALOGUE_DB,
        &format!(
            "UPDATE dialogue
             SET last_seq = {}, message_count = message_count + 1, updated_at = datetime('now')
             WHERE id = {}",
            seq,
            esc(uuid)
        ),
    )
    .with_context(|| format!("刷新 {} 计数失败", uuid))?;

    Ok(())
}

pub fn update_keyword(uuid: &str, keyword: &str) -> Result<()> {
    if !valid_uuid(uuid) {
        bail!("非法 uuid: {}", uuid);
    }
    update_dialogue(
        uuid,
        &DialoguePatch {
            title: None,
            keyword: Some(keyword),
            model_name: None,
            system_prompt: None,
            model_config: None,
            is_pinned: None,
            is_archived: None,
            session_id: None,
            show_status: None,
        },
    )
}

pub fn list_dialogue(page: Option<i64>, page_size: Option<i64>) -> Result<Value> {
    ensure_dialogue_schema()?;
    let page = dialogue_page(page, page_size);
    let where_clause = "is_archived = 0 AND show_status != 2";
    let total = query_total(where_clause)?;
    let rows = db_query(
        DIALOGUE_DB,
        &format!(
            "SELECT * FROM dialogue WHERE {}
             ORDER BY is_pinned DESC, updated_at DESC
             LIMIT {} OFFSET {}",
            where_clause, page.page_size, page.offset
        ),
    )?;
    let list = rows.into_iter().map(normalize_dialogue_row).collect();
    Ok(page_result(list, &page, total))
}

pub fn search_dialogue(keyword: &str, page: Option<i64>, page_size: Option<i64>) -> Result<Value> {
    ensure_dialogue_schema()?;
    let page = dialogue_page(page, page_size);
    let needle = keyword.trim();
    if needle.is_empty() {
        return list_dialogue(Some(page.page), Some(page.page_size));
    }

    let needle = esc(needle);
    let where_clause = format!(
        "is_archived = 0 AND show_status != 2
         AND (instr(lower(title), lower({})) > 0 OR instr(lower(keyword), lower({})) > 0)",
        needle, needle
    );
    let total = query_total(&where_clause)?;
    let rows = db_query(
        DIALOGUE_DB,
        &format!(
            "SELECT * FROM dialogue WHERE {}
             ORDER BY is_pinned DESC, updated_at DESC
             LIMIT {} OFFSET {}",
            where_clause, page.page_size, page.offset
        ),
    )?;
    let list = rows.into_iter().map(normalize_dialogue_row).collect();
    Ok(page_result(list, &page, total))
}

pub struct DialoguePatch<'a> {
    pub title: Option<&'a str>,
    pub keyword: Option<&'a str>,
    pub model_name: Option<&'a str>,
    pub system_prompt: Option<&'a str>,
    pub model_config: Option<&'a str>,
    pub is_pinned: Option<i64>,
    pub is_archived: Option<i64>,
    pub session_id: Option<&'a str>,
    pub show_status: Option<i64>,
}

pub fn update_dialogue(id: &str, p: &DialoguePatch<'_>) -> Result<()> {
    if !valid_uuid(id) {
        bail!("非法 uuid: {}", id);
    }
    ensure_dialogue_schema()?;

    let mut sets: Vec<String> = Vec::new();
    if let Some(v) = p.title {
        sets.push(format!("title = {}", esc(v)));
    }
    if let Some(v) = p.keyword {
        sets.push(format!("keyword = {}", esc(v)));
    }
    if let Some(v) = p.model_name {
        sets.push(format!("model_name = {}", esc(v)));
    }
    if let Some(v) = p.system_prompt {
        sets.push(format!("system_prompt = {}", esc(v)));
    }
    if let Some(v) = p.model_config {
        sets.push(format!("model_config = {}", esc(v)));
    }
    if let Some(v) = p.is_pinned {
        sets.push(format!("is_pinned = {}", v));
    }
    if let Some(v) = p.is_archived {
        sets.push(format!("is_archived = {}", v));
    }
    if let Some(v) = p.session_id {
        sets.push(format!("session_id = {}", esc(v)));
    }
    if let Some(v) = p.show_status {
        sets.push(format!("show_status = {}", v));
    }

    if sets.is_empty() {
        return Ok(());
    }
    sets.push("updated_at = datetime('now')".to_string());

    let sql = format!(
        "UPDATE dialogue SET {} WHERE id = {}",
        sets.join(", "),
        esc(id)
    );
    db_update(DIALOGUE_DB, &sql).with_context(|| format!("更新对话 {} 失败", id))?;
    Ok(())
}

/// 更新模型会话 ID（模型响应 `id`）。
pub fn update_session_id(id: &str, session_id: &str) -> Result<()> {
    update_dialogue(
        id,
        &DialoguePatch {
            title: None,
            keyword: None,
            model_name: None,
            system_prompt: None,
            model_config: None,
            is_pinned: None,
            is_archived: None,
            session_id: Some(session_id),
            show_status: None,
        },
    )
}

/// 删除对话及其全部消息（先删消息库，再删索引）。
pub fn delete_dialogue(id: &str) -> Result<()> {
    if !valid_uuid(id) {
        bail!("非法 uuid: {}", id);
    }
    designdb::delete(id, MESSAGES, Some(json!({})))?;
    db_update(
        DIALOGUE_DB,
        &format!("DELETE FROM dialogue WHERE id = {}", esc(id)),
    )
    .with_context(|| format!("删除对话 {} 失败", id))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// messages
// ---------------------------------------------------------------------------

/// 读取某对话的全部消息（按 seq 升序），供续聊组装上下文。
pub fn message_history(uuid: &str) -> Result<Vec<Value>> {
    if !valid_uuid(uuid) {
        bail!("非法 uuid: {}", uuid);
    }
    let mut all = designdb::query_all(uuid, MESSAGES, Some(json!({})), Some(json!({ "seq": 1 })))?;
    all.sort_by_key(|m| as_i64(m.get("seq"), 0));
    Ok(all)
}

pub fn list_messages(
    uuid: &str,
    before_id: Option<i64>,
    after_id: Option<i64>,
    limit: Option<i64>,
) -> Result<Value> {
    if !valid_uuid(uuid) {
        bail!("非法 uuid: {}", uuid);
    }
    let limit = limit.unwrap_or(DEFAULT_MSG_LIMIT).clamp(1, MAX_MSG_LIMIT);
    let all = message_history(uuid)?;

    let seq_of = |m: &Value| as_i64(m.get("seq"), 0);
    let has_more_before;
    let has_more_after;

    let selected: Vec<Value> = if let Some(before) = before_id {
        let candidates: Vec<Value> = all.iter().filter(|m| seq_of(m) < before).cloned().collect();
        has_more_before = candidates.len() as i64 > limit;
        has_more_after = all.iter().any(|m| seq_of(m) >= before);
        let start = candidates.len().saturating_sub(limit as usize);
        candidates[start..].to_vec()
    } else if let Some(after) = after_id {
        let candidates: Vec<Value> = all.iter().filter(|m| seq_of(m) > after).cloned().collect();
        has_more_after = candidates.len() as i64 > limit;
        has_more_before = all.iter().any(|m| seq_of(m) <= after);
        candidates.into_iter().take(limit as usize).collect()
    } else {
        has_more_before = all.len() as i64 > limit;
        has_more_after = false;
        let start = all.len().saturating_sub(limit as usize);
        all[start..].to_vec()
    };

    let mut list: Vec<Value> = selected
        .into_iter()
        .map(|mut row| {
            if let Some(obj) = row.as_object_mut() {
                let content = obj
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let html = crate::utils::markparse::to_html(&content).unwrap_or_default();
                obj.insert("content_html".to_string(), Value::String(html));
            }
            normalize_message(row)
        })
        .collect();
    list.sort_by_key(|m| as_i64(m.get("id"), 0));

    let first_id = list
        .first()
        .and_then(|r| r.get("id"))
        .and_then(Value::as_i64);
    let last_id = list
        .last()
        .and_then(|r| r.get("id"))
        .and_then(Value::as_i64);

    Ok(json!({
        "list": list,
        "limit": limit,
        "first_id": first_id,
        "last_id": last_id,
        "has_more_before": has_more_before,
        "has_more_after": has_more_after,
    }))
}

pub fn is_valid_uuid(uuid: &str) -> bool {
    valid_uuid(uuid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_escape_quotes() {
        assert_eq!(esc("a'b"), "'a''b'");
        assert_eq!(esc("abc"), "'abc'");
    }

    #[test]
    fn page_normalization() {
        assert_eq!(norm_page(None, None), (1, 20, 0));
        assert_eq!(norm_page(Some(2), Some(15)), (2, 15, 15));
        assert_eq!(norm_page(Some(0), Some(500)), (1, 100, 0));
    }

    #[test]
    fn normalize_dialogue_defaults() {
        let row = json!({ "id": "x", "model_config": null });
        let out = normalize_dialogue_row(row);
        assert_eq!(out["session_id"], "");
        assert_eq!(out["show_status"], 1);
        assert_eq!(out["message_count"], 0);
        assert_eq!(out["last_seq"], 0);
        assert_eq!(out["model_config"], json!({}));
    }
}
