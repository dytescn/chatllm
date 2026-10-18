// src/ffi/run.rs
use crate::utils::dialogue::{
    append_message, create_dialogue, is_valid_uuid, update_keyword,
};
use crate::utils::siliconflow::chat_single;

pub struct ChatOutcome {
    pub uuid: String,
    pub created: bool,
    pub content: String,
}

pub enum ChatFlowError {
    InvalidUuid(String),
    CreateDialogue(String),
    AppendUser(String),
    UpdateKeyword(String),
    SiliconFlow(String),
    AppendAssistant(String),
}

impl ChatFlowError {
    pub fn code(&self) -> i32 {
        match self {
            ChatFlowError::InvalidUuid(_) => 400103,
            ChatFlowError::CreateDialogue(_) => 400300,
            ChatFlowError::AppendUser(_) => 400301,
            ChatFlowError::UpdateKeyword(_) => 400305,
            ChatFlowError::SiliconFlow(_) => 400400,
            ChatFlowError::AppendAssistant(_) => 400401,
        }
    }

    pub fn message(&self) -> String {
        match self {
            ChatFlowError::InvalidUuid(e) => format!("invalid uuid: {}", e),
            ChatFlowError::CreateDialogue(e) => format!("create dialogue failed: {}", e),
            ChatFlowError::AppendUser(e) => format!("append user message failed: {}", e),
            ChatFlowError::UpdateKeyword(e) => format!("update keyword failed: {}", e),
            ChatFlowError::SiliconFlow(e) => format!("siliconflow chat failed: {}", e),
            ChatFlowError::AppendAssistant(e) => format!("append assistant message failed: {}", e),
        }
    }
}

/// 一次完整 chat 流程：
/// 1) 校验 uuid
/// 2) 有 uuid 续聊 / 无 uuid 新建
/// 3) 续聊时更新 keyword
/// 4) 写 user 消息
/// 5) 调 siliconflow 拿 content
/// 6) 写 assistant 消息
pub fn run_chat(
    prompt: &str,
    uuid: Option<&str>,
    name: Option<&str>,
    keyword: Option<&str>,
) -> Result<ChatOutcome, ChatFlowError> {
    if let Some(u) = uuid {
        if !is_valid_uuid(u) {
            return Err(ChatFlowError::InvalidUuid(u.to_string()));
        }
    }

    let (uuid, created) = match uuid {
        Some(u) => (u.to_string(), false),
        None => {
            let title: String = match name {
                Some(n) if !n.is_empty() => n.to_string(),
                _ => {
                    let s: String = prompt.chars().take(20).collect();
                    if s.is_empty() { "新对话".to_string() } else { s }
                }
            };
            let kw = keyword.unwrap_or("");
            match create_dialogue(&title, "", kw) {
                Ok(u) => (u, true),
                Err(e) => return Err(ChatFlowError::CreateDialogue(e.to_string())),
            }
        }
    };

    if !created {
        if let Some(kw) = keyword {
            if let Err(e) = update_keyword(&uuid, kw) {
                return Err(ChatFlowError::UpdateKeyword(e.to_string()));
            }
        }
    }

    if let Err(e) = append_message(&uuid, "user", prompt) {
        return Err(ChatFlowError::AppendUser(e.to_string()));
    }

    let content = match chat_single(prompt) {
        Ok(c) => c,
        Err(e) => return Err(ChatFlowError::SiliconFlow(e.to_string())),
    };

    if let Err(e) = append_message(&uuid, "assistant", &content) {
        return Err(ChatFlowError::AppendAssistant(e.to_string()));
    }

    Ok(ChatOutcome { uuid, created, content })
}
