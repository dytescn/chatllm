// src/utils/message.rs
//! 对话消息：存于 `./designdb/<dialogue_id>` 的 `messages` 集合。
//!
//! 写入消息时同步维护 `dialogue.message_count` 与 `dialogue.last_seq`。

use crate::utils::db::{db_query, db_update};
use crate::utils::designdb;
use crate::utils::dialogue::{is_valid_uuid, DIALOGUE_DB};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

/// designdb 中存放消息的集合名。
pub(crate) const MESSAGES: &str = "messages";

const DEFAULT_MSG_LIMIT: i64 = 20;
const MAX_MSG_LIMIT: i64 = 100;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn as_i64(value: Option<&Value>, default: i64) -> i64 {
    match value {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(default),
        Some(Value::String(s)) => s.parse::<i64>().unwrap_or(default),
        _ => default,
    }
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

/// 追加一条消息，并把 `dialogue.last_seq` / `message_count` 加一。
pub fn append_message(uuid: &str, role: &str, content: &str) -> Result<()> {
    println!(
        "[message] append: uuid={}, role={}, content={:?}",
        uuid,
        role,
        preview(content, 200)
    );
    if !is_valid_uuid(uuid) {
        bail!("非法 uuid: {}", uuid);
    }

    let rows = db_query(
        DIALOGUE_DB,
        &format!(
            "SELECT last_seq, message_count FROM dialogue WHERE id = '{}' LIMIT 1",
            uuid
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
             WHERE id = '{}'",
            seq, uuid
        ),
    )
    .with_context(|| format!("刷新 {} 计数失败", uuid))?;

    println!("[message] append 完成: uuid={}, seq={}", uuid, seq);
    Ok(())
}

/// 读取某对话的全部消息（按 seq 升序），供续聊组装上下文。
pub fn message_history(uuid: &str) -> Result<Vec<Value>> {
    if !is_valid_uuid(uuid) {
        bail!("非法 uuid: {}", uuid);
    }
    let mut all = designdb::query_all(uuid, MESSAGES, Some(json!({})), Some(json!({ "seq": 1 })))?;
    all.sort_by_key(|m| as_i64(m.get("seq"), 0));
    println!("[message] history: uuid={}, count={}", uuid, all.len());
    Ok(all)
}

/// 按 `before_id` / `after_id` 游标分页读取消息，并附带 `content_html`。
pub fn list_messages(
    uuid: &str,
    before_id: Option<i64>,
    after_id: Option<i64>,
    limit: Option<i64>,
) -> Result<Value> {
    println!(
        "[message] list: uuid={}, before_id={:?}, after_id={:?}, limit={:?}",
        uuid, before_id, after_id, limit
    );
    if !is_valid_uuid(uuid) {
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

    println!(
        "[message] list 完成: uuid={}, count={}, first_id={:?}, last_id={:?}",
        uuid,
        list.len(),
        first_id,
        last_id
    );
    Ok(json!({
        "list": list,
        "limit": limit,
        "first_id": first_id,
        "last_id": last_id,
        "has_more_before": has_more_before,
        "has_more_after": has_more_after,
    }))
}

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
