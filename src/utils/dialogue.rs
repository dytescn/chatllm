// src/utils/dialogue.rs
//! 对话索引（SQLite `dialogue.db` 的 `dialogue` 表）。
//!
//! 表结构以 `docs/db.md` 为准，由 `setup.json` 创建；本模块不再建表或补列。
//! 消息操作见 `utils::message`，业务错误码见 `utils::errcode`。

use crate::utils::db::{db_execute, db_query, db_update};
use crate::utils::designdb;
use crate::utils::errcode::ChatError;
use crate::utils::message;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

pub(crate) const DIALOGUE_DB: &str = "dialogue";

const DEFAULT_PAGE_SIZE: i64 = 20;
const MAX_PAGE_SIZE: i64 = 100;

fn valid_uuid(uuid: &str) -> bool {
    uuid.len() == 36 && uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// 归一化分页参数，返回 `(page, page_size, offset)`。
pub fn norm_page(page: Option<i64>, page_size: Option<i64>) -> (i64, i64, i64) {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let offset = (page - 1) * page_size;
    (page, page_size, offset)
}

/// 把字符串转成 SQL 字面量（单引号转义）。
pub fn esc(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn as_i64(value: Option<&Value>, default: i64) -> i64 {
    match value {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(default),
        Some(Value::String(s)) => s.parse::<i64>().unwrap_or(default),
        _ => default,
    }
}

/// 补齐对话行的数值字段默认值。
pub fn normalize_dialogue_row(mut row: Value) -> Value {
    if let Some(obj) = row.as_object_mut() {
        for (key, default) in [
            ("show_status", json!(1)),
            ("message_count", json!(0)),
            ("last_seq", json!(0)),
            ("is_pinned", json!(0)),
            ("is_archived", json!(0)),
        ] {
            if !obj.contains_key(key) {
                obj.insert(key.to_string(), default);
            }
        }
    }
    row
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

/// 新建对话，可指定 `dialogue.id`（如 `plugin_uuid`）、`show_status` 与 system。
///
/// `show_status = 2` 的对话不会出现在 `list_dialogue` 的普通列表中。
pub fn create_dialogue_with(
    id: Option<&str>,
    title: &str,
    show_status: i64,
    system_prompt: Option<&str>,
) -> Result<String> {
    println!(
        "[dialogue] create: id={:?}, title={:?}, show_status={}, system={}",
        id,
        title,
        show_status,
        system_prompt.is_some()
    );
    let uuid = match id {
        Some(id) => {
            if !valid_uuid(id) {
                bail!("非法对话 id: {}", id);
            }
            id.to_string()
        }
        None => uuid::Uuid::new_v4().to_string(),
    };
    let system = system_prompt
        .map(|text| esc(text))
        .unwrap_or_else(|| "NULL".to_string());
    let sql = format!(
        "INSERT INTO dialogue
         (id, title, is_pinned, is_archived, show_status, system_prompt,
          message_count, last_seq, created_at, updated_at)
         VALUES ({}, {}, 0, 0, {}, {}, 0, 0, datetime('now'), datetime('now'))",
        esc(&uuid),
        esc(title),
        show_status,
        system,
    );
    db_execute(DIALOGUE_DB, &sql).with_context(|| format!("插入对话 {} 失败", uuid))?;
    println!("[dialogue] create 完成: {}", uuid);
    Ok(uuid)
}

pub fn get_dialogue(id: &str) -> Result<Option<Value>> {
    println!("[dialogue] get: id={}", id);
    if !valid_uuid(id) {
        bail!("非法 uuid: {}", id);
    }
    let rows = db_query(
        DIALOGUE_DB,
        &format!("SELECT * FROM dialogue WHERE id = {} LIMIT 1", esc(id)),
    )?;
    let found = rows.into_iter().next().map(normalize_dialogue_row);
    println!("[dialogue] get 完成: id={}, found={}", id, found.is_some());
    Ok(found)
}

pub fn list_dialogue(page: Option<i64>, page_size: Option<i64>) -> Result<Value> {
    println!(
        "[dialogue] list: page={:?}, page_size={:?}",
        page, page_size
    );
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
    println!(
        "[dialogue] search: keyword={:?}, page={:?}, page_size={:?}",
        keyword, page, page_size
    );
    let page = dialogue_page(page, page_size);
    let needle = keyword.trim();
    if needle.is_empty() {
        return list_dialogue(Some(page.page), Some(page.page_size));
    }

    let needle = esc(needle);
    let where_clause = format!(
        "is_archived = 0 AND show_status != 2
         AND instr(lower(title), lower({})) > 0",
        needle
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

/// 对话更新项：`None` 表示不修改该字段。
pub struct DialoguePatch<'a> {
    pub title: Option<&'a str>,
    pub is_pinned: Option<i64>,
    pub is_archived: Option<i64>,
    pub show_status: Option<i64>,
}

pub fn update_dialogue(id: &str, p: &DialoguePatch<'_>) -> Result<()> {
    println!("[dialogue] update: id={}", id);
    if !valid_uuid(id) {
        bail!("非法 uuid: {}", id);
    }

    let mut sets: Vec<String> = Vec::new();
    if let Some(v) = p.title {
        sets.push(format!("title = {}", esc(v)));
    }
    if let Some(v) = p.is_pinned {
        sets.push(format!("is_pinned = {}", v));
    }
    if let Some(v) = p.is_archived {
        sets.push(format!("is_archived = {}", v));
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

/// 删除对话及其全部消息（先删消息库，再删索引）。
pub fn delete_dialogue(id: &str) -> Result<()> {
    println!("[dialogue] delete: id={}", id);
    if !valid_uuid(id) {
        bail!("非法 uuid: {}", id);
    }
    designdb::delete(id, message::MESSAGES, Some(json!({})))?;
    db_update(
        DIALOGUE_DB,
        &format!("DELETE FROM dialogue WHERE id = {}", esc(id)),
    )
    .with_context(|| format!("删除对话 {} 失败", id))?;
    Ok(())
}

/// 列表查询：`keyword` 非空时按标题搜索，否则返回普通列表。
pub fn query_dialogue(
    keyword: Option<&str>,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<Value> {
    println!(
        "[dialogue] query: keyword={:?}, page={:?}, page_size={:?}",
        keyword, page, page_size
    );
    match keyword.map(str::trim).filter(|s| !s.is_empty()) {
        Some(keyword) => search_dialogue(keyword, page, page_size),
        None => list_dialogue(page, page_size),
    }
}

/// 更新对话信息：`uuid` 必填，其余字段为 `None` 时不修改。
pub fn update_chat(
    uuid: Option<&str>,
    title: Option<&str>,
    show_status: Option<i64>,
    is_pinned: Option<i64>,
    is_archived: Option<i64>,
) -> Result<String, ChatError> {
    println!(
        "[dialogue] update_chat: uuid={:?}, title={:?}, show_status={:?}, pinned={:?}, archived={:?}",
        uuid, title, show_status, is_pinned, is_archived
    );
    let id = uuid
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChatError::InvalidUuid("uuid is required".to_string()))?;
    if !valid_uuid(id) {
        return Err(ChatError::InvalidUuid(id.to_string()));
    }
    update_dialogue(
        id,
        &DialoguePatch {
            title,
            is_pinned,
            is_archived,
            show_status,
        },
    )
    .map_err(|e| ChatError::Load(e.to_string()))?;
    Ok(id.to_string())
}

/// 读取一条对话及其消息分页：`{ dialogue, messages }`。
pub fn dialogue_detail(
    uuid: &str,
    before_id: Option<i64>,
    after_id: Option<i64>,
    limit: Option<i64>,
) -> Result<Value, ChatError> {
    println!(
        "[dialogue] detail: uuid={}, before_id={:?}, after_id={:?}, limit={:?}",
        uuid, before_id, after_id, limit
    );
    if !valid_uuid(uuid) {
        return Err(ChatError::InvalidUuid(uuid.to_string()));
    }
    let dialogue = get_dialogue(uuid)
        .map_err(|e| ChatError::Load(e.to_string()))?
        .ok_or_else(|| ChatError::Load(format!("对话不存在: {}", uuid)))?;
    let messages = message::list_messages(uuid, before_id, after_id, limit)
        .map_err(|e| ChatError::Load(e.to_string()))?;
    Ok(json!({ "dialogue": dialogue, "messages": messages }))
}

/// 标题：显式 `title` 优先，否则取 `prompt` 前 20 字符。
fn new_title(title: Option<&str>, prompt: &str) -> String {
    if let Some(title) = title.map(str::trim).filter(|s| !s.is_empty()) {
        return title.to_string();
    }
    let title: String = prompt.chars().take(20).collect();
    if title.is_empty() {
        "新对话".to_string()
    } else {
        title
    }
}

/// 截断长文本，避免日志刷屏。
fn preview(value: &str, max: usize) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

/// 已配置 AI 供应商后的对话流程：
///
/// 1. `think`：检索 `chat_think`；
/// 2. 按 `|` 拆分为数组；
/// 3. 数组中的每一段分别查询 `chat_plugin` 并执行插件；
/// 4. 合并插件结果，交给 `reply` 生成最终 assistant 内容。
///
/// think 未命中或拆分后没有有效段时，跳过插件，直接把用户原 prompt 交给 `reply`。
fn reply_with_pipeline(prompt: &str) -> String {
    let thought = match crate::utils::think::query(prompt, crate::utils::think::TOP_K) {
        Ok(value) => value,
        Err(error) => {
            println!("[chat] think 检索失败: {:#}", error);
            None
        }
    };

    let queries = thought
        .as_deref()
        .map(crate::utils::think::segments)
        .unwrap_or_default();
    let results = crate::utils::plugins::handle_many(&queries);
    let plugin_result = crate::utils::plugins::join_results(&results);
    let reply = crate::utils::reply::Reply::from_result(plugin_result.clone(), prompt);

    if !reply.used_reply_plugin {
        if let Some(input) = plugin_result.as_deref() {
            record_reply_exchange(input, &reply.content);
        }
    }

    reply.content
}

/// 把 plugins 结果与 reply_base 清洗结果写入 reply_base 系统对话。
///
/// 对话 id 使用 reply_base uuid；不存在时创建，并把 skills 写入 system_prompt。
fn record_reply_exchange(input: &str, output: &str) {
    let Some(uuid) = crate::utils::reply_base::uuid() else {
        println!("[reply] reply_base 未安装，跳过系统对话写入");
        return;
    };
    let skills = crate::utils::reply_base::skills();
    println!(
        "[reply] 写入 reply_base 系统对话: uuid={}, system={}, input={:?}, output={:?}",
        uuid,
        skills.is_some(),
        preview(input, 200),
        preview(output, 200)
    );

    match get_dialogue(&uuid) {
        Ok(Some(_)) => {}
        Ok(None) => {
            let title = "reply_base 系统对话";
            if let Err(error) = create_dialogue_with(Some(&uuid), title, 2, skills.as_deref()) {
                println!("[reply] 创建 reply_base 系统对话失败: {:#}", error);
                return;
            }
            println!("[reply] 已创建 reply_base 系统对话: {}", uuid);
        }
        Err(error) => {
            println!("[reply] 查询 reply_base 系统对话失败: {:#}", error);
            return;
        }
    }

    if !input.trim().is_empty() {
        if let Err(error) = message::append_message(&uuid, "user", input) {
            println!("[reply] 写入 reply_base user 消息失败: {:#}", error);
        }
    }
    if !output.trim().is_empty() {
        if let Err(error) = message::append_message(&uuid, "assistant", output) {
            println!("[reply] 写入 reply_base assistant 消息失败: {:#}", error);
        }
    }
}

/// 新建或续聊对话，并把 `prompt` 写成一条 user 消息，返回 `(uuid, created)`。
///
/// - `uuid` 非空：续聊已有对话，不存在则报错；
/// - `plugin_uuid` 非空：以它作为对话 id，已存在则复用，新建时 `show_status` 默认 2；
/// - `system_prompt`：仅新建对话时写入 `dialogue.system_prompt`；
/// - 其余情况：自动生成 id，`show_status` 默认 1。
pub fn open_chat(
    prompt: &str,
    uuid: Option<&str>,
    plugin_uuid: Option<&str>,
    title: Option<&str>,
    show_status: Option<i64>,
    system_prompt: Option<&str>,
) -> Result<(String, bool), ChatError> {
    println!(
        "[dialogue] open_chat: prompt={:?}, uuid={:?}, plugin_uuid={:?}, title={:?}, show_status={:?}, system={}",
        preview(prompt, 200),
        uuid,
        plugin_uuid,
        title,
        show_status,
        system_prompt.is_some()
    );
    let (uuid, created) = match uuid.map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => {
            if !valid_uuid(id) {
                return Err(ChatError::InvalidUuid(id.to_string()));
            }
            match get_dialogue(id) {
                Ok(Some(_)) => (id.to_string(), false),
                Ok(None) => {
                    return Err(ChatError::Load(format!("对话不存在: {}", id)));
                }
                Err(e) => return Err(ChatError::Load(e.to_string())),
            }
        }
        None => match plugin_uuid.map(str::trim).filter(|s| !s.is_empty()) {
            Some(id) => {
                if !valid_uuid(id) {
                    return Err(ChatError::InvalidUuid(id.to_string()));
                }
                match get_dialogue(id) {
                    Ok(Some(_)) => (id.to_string(), false),
                    Ok(None) => {
                        let title = new_title(title, prompt);
                        let created = create_dialogue_with(
                            Some(id),
                            &title,
                            show_status.unwrap_or(2),
                            system_prompt,
                        )
                        .map_err(|e| ChatError::Create(e.to_string()))?;
                        (created, true)
                    }
                    Err(e) => return Err(ChatError::Load(e.to_string())),
                }
            }
            None => {
                let title = new_title(title, prompt);
                let created =
                    create_dialogue_with(None, &title, show_status.unwrap_or(1), system_prompt)
                        .map_err(|e| ChatError::Create(e.to_string()))?;
                (created, true)
            }
        },
    };

    message::append_message(&uuid, "user", prompt).map_err(|e| ChatError::Append(e.to_string()))?;

    // check 已在 FFI 入口完成；这里直接进入 think → plugins → reply。
    let content = reply_with_pipeline(prompt);
    message::append_message(&uuid, "assistant", &content)
        .map_err(|e| ChatError::Append(e.to_string()))?;

    println!(
        "[dialogue] open_chat 完成: uuid={}, created={}, assistant={:?}",
        uuid,
        created,
        preview(&content, 300)
    );
    Ok((uuid, created))
}

pub fn is_valid_uuid(uuid: &str) -> bool {
    valid_uuid(uuid)
}
