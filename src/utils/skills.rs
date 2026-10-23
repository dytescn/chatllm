use crate::utils::db::db_query;
use anyhow::{Context, Result};
use serde_json::{json, Value};

const DB_NAME: &str = "skills";
const DEFAULT_PAGE_SIZE: i64 = 20;
const MAX_PAGE_SIZE: i64 = 100;
const SELECT_COLS: &str = "id, uid, content, version, created_at, updated_at";

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

fn where_clause(id: Option<i64>, uid: Option<&str>) -> Result<String> {
    match (id, uid) {
        (Some(id), _) => Ok(format!("id = {}", id)),
        (None, Some(uid)) if !uid.trim().is_empty() => {
            Ok(format!("uid = '{}'", escape_sql(uid.trim())))
        }
        _ => anyhow::bail!("缺少 id 或 uid"),
    }
}

pub fn list_skills(page: Option<i64>, page_size: Option<i64>) -> Result<Value> {
    let (page, page_size, offset) = norm_page(page, page_size);

    let count_rows =
        db_query(DB_NAME, "SELECT count(*) AS n FROM skills").context("统计 skills 失败")?;
    let total = count_rows
        .first()
        .and_then(|r| r.get("n"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let sql = format!(
        "SELECT {SELECT_COLS} FROM skills ORDER BY id DESC LIMIT {page_size} OFFSET {offset}"
    );
    let list = db_query(DB_NAME, &sql).context("查询 skills 列表失败")?;

    Ok(json!({
        "list": list,
        "page": page,
        "page_size": page_size,
        "total": total,
    }))
}

pub fn get_skill(id: Option<i64>, uid: Option<&str>) -> Result<Option<Value>> {
    let where_sql = where_clause(id, uid)?;
    let sql = format!("SELECT {SELECT_COLS} FROM skills WHERE {where_sql} LIMIT 1");
    let rows = db_query(DB_NAME, &sql).context("查询 skill 失败")?;
    Ok(rows.into_iter().next())
}

pub fn create_skill(uid: Option<&str>, content: &str, version: Option<i64>) -> Result<Value> {
    let content = content.trim();
    if content.is_empty() {
        anyhow::bail!("content 不能为空");
    }

    let uid = match uid {
        Some(u) if !u.trim().is_empty() => u.trim().to_string(),
        _ => uuid::Uuid::new_v4().to_string(),
    };
    let version = version.unwrap_or(1).max(1);

    let sql = format!(
        "INSERT INTO skills (uid, content, version) VALUES ('{}', '{}', {})",
        escape_sql(&uid),
        escape_sql(content),
        version
    );
    db_query(DB_NAME, &sql).context("创建 skill 失败")?;

    get_skill(None, Some(&uid))
        .context("读取新建 skill 失败")?
        .ok_or_else(|| anyhow::anyhow!("新建 skill 后读取失败"))
}

pub fn update_skill(
    id: Option<i64>,
    uid: Option<&str>,
    content: &str,
    version: Option<i64>,
) -> Result<Option<Value>> {
    let content = content.trim();
    if content.is_empty() {
        anyhow::bail!("content 不能为空");
    }
    let where_sql = where_clause(id, uid)?;

    let mut sets = vec![format!("content = '{}'", escape_sql(content))];
    if let Some(v) = version {
        sets.push(format!("version = {}", v.max(1)));
    }
    sets.push("updated_at = CURRENT_TIMESTAMP".to_string());

    let sql = format!("UPDATE skills SET {} WHERE {where_sql}", sets.join(", "));
    db_query(DB_NAME, &sql).context("更新 skill 失败")?;

    get_skill(id, uid)
}

pub fn patch_skill(
    id: Option<i64>,
    uid: Option<&str>,
    content: Option<&str>,
    version: Option<i64>,
) -> Result<Option<Value>> {
    let where_sql = where_clause(id, uid)?;

    let mut sets: Vec<String> = Vec::new();
    if let Some(content) = content {
        let content = content.trim();
        if content.is_empty() {
            anyhow::bail!("content 不能为空");
        }
        sets.push(format!("content = '{}'", escape_sql(content)));
    }
    if let Some(v) = version {
        sets.push(format!("version = {}", v.max(1)));
    }

    if sets.is_empty() {
        return get_skill(id, uid);
    }

    sets.push("updated_at = CURRENT_TIMESTAMP".to_string());
    let sql = format!("UPDATE skills SET {} WHERE {where_sql}", sets.join(", "));
    db_query(DB_NAME, &sql).context("更新 skill 失败")?;

    get_skill(id, uid)
}
