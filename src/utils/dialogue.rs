// src/utils/dialogue.rs
use crate::utils::db::db_query;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

const LIST_DB: &str = "dialogue";
const DEFAULT_PAGE_SIZE: i64 = 20;
const MAX_PAGE_SIZE: i64 = 100;
const DEFAULT_MSG_LIMIT: i64 = 20;
const MAX_MSG_LIMIT: i64 = 100;

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn valid_uuid(uuid: &str) -> bool {
    uuid.len() == 36
        && uuid
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn escape_sql(s: &str) -> String {
    s.replace('\'', "''")
}

fn norm_page(page: Option<i64>, page_size: Option<i64>) -> (i64, i64, i64) {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let offset = (page - 1) * page_size;
    (page, page_size, offset)
}

fn ensure_chat_table(uuid: &str) -> Result<()> {
    let sql = "CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at INTEGER NOT NULL
        )";
    db_query(uuid, sql).with_context(|| format!("建 {} 的 messages 表失败", uuid))?;
    Ok(())
}

pub fn create_dialogue(title: &str, model_name: &str, keyword: &str) -> Result<String> {
    let uuid = uuid::Uuid::new_v4().to_string();
    ensure_chat_table(&uuid)?;

    let sql = format!(
        "INSERT INTO dialogue (id, title, keyword, model_name, created_at, updated_at)
         VALUES ('{}', '{}', '{}', '{}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        escape_sql(&uuid),
        escape_sql(title),
        escape_sql(keyword),
        escape_sql(model_name)
    );
    db_query(LIST_DB, &sql).with_context(|| format!("插入 dialogue 失败, sql={}", sql))?;
    Ok(uuid)
}

pub fn append_message(uuid: &str, role: &str, content: &str) -> Result<()> {
    if !valid_uuid(uuid) {
        anyhow::bail!("非法 uuid: {}", uuid);
    }
    ensure_chat_table(uuid)?;

    let sql = format!(
        "INSERT INTO messages (role, content, created_at) VALUES ('{}', '{}', {})",
        escape_sql(role),
        escape_sql(content),
        now_ts()
    );
    db_query(uuid, &sql).with_context(|| format!("写 {} 消息失败, sql={}", uuid, sql))?;

    let upd = format!(
        "UPDATE dialogue SET updated_at = CURRENT_TIMESTAMP WHERE id = '{}'",
        escape_sql(uuid)
    );
    db_query(LIST_DB, &upd)
        .with_context(|| format!("刷新 {} updated_at 失败, sql={}", uuid, upd))?;
    Ok(())
}

pub fn update_keyword(uuid: &str, keyword: &str) -> Result<()> {
    if !valid_uuid(uuid) {
        anyhow::bail!("非法 uuid: {}", uuid);
    }
    let sql = format!(
        "UPDATE dialogue SET keyword = '{}', updated_at = CURRENT_TIMESTAMP WHERE id = '{}'",
        escape_sql(keyword),
        escape_sql(uuid)
    );
    db_query(LIST_DB, &sql)
        .with_context(|| format!("更新 {} keyword 失败, sql={}", uuid, sql))?;
    Ok(())
}

const SELECT_COLS: &str =
    "id, title, keyword, model_name, system_prompt, model_config,
     is_pinned, is_archived, created_at, updated_at";

fn count_dialogue(where_clause: &str) -> Result<i64> {
    let sql = if where_clause.is_empty() {
        "SELECT count(*) AS n FROM dialogue".to_string()
    } else {
        format!("SELECT count(*) AS n FROM dialogue WHERE {}", where_clause)
    };
    let rows =
        db_query(LIST_DB, &sql).with_context(|| format!("count dialogue 失败, sql={}", sql))?;
    let n = rows
        .first()
        .and_then(|r| r.get("n"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    Ok(n)
}

fn fetch_page(where_clause: &str, page_size: i64, offset: i64) -> Result<Vec<Value>> {
    let where_sql = if where_clause.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clause)
    };
    let sql = format!(
        "SELECT {SELECT_COLS} FROM dialogue {where_sql}
         ORDER BY is_pinned DESC, created_at DESC
         LIMIT {page_size} OFFSET {offset}"
    );
    db_query(LIST_DB, &sql).with_context(|| format!("查询 dialogue 失败, sql={}", sql))
}

fn build_page_result(list: Vec<Value>, page: i64, page_size: i64, total: i64) -> Value {
    json!({
        "list": list,
        "page": page,
        "page_size": page_size,
        "total": total,
    })
}

pub fn list_dialogue(page: Option<i64>, page_size: Option<i64>) -> Result<Value> {
    let (page, page_size, offset) = norm_page(page, page_size);
    let where_clause = "is_archived = 0";
    let total = count_dialogue(where_clause)?;
    let list = fetch_page(where_clause, page_size, offset)?;
    Ok(build_page_result(list, page, page_size, total))
}

pub fn search_dialogue(
    keyword: &str,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<Value> {
    let (page, page_size, offset) = norm_page(page, page_size);
    let kw = keyword.trim();
    let where_clause = if kw.is_empty() {
        "is_archived = 0".to_string()
    } else {
        let e = escape_sql(kw);
        format!("is_archived = 0 AND (keyword LIKE '%{e}%' OR title LIKE '%{e}%')")
    };
    let total = count_dialogue(&where_clause)?;
    let list = fetch_page(&where_clause, page_size, offset)?;
    Ok(build_page_result(list, page, page_size, total))
}

fn msg_count(uuid: &str, where_clause: &str) -> Result<i64> {
    let sql = if where_clause.is_empty() {
        "SELECT count(*) AS n FROM messages".to_string()
    } else {
        format!("SELECT count(*) AS n FROM messages WHERE {}", where_clause)
    };
    let rows =
        db_query(uuid, &sql).with_context(|| format!("count {} messages 失败", uuid))?;
    Ok(rows
        .first()
        .and_then(|r| r.get("n"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0))
}

/// 游标读消息：
/// - 都不传：返回最新 limit 条（ASC 返回）
/// - before_id：返回 id < before_id 的更早 limit 条（ASC 返回）
/// - after_id：返回 id > after_id 的更新 limit 条（ASC 返回）
pub fn list_messages(
    uuid: &str,
    before_id: Option<i64>,
    after_id: Option<i64>,
    limit: Option<i64>,
) -> Result<Value> {
    if !valid_uuid(uuid) {
        anyhow::bail!("非法 uuid: {}", uuid);
    }
    ensure_chat_table(uuid)?;

    let limit = limit.unwrap_or(DEFAULT_MSG_LIMIT).clamp(1, MAX_MSG_LIMIT);

    let (where_sql, order_sql) = match (before_id, after_id) {
        (Some(b), _) => (format!("WHERE id < {}", b), "ORDER BY id DESC"),
        (None, Some(a)) => (format!("WHERE id > {}", a), "ORDER BY id ASC"),
        (None, None) => ("".to_string(), "ORDER BY id DESC"),
    };

    let sql = format!(
        "SELECT id, role, content, created_at FROM messages {where_sql} {order_sql} LIMIT {limit}"
    );
    let mut list = db_query(uuid, &sql)
        .with_context(|| format!("查询 {} messages 失败, sql={}", uuid, sql))?;

    // 每条消息附带 Markdown → HTML（markparse），供前端直接渲染
    for row in list.iter_mut() {
        if let Some(obj) = row.as_object_mut() {
            let content = obj
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            let html = crate::utils::markparse::to_html(&content).unwrap_or_default();
            obj.insert("content_html".to_string(), Value::String(html));
        }
    }

    // 无游标 / before 时取的是 DESC，反转成 ASC 返回
    if before_id.is_some() || (before_id.is_none() && after_id.is_none()) {
        list.reverse();
    }

    let first_id = list
        .first()
        .and_then(|r| r.get("id"))
        .and_then(|v| v.as_i64());
    let last_id = list
        .last()
        .and_then(|r| r.get("id"))
        .and_then(|v| v.as_i64());

    let has_more_before = match first_id {
        Some(fid) => msg_count(uuid, &format!("id < {}", fid))? > 0,
        None => false,
    };
    let has_more_after = match last_id {
        Some(lid) => msg_count(uuid, &format!("id > {}", lid))? > 0,
        None => false,
    };

    Ok(json!({
        "list": list,
        "limit": limit,
        "first_id": first_id,
        "last_id": last_id,
        "has_more_before": has_more_before,
        "has_more_after": has_more_after,
    }))
}

pub struct DialoguePatch<'a> {
    pub title: Option<&'a str>,
    pub keyword: Option<&'a str>,
    pub model_name: Option<&'a str>,
    pub system_prompt: Option<&'a str>,
    pub model_config: Option<&'a str>,
    pub is_pinned: Option<i64>,
    pub is_archived: Option<i64>,
}

pub fn update_dialogue(id: &str, p: &DialoguePatch<'_>) -> Result<()> {
    if !valid_uuid(id) {
        anyhow::bail!("非法 uuid: {}", id);
    }

    let mut sets: Vec<String> = Vec::new();
    if let Some(v) = p.title {
        sets.push(format!("title = '{}'", escape_sql(v)));
    }
    if let Some(v) = p.keyword {
        sets.push(format!("keyword = '{}'", escape_sql(v)));
    }
    if let Some(v) = p.model_name {
        sets.push(format!("model_name = '{}'", escape_sql(v)));
    }
    if let Some(v) = p.system_prompt {
        sets.push(format!("system_prompt = '{}'", escape_sql(v)));
    }
    if let Some(v) = p.model_config {
        sets.push(format!("model_config = '{}'", escape_sql(v)));
    }
    if let Some(v) = p.is_pinned {
        sets.push(format!("is_pinned = {}", v));
    }
    if let Some(v) = p.is_archived {
        sets.push(format!("is_archived = {}", v));
    }

    if sets.is_empty() {
        return Ok(());
    }
    sets.push("updated_at = CURRENT_TIMESTAMP".to_string());

    let sql = format!(
        "UPDATE dialogue SET {} WHERE id = '{}'",
        sets.join(", "),
        escape_sql(id)
    );
    db_query(LIST_DB, &sql)
        .with_context(|| format!("更新 dialogue {} 失败, sql={}", id, sql))?;
    Ok(())
}

pub fn is_valid_uuid(uuid: &str) -> bool {
    valid_uuid(uuid)
}
