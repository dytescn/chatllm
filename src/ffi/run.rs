// src/ffi/run.rs
use serde_json::Value;

use crate::utils::dialogue::{
    append_message, create_dialogue, get_dialogue, is_valid_uuid, message_history,
    update_dialogue, update_keyword, update_session_id, DialoguePatch,
};
use crate::utils::markparse::to_html;
use crate::utils::siliconflow::{chat_messages, ChatMessage};
use crate::utils::skills;

#[derive(Debug, Clone, Default)]
pub struct ChatOptions {
    pub use_skills: bool,
    pub skill_categories: Vec<String>,
    pub skill_ids: Vec<String>,
    pub system_prompt: Option<String>,
    pub refresh_system_prompt: bool,
}

pub struct ChatOutcome {
    pub uuid: String,
    pub created: bool,
    pub content: String,
    /// assistant 回复的 Markdown → HTML（转译失败时为空字符串，不阻断对话）
    pub content_html: String,
    /// 模型响应 id（写入 dialogue.session_id）
    pub session_id: Option<String>,
    pub system_prompt: Option<String>,
    pub skills: Vec<String>,
    pub truncated: bool,
}

pub enum ChatFlowError {
    InvalidUuid(String),
    CreateDialogue(String),
    LoadDialogue(String),
    LoadHistory(String),
    Skills(String),
    AppendUser(String),
    UpdateKeyword(String),
    SiliconFlow(String),
    UpdateSession(String),
    AppendAssistant(String),
}

impl ChatFlowError {
    pub fn code(&self) -> i32 {
        match self {
            ChatFlowError::InvalidUuid(_) => 400103,
            ChatFlowError::CreateDialogue(_) => 400300,
            ChatFlowError::LoadDialogue(_) => 400303,
            ChatFlowError::LoadHistory(_) => 400301,
            ChatFlowError::Skills(_) => 400109,
            ChatFlowError::AppendUser(_) => 400301,
            ChatFlowError::UpdateKeyword(_) => 400305,
            ChatFlowError::SiliconFlow(_) => 400400,
            ChatFlowError::UpdateSession(_) => 400303,
            ChatFlowError::AppendAssistant(_) => 400401,
        }
    }

    pub fn message(&self) -> String {
        match self {
            ChatFlowError::InvalidUuid(e) => format!("invalid uuid: {}", e),
            ChatFlowError::CreateDialogue(e) => format!("create dialogue failed: {}", e),
            ChatFlowError::LoadDialogue(e) => format!("load dialogue failed: {}", e),
            ChatFlowError::LoadHistory(e) => format!("load messages failed: {}", e),
            ChatFlowError::Skills(e) => format!("build system prompt failed: {}", e),
            ChatFlowError::AppendUser(e) => format!("append user message failed: {}", e),
            ChatFlowError::UpdateKeyword(e) => format!("update keyword failed: {}", e),
            ChatFlowError::SiliconFlow(e) => format!("siliconflow chat failed: {}", e),
            ChatFlowError::UpdateSession(e) => format!("update session_id failed: {}", e),
            ChatFlowError::AppendAssistant(e) => format!("append assistant message failed: {}", e),
        }
    }
}

