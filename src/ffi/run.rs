use serde_json::Value;

use crate::utils::dialogue::{
    append_message, create_dialogue_with, get_dialogue, is_valid_uuid, message_history,
    update_dialogue, update_keyword, DialoguePatch,
};

#[derive(Debug, Clone, Default)]
pub struct ChatOptions {
    pub system_prompt: Option<String>,
    /// 插件 UUID：新建时作为 `dialogue.id`
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
    /// 模型生成已移除；保留字段以兼容旧响应，固定为空字符串。
    pub content: String,
    /// 模型生成已移除；保留字段以兼容旧响应，固定为空字符串。
    pub content_html: String,
    pub session_id: Option<String>,
    pub system_prompt: Option<String>,
}

pub enum ChatFlowError {
    InvalidUuid(String),
    CreateDialogue(String),
    LoadDialogue(String),
    AppendUser(String),
    UpdateKeyword(String),
}

impl ChatFlowError {
    pub fn code(&self) -> i32 {
        match self {
            ChatFlowError::InvalidUuid(_) => 400103,
            ChatFlowError::CreateDialogue(_) => 400300,
            ChatFlowError::LoadDialogue(_) => 400303,
            ChatFlowError::AppendUser(_) => 400301,
            ChatFlowError::UpdateKeyword(_) => 400305,
        }
    }

    pub fn message(&self) -> String {
        match self {
            ChatFlowError::InvalidUuid(e) => format!("invalid uuid: {}", e),
            ChatFlowError::CreateDialogue(e) => format!("create dialogue failed: {}", e),
            ChatFlowError::LoadDialogue(e) => format!("load dialogue failed: {}", e),
            ChatFlowError::AppendUser(e) => format!("append user message failed: {}", e),
            ChatFlowError::UpdateKeyword(e) => format!("update keyword failed: {}", e),
        }
    }
}

fn non_empty(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
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
) -> Result<String, ChatFlowError> {
    let title = new_title(name, prompt, plugin_uuid);
    let kw = keyword.unwrap_or("");
    create_dialogue_with(
        id,
        &title,
        "",
        kw,
        options.system_prompt.as_deref(),
        show_status,
    )
    .map_err(|e| ChatFlowError::CreateDialogue(e.to_string()))
}

/// 一次 chat 写入的聊天插件候选数量上限。
const CHAT_PLUGIN_TOP_K: i64 = 5;

