// src/utils/chat.rs
//! 对话入口：只暴露 [`chat_message`] 一个方法。
//!
//! 负责对话生命周期（新建 / 续聊、写 user 消息），具体流程由
//! [`crate::utils::run_flow`] 执行。

use crate::ffi::types::ChatDialogue;
use crate::utils::dialogue;
use crate::utils::errcode::ChatError;
use crate::utils::run_flow;
use serde_json::{json, Value};

/// 执行一次对话，返回 `{ uuid, created, content }`。
///
/// 对话生命周期：
/// - `uuid` 为空：新建一条需要显示的对话；
/// - `uuid` 非空且对话存在：直接续聊；
/// - `uuid` 非空但对话不存在：以该 uuid 新建一条不显示的对话
///   （用于 plugins_uid 的回调内容）。
pub fn chat_message(req: &ChatDialogue) -> Result<Value, ChatError> {
    let title = dialogue::new_title(req.title.as_deref(), &req.content);

    // 判断是否需要创建对话。
    let (uuid, created) = match req.uuid.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        // 没有 uuid：创建一条显示的对话。
        None => {
            let uuid =
                dialogue::create_dialogue_with(None, &title, req.show_status.unwrap_or(1), None)
                    .map_err(|e| ChatError::Create(e.to_string()))?;
            (uuid, true)
        }
        Some(id) => {
            if !dialogue::is_valid_uuid(id) {
                return Err(ChatError::InvalidUuid(id.to_string()));
            }
            match dialogue::get_dialogue(id) {
                // 已有对话：直接续聊。
                Ok(Some(_)) => (id.to_string(), false),
                // 对话不存在：以该 uuid 创建一条不显示的对话。
                Ok(None) => {
                    let uuid = dialogue::create_dialogue_with(Some(id), &title, 2, None)
                        .map_err(|e| ChatError::Create(e.to_string()))?;
                    (uuid, true)
                }
                Err(e) => return Err(ChatError::Load(e.to_string())),
            }
        }
    };

    // 流程只处理已有对话。
    let flow = ChatDialogue {
        uuid: Some(uuid.clone()),
        ..req.clone()
    };
    let res = run_flow::run(&flow)?;

    Ok(json!({
        "uuid": uuid,
        "created": created,
        "content": res["content"].clone(),
    }))
}