fn non_empty(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn build_prompt(options: &ChatOptions) -> Result<skills::SkillBuild, ChatFlowError> {
    skills::build_system_prompt(
        options.use_skills,
        &options.skill_categories,
        &options.skill_ids,
        options.system_prompt.as_deref(),
    )
    .map_err(|e| ChatFlowError::Skills(e.to_string()))
}

/// 一次完整 chat 流程：
/// 1) 校验 uuid
/// 2) 新建 / 续聊
/// 3) 组装 system prompt（skills）
/// 4) 写 user 消息
/// 5) 读历史 messages + session_id
/// 6) 调模型，取 content 与响应 id
/// 7) 响应 id 写入 dialogue.session_id
/// 8) 写 assistant 消息
pub fn run_chat(
    prompt: &str,
    uuid: Option<&str>,
    name: Option<&str>,
    keyword: Option<&str>,
    options: &ChatOptions,
) -> Result<ChatOutcome, ChatFlowError> {
    if let Some(u) = uuid {
        if !is_valid_uuid(u) {
            return Err(ChatFlowError::InvalidUuid(u.to_string()));
        }
    }

    let final_system_prompt: Option<String>;
    let mut used_skills: Vec<String> = Vec::new();
    let mut truncated = false;

    let (uuid, created) = match uuid {
        Some(u) => {
            let existing = match get_dialogue(u) {
                Ok(Some(row)) => row,
                Ok(None) => {
                    return Err(ChatFlowError::LoadDialogue(format!("对话不存在: {}", u)));
                }
                Err(e) => return Err(ChatFlowError::LoadDialogue(e.to_string())),
            };
            let current_system = non_empty(existing.get("system_prompt"));
            let should_build = options.refresh_system_prompt
                || options.system_prompt.is_some()
                || (current_system.is_none() && options.use_skills);

            if should_build {
                let build = build_prompt(options)?;
                let patch = DialoguePatch {
                    title: None,
                    keyword: None,
                    model_name: None,
                    system_prompt: Some(build.system_prompt.as_str()),
                    model_config: None,
                    is_pinned: None,
                    is_archived: None,
                    session_id: None,
                    show_status: None,
                };
                update_dialogue(u, &patch)
                    .map_err(|e| ChatFlowError::LoadDialogue(e.to_string()))?;
                final_system_prompt = Some(build.system_prompt);
                used_skills = build.skill_ids;
                truncated = build.truncated;
            } else {
                final_system_prompt = current_system;
            }
            (u.to_string(), false)
        }
        None => {
            let title: String = match name {
                Some(n) if !n.is_empty() => n.to_string(),
                _ => {
                    let s: String = prompt.chars().take(20).collect();
                    if s.is_empty() { "新对话".to_string() } else { s }
                }
            };
            let build = build_prompt(options)?;
            let kw = keyword.unwrap_or("");
            match create_dialogue(&title, "", kw, Some(build.system_prompt.as_str())) {
                Ok(u) => {
                    final_system_prompt = Some(build.system_prompt);
                    used_skills = build.skill_ids;
                    truncated = build.truncated;
                    (u, true)
                }
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

    let dialogue = match get_dialogue(&uuid) {
        Ok(Some(row)) => row,
        Ok(None) => {
            return Err(ChatFlowError::LoadDialogue(format!("对话不存在: {}", uuid)));
        }
        Err(e) => return Err(ChatFlowError::LoadDialogue(e.to_string())),
    };
    let system_prompt = final_system_prompt
        .or_else(|| non_empty(dialogue.get("system_prompt")));
    let session_id = non_empty(dialogue.get("session_id"));

    let history = match message_history(&uuid) {
        Ok(rows) => rows,
        Err(e) => return Err(ChatFlowError::LoadHistory(e.to_string())),
    };

    let mut messages: Vec<ChatMessage> = Vec::with_capacity(history.len() + 1);
    if let Some(system_prompt) = &system_prompt {
        messages.push(ChatMessage::system(system_prompt));
    }
    for row in &history {
        let role = row.get("role").and_then(Value::as_str).unwrap_or("user");
        let content = row.get("content").and_then(Value::as_str).unwrap_or("");
        if content.is_empty() {
            continue;
        }
        match role {
            "assistant" => messages.push(ChatMessage::assistant(content)),
            "system" => messages.push(ChatMessage::system(content)),
            _ => messages.push(ChatMessage::user(content)),
        }
    }

    let result = match chat_messages(messages, session_id.as_deref()) {
        Ok(result) => result,
        Err(e) => return Err(ChatFlowError::SiliconFlow(e.to_string())),
    };

    if let Some(id) = result.id.as_deref() {
        if let Err(e) = update_session_id(&uuid, id) {
            return Err(ChatFlowError::UpdateSession(e.to_string()));
        }
    }

    // Markdown 转译只影响返回给前端的 HTML，原文仍写入消息表；失败不阻断对话。
    let content_html = match to_html(&result.content) {
        Ok(html) => html,
        Err(e) => {
            eprintln!("[chatllm] markdown 转译失败，降级为原文: {:#}", e);
            String::new()
        }
    };

    if let Err(e) = append_message(&uuid, "assistant", &result.content) {
        return Err(ChatFlowError::AppendAssistant(e.to_string()));
    }

    Ok(ChatOutcome {
        uuid,
        created,
        content: result.content,
        content_html,
        session_id: result.id,
        system_prompt,
        skills: used_skills,
        truncated,
    })
}