/// 把 `chat_plugin` 检索结果格式化为对话中的 assistant 文本。
///
/// 纯函数，便于单测；`value` 预期是插件元数据 JSON 字符串。
fn format_plugin_reply(hits: &[Value]) -> String {
    if hits.is_empty() {
        return install_plugin_prompt();
    }

    let mut lines = Vec::with_capacity(hits.len() * 2 + 1);
    lines.push(format!("找到 {} 个匹配的 chat 插件：", hits.len()));
    for (index, hit) in hits.iter().enumerate() {
        let key = hit.get("key").and_then(Value::as_str).unwrap_or("");
        let score = hit.get("score").and_then(Value::as_f64).unwrap_or(0.0);
        let meta: Option<Value> = hit
            .get("value")
            .and_then(Value::as_str)
            .and_then(|raw| serde_json::from_str(raw).ok());
        let name = meta
            .as_ref()
            .and_then(|v| v.get("name"))
            .and_then(Value::as_str)
            .unwrap_or(key);
        lines.push(format!("{}. {}（score {:.4}）", index + 1, name, score));

        let lib_name = meta
            .as_ref()
            .and_then(|v| v.get("lib_name"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let ffi_symbol = meta
            .as_ref()
            .and_then(|v| v.get("ffi_symbol"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if !lib_name.is_empty() || !ffi_symbol.is_empty() {
            lines.push(format!(
                "   lib_name: {}, ffi_symbol: {}",
                lib_name, ffi_symbol
            ));
        }
    }
    lines.join("\n")
}

/// 没有可用聊天插件时的提示。
fn install_plugin_prompt() -> String {
    "未找到可用的聊天插件，请先安装插件：deepseek 或 siliconflow。".to_string()
}

/// 从 `chat_plugin` 命中记录中取出插件调用信息。
fn plugin_target(hit: &Value) -> Option<(String, String)> {
    let meta: Value = hit
        .get("value")
        .and_then(Value::as_str)
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or(Value::Null);
    let lib_name = meta.get("lib_name").and_then(Value::as_str)?.to_string();
    let ffi_symbol = meta.get("ffi_symbol").and_then(Value::as_str)?.to_string();
    Some((lib_name, ffi_symbol))
}

/// 调用 Top1 聊天插件，把当前对话内容交给它并取回 content。
///
/// 插件不可调用时返回 `Ok(None)`，由调用方回退到候选列表文本。
fn call_top_chat_plugin(
    hits: &[Value],
    prompt: &str,
    uuid: &str,
) -> anyhow::Result<Option<String>> {
    let Some(hit) = hits.first() else {
        return Ok(None);
    };
    let Some((lib_name, ffi_symbol)) = plugin_target(hit) else {
        return Ok(None);
    };

    let messages: Vec<Value> = message_history(uuid)?
        .into_iter()
        .filter_map(|message| {
            let role = message.get("role")?.as_str()?;
            let content = message.get("content")?.as_str()?;
            if content.is_empty() {
                return None;
            }
            Some(serde_json::json!({ "role": role, "content": content }))
        })
        .collect();
    let request = serde_json::json!({
        "prompt": prompt,
        "messages": messages
    });

    let data = crate::utils::plugin::call(&lib_name, &ffi_symbol, &request)?;
    Ok(data
        .get("content")
        .and_then(Value::as_str)
        .filter(|content| !content.trim().is_empty())
        .map(str::to_string))
}

/// 一次 chat 只负责对话与消息持久化：
/// 1) 校验 uuid
/// 2) 新建 / 续聊
/// 3) 可选写入显式 system_prompt
/// 4) 写 user 消息
/// 5) 读取 dialogue 的 system_prompt / session_id 返回
///
/// 不再调用 siliconflow，也不注入 skills；assistant 消息由宿主 / 其他服务写入。
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
    let target = resolve_chat_target(
        uuid,
        plugin_uuid,
        options.dialogue_id.as_deref(),
        options.show_status,
    );

    let (uuid, created) = match target {
        ChatTarget::Existing(id) => {
            if !is_valid_uuid(&id) {
                return Err(ChatFlowError::InvalidUuid(id));
            }
            match get_dialogue(&id) {
                Ok(Some(_)) => (id, false),
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
                    Ok(Some(_)) => (existing_id.to_string(), false),
                    Ok(None) => (
                        create_new_chat(
                            id.as_deref(),
                            show_status,
                            name,
                            prompt,
                            keyword,
                            options,
                            plugin_uuid,
                        )?,
                        true,
                    ),
                    Err(e) => return Err(ChatFlowError::LoadDialogue(e.to_string())),
                }
            } else {
                (
                    create_new_chat(
                        id.as_deref(),
                        show_status,
                        name,
                        prompt,
                        keyword,
                        options,
                        plugin_uuid,
                    )?,
                    true,
                )
            }
        }
    };

    if !created {
        if let Some(kw) = keyword {
            if let Err(e) = update_keyword(&uuid, kw) {
                return Err(ChatFlowError::UpdateKeyword(e.to_string()));
            }
        }
        if let Some(system_prompt) = options.system_prompt.as_deref() {
            let patch = DialoguePatch {
                title: None,
                keyword: None,
                model_name: None,
                system_prompt: Some(system_prompt),
                model_config: None,
                is_pinned: None,
                is_archived: None,
                session_id: None,
                show_status: None,
            };
            update_dialogue(&uuid, &patch)
                .map_err(|e| ChatFlowError::LoadDialogue(e.to_string()))?;
        }
    }

    if let Err(e) = append_message(&uuid, "user", prompt) {
        return Err(ChatFlowError::AppendUser(e.to_string()));
    }

    // 用户消息落库后，检索 chat 分类插件：
    // - 无候选：提示安装插件；
    // - 有候选：调用 Top1 插件，把对话内容交给它并写回 assistant；
    // - 插件不可调用：回退为候选列表文本。
    let plugin_reply = match crate::utils::kv::search_chat_plugins(prompt, CHAT_PLUGIN_TOP_K) {
        Ok(hits) if hits.is_empty() => install_plugin_prompt(),
        Ok(hits) => match call_top_chat_plugin(&hits, prompt, &uuid) {
            Ok(Some(content)) => content,
            Ok(None) => format_plugin_reply(&hits),
            Err(e) => {
                eprintln!("[chat] 调用聊天插件失败: {:#}", e);
                format!("{:#}", e)
            }
        },
        Err(e) => format!("chat 插件检索失败：{:#}", e),
    };
    if let Err(e) = append_message(&uuid, "assistant", &plugin_reply) {
        eprintln!("[chat] 写入插件检索结果失败: {:#}", e);
    }

    let dialogue = match get_dialogue(&uuid) {
        Ok(Some(row)) => row,
        Ok(None) => {
            return Err(ChatFlowError::LoadDialogue(format!("对话不存在: {}", uuid)));
        }
        Err(e) => return Err(ChatFlowError::LoadDialogue(e.to_string())),
    };
    let system_prompt = non_empty(dialogue.get("system_prompt"));
    let session_id = non_empty(dialogue.get("session_id"));

    Ok(ChatOutcome {
        uuid,
        created,
        content: String::new(),
        content_html: String::new(),
        session_id,
        system_prompt,
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

    #[test]
    fn format_plugin_reply_without_hits() {
        assert!(format_plugin_reply(&[]).contains("请先安装插件"));
    }

    #[test]
    fn plugin_target_reads_metadata() {
        let hit = serde_json::json!({
            "key": "chat-plugin-do-test-chat",
            "value": "{\"lib_name\":\"do_test_chat\",\"ffi_symbol\":\"chat\"}"
        });
        assert_eq!(
            plugin_target(&hit),
            Some(("do_test_chat".to_string(), "chat".to_string()))
        );
        assert!(plugin_target(&serde_json::json!({ "value": "not-json" })).is_none());
    }

    #[test]
    fn format_plugin_reply_lists_plugin_metadata() {
        let hits = vec![
            serde_json::json!({
                "key": "chat-plugin-deepseek",
                "score": 0.91234,
                "value": "{\"name\":\"DeepSeek\",\"lib_name\":\"libdeepseek\",\"ffi_symbol\":\"chat\"}"
            }),
            serde_json::json!({ "key": "chat-plugin-broken", "score": 0.4, "value": "not-json" }),
        ];
        let reply = format_plugin_reply(&hits);

        assert!(reply.contains("找到 2 个匹配的 chat 插件"));
        assert!(reply.contains("1. DeepSeek（score 0.9123）"));
        assert!(reply.contains("lib_name: libdeepseek, ffi_symbol: chat"));
        // value 不是合法 JSON 时退化为 key。
        assert!(reply.contains("2. chat-plugin-broken"));
    }
}
