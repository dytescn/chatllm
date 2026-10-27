// src/ffi/run.rs
use serde_json::Value;

use crate::utils::dialogue::{
    append_message, create_dialogue_with, get_dialogue, is_valid_uuid, message_history,
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
    /// 插件 UUID：新建时作为 `dialogue.id`，并默认注入该插件 skills
    pub plugin_uuid: Option<String>,
    /// 新建对话时指定 `dialogue.id`（普通指定 id 场景）
    pub dialogue_id: Option<String>,
    /// 新建对话的显示状态；插件对话默认 2（列表不显示）
    pub show_status: Option<i64>,
}

/// 对话目标：续聊已有对话，或新建指定 / 自动 id 的对话。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChatTarget {
    Existing(String),
    New {
        id: Option<String>,
        show_status: i64,
    },
}

/// 解析本次请求应使用/创建的对话（纯函数，便于测试）。
///
/// 优先级：`uuid`（续聊） > `plugin_uuid`（固定 id，默认隐藏） >
/// `dialogue_id`（指定 id） > 自动生成。
pub(crate) fn resolve_chat_target(
    uuid: Option<&str>,
    plugin_uuid: Option<&str>,
    dialogue_id: Option<&str>,
    show_status: Option<i64>,
) -> ChatTarget {
    if let Some(uuid) = uuid.map(str::trim).filter(|s| !s.is_empty()) {
        return ChatTarget::Existing(uuid.to_string());
    }
    if let Some(plugin) = plugin_uuid.map(str::trim).filter(|s| !s.is_empty()) {
        return ChatTarget::New {
            id: Some(plugin.to_string()),
            show_status: show_status.unwrap_or(2),
        };
    }
    if let Some(id) = dialogue_id.map(str::trim).filter(|s| !s.is_empty()) {
        return ChatTarget::New {
            id: Some(id.to_string()),
            show_status: show_status.unwrap_or(1),
        };
    }
    ChatTarget::New {
        id: None,
        show_status: show_status.unwrap_or(1),
    }
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

fn should_use_skills(options: &ChatOptions, plugin_uuid: Option<&str>) -> bool {
    options.use_skills || (plugin_uuid.is_some() && options.system_prompt.is_none())
}

fn build_prompt(
    options: &ChatOptions,
    plugin_uuid: Option<&str>,
) -> Result<skills::SkillBuild, ChatFlowError> {
    skills::build_system_prompt(
        should_use_skills(options, plugin_uuid),
        plugin_uuid,
        &options.skill_categories,
        &options.skill_ids,
        options.system_prompt.as_deref(),
    )
    .map_err(|e| ChatFlowError::Skills(e.to_string()))
}

fn new_title(name: Option<&str>, prompt: &str, plugin_uuid: Option<&str>) -> String {
    if let Some(name) = name.map(str::trim).filter(|s| !s.is_empty()) {
        return name.to_string();
    }
    if let Some(plugin) = plugin_uuid {
        return format!("插件对话 {}", plugin);
    }
    let title: String = prompt.chars().take(20).collect();
    if title.is_empty() {
        "新对话".to_string()
    } else {
        title
    }
}

fn create_new_chat(
    id: Option<&str>,
    show_status: i64,
    name: Option<&str>,
    prompt: &str,
    keyword: Option<&str>,
    options: &ChatOptions,
    plugin_uuid: Option<&str>,
) -> Result<(String, bool, Option<Value>, Option<skills::SkillBuild>), ChatFlowError> {
    let build = build_prompt(options, plugin_uuid)?;
    let title = new_title(name, prompt, plugin_uuid);
    let kw = keyword.unwrap_or("");
    let new_id = create_dialogue_with(
        id,
        &title,
        "",
        kw,
        Some(build.system_prompt.as_str()),
        show_status,
    )
    .map_err(|e| ChatFlowError::CreateDialogue(e.to_string()))?;
    Ok((new_id, true, None, Some(build)))
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

    let plugin_uuid = options
        .plugin_uuid
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let use_skills = should_use_skills(options, plugin_uuid);
    let target = resolve_chat_target(
        uuid,
        plugin_uuid,
        options.dialogue_id.as_deref(),
        options.show_status,
    );

    let mut used_skills: Vec<String> = Vec::new();
    let mut truncated = false;

    let (uuid, created, existing_row, created_build) = match target {
        ChatTarget::Existing(id) => {
            if !is_valid_uuid(&id) {
                return Err(ChatFlowError::InvalidUuid(id));
            }
            match get_dialogue(&id) {
                Ok(Some(row)) => (id, false, Some(row), None),
                Ok(None) => {
                    return Err(ChatFlowError::LoadDialogue(format!("对话不存在: {}", id)));
                }
                Err(e) => return Err(ChatFlowError::LoadDialogue(e.to_string())),
            }
        }
        ChatTarget::New { id, show_status } => {
            // plugin_uuid 场景：以 plugin_uuid 作为 dialogue.id；已存在则直接复用。
            if let (Some(_), Some(existing_id)) = (plugin_uuid, id.as_deref()) {
                if !is_valid_uuid(existing_id) {
                    return Err(ChatFlowError::InvalidUuid(existing_id.to_string()));
                }
                match get_dialogue(existing_id) {
                    Ok(Some(row)) => (existing_id.to_string(), false, Some(row), None),
                    Ok(None) => create_new_chat(
                        id.as_deref(),
                        show_status,
                        name,
                        prompt,
                        keyword,
                        options,
                        plugin_uuid,
                    )?,
                    Err(e) => return Err(ChatFlowError::LoadDialogue(e.to_string())),
                }
            } else {
                create_new_chat(
                    id.as_deref(),
                    show_status,
                    name,
                    prompt,
                    keyword,
                    options,
                    plugin_uuid,
                )?
            }
        }
    };

    let final_system_prompt = if let Some(build) = created_build {
        used_skills = build.skill_ids;
        truncated = build.truncated;
        Some(build.system_prompt)
    } else if let Some(existing) = existing_row {
        let current_system = non_empty(existing.get("system_prompt"));
        let should_build = options.refresh_system_prompt
            || options.system_prompt.is_some()
            || (current_system.is_none() && use_skills);

        if should_build {
            let build = build_prompt(options, plugin_uuid)?;
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
            update_dialogue(&uuid, &patch)
                .map_err(|e| ChatFlowError::LoadDialogue(e.to_string()))?;
            used_skills = build.skill_ids;
            truncated = build.truncated;
            Some(build.system_prompt)
        } else {
            current_system
        }
    } else {
        None
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
    let system_prompt = final_system_prompt.or_else(|| non_empty(dialogue.get("system_prompt")));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_wins_over_plugin_and_dialogue_id() {
        let target = resolve_chat_target(
            Some("11111111-1111-4111-8111-111111111111"),
            Some("plugin-1"),
            Some("dialogue-1"),
            Some(2),
        );
        assert_eq!(
            target,
            ChatTarget::Existing("11111111-1111-4111-8111-111111111111".to_string())
        );
    }

    #[test]
    fn plugin_uuid_creates_hidden_dialogue_with_its_id() {
        let target = resolve_chat_target(None, Some("plugin-1"), None, None);
        assert_eq!(
            target,
            ChatTarget::New {
                id: Some("plugin-1".to_string()),
                show_status: 2,
            }
        );
    }

    #[test]
    fn plugin_uuid_beats_dialogue_id() {
        let target = resolve_chat_target(None, Some("plugin-1"), Some("dialogue-1"), None);
        assert_eq!(
            target,
            ChatTarget::New {
                id: Some("plugin-1".to_string()),
                show_status: 2,
            }
        );
    }

    #[test]
    fn dialogue_id_creates_visible_dialogue_by_default() {
        let target = resolve_chat_target(None, None, Some("dialogue-1"), None);
        assert_eq!(
            target,
            ChatTarget::New {
                id: Some("dialogue-1".to_string()),
                show_status: 1,
            }
        );
    }

    #[test]
    fn no_id_creates_visible_dialogue() {
        let target = resolve_chat_target(None, None, None, None);
        assert_eq!(
            target,
            ChatTarget::New {
                id: None,
                show_status: 1,
            }
        );
    }

    #[test]
    fn explicit_show_status_overrides_defaults() {
        let target = resolve_chat_target(None, Some("plugin-1"), None, Some(1));
        assert_eq!(
            target,
            ChatTarget::New {
                id: Some("plugin-1".to_string()),
                show_status: 1,
            }
        );
    }

    #[test]
    fn title_prefers_name_then_plugin_then_prompt() {
        assert_eq!(
            new_title(Some("会话标题"), "prompt", Some("plugin-1")),
            "会话标题"
        );
        assert_eq!(
            new_title(None, "prompt", Some("plugin-1")),
            "插件对话 plugin-1"
        );
        assert_eq!(
            new_title(None, "这是一段很长的提示词内容超过二十个字符", None)
                .chars()
                .count(),
            19
        );
    }
}
