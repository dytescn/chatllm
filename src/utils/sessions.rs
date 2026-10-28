use crate::utils::db::db_query;
use anyhow::{Context, Result};
use serde_json::{json, Value};

const DB_NAME: &str = "sessions";
const DEFAULT_PAGE_SIZE: i64 = 20;
const MAX_PAGE_SIZE: i64 = 100;
const SELECT_COLS: &str = "id, title, model, system_prompt, created_at, updated_at";

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

pub fn list_sessions(page: Option<i64>, page_size: Option<i64>) -> Result<Value> {
    let (page, page_size, offset) = norm_page(page, page_size);

    let count_rows =
        db_query(DB_NAME, "SELECT count(*) AS n FROM sessions").context("统计 sessions 失败")?;
    let total = count_rows
        .first()
        .and_then(|r| r.get("n"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let sql = format!(
        "SELECT {SELECT_COLS} FROM sessions ORDER BY updated_at DESC LIMIT {page_size} OFFSET {offset}"
    );
    let list = db_query(DB_NAME, &sql).context("查询 sessions 列表失败")?;

    Ok(json!({
        "list": list,
        "page": page,
        "page_size": page_size,
        "total": total,
    }))
}

pub fn get_session(id: &str) -> Result<Option<Value>> {
    let id = id.trim();
    if id.is_empty() {
        anyhow::bail!("缺少 id");
    }

    let sql = format!(
        "SELECT {SELECT_COLS} FROM sessions WHERE id = '{}' LIMIT 1",
        escape_sql(id)
    );
    let rows = db_query(DB_NAME, &sql).context("查询 session 失败")?;
    Ok(rows.into_iter().next())
}

pub fn create_session(
    id: Option<&str>,
    title: Option<&str>,
    model: Option<&str>,
    system_prompt: Option<&str>,
) -> Result<Value> {
    let id = match id {
        Some(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => uuid::Uuid::new_v4().to_string(),
    };
    let title = title.unwrap_or("").trim();
    let model = model.unwrap_or("").trim();
    let system_prompt = system_prompt.unwrap_or("").trim();

    let sql = format!(
        "INSERT INTO sessions (id, title, model, system_prompt) VALUES ('{}', '{}', '{}', '{}')",
        escape_sql(&id),
        escape_sql(title),
        escape_sql(model),
        escape_sql(system_prompt)
    );
    db_query(DB_NAME, &sql).context("创建 session 失败")?;

    get_session(&id)
        .context("读取新建 session 失败")?
        .ok_or_else(|| anyhow::anyhow!("新建 session 后读取失败"))
}

pub fn update_session(
    id: &str,
    title: Option<&str>,
    model: Option<&str>,
    system_prompt: Option<&str>,
) -> Result<Option<Value>> {
    let id = id.trim();
    if id.is_empty() {
        anyhow::bail!("缺少 id");
    }

    let sets = vec![
        format!("title = '{}'", escape_sql(title.unwrap_or("").trim())),
        format!("model = '{}'", escape_sql(model.unwrap_or("").trim())),
        format!(
            "system_prompt = '{}'",
            escape_sql(system_prompt.unwrap_or("").trim())
        ),
        "updated_at = CURRENT_TIMESTAMP".to_string(),
    ];

    let sql = format!(
        "UPDATE sessions SET {} WHERE id = '{}'",
        sets.join(", "),
        escape_sql(id)
    );
    db_query(DB_NAME, &sql).context("更新 session 失败")?;

    get_session(id)
}

pub fn patch_session(
    id: &str,
    title: Option<&str>,
    model: Option<&str>,
    system_prompt: Option<&str>,
) -> Result<Option<Value>> {
    let id = id.trim();
    if id.is_empty() {
        anyhow::bail!("缺少 id");
    }

    let mut sets: Vec<String> = Vec::new();
    if let Some(v) = title {
        sets.push(format!("title = '{}'", escape_sql(v.trim())));
    }
    if let Some(v) = model {
        sets.push(format!("model = '{}'", escape_sql(v.trim())));
    }
    if let Some(v) = system_prompt {
        sets.push(format!("system_prompt = '{}'", escape_sql(v.trim())));
    }

    if sets.is_empty() {
        return get_session(id);
    }

    sets.push("updated_at = CURRENT_TIMESTAMP".to_string());
    let sql = format!(
        "UPDATE sessions SET {} WHERE id = '{}'",
        sets.join(", "),
        escape_sql(id)
    );
    db_query(DB_NAME, &sql).context("更新 session 失败")?;

    get_session(id)
}
