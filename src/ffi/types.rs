// src/ffi/types.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct ChatReq {
    pub prompt: String,
    pub max_tokens: Option<i32>,
    pub uuid: Option<String>,
    /// 新建对话时直接指定 `dialogue.id`（如插件对话传 `plugin_uuid`）
    pub dialogue_id: Option<String>,
    /// 插件 UUID：等价于指定 `dialogue.id`，并默认注入该插件的 skills
    pub plugin_uuid: Option<String>,
    /// 新建对话的显示状态：1 正常，2 不显示（插件对话默认 2）
    pub show_status: Option<i64>,
    pub name: Option<String>,
    pub keyword: Option<String>,
    pub use_skills: Option<bool>,
    pub skill_categories: Option<Vec<String>>,
    pub skill_ids: Option<Vec<String>>,
    pub system_prompt: Option<String>,
    pub refresh_system_prompt: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DialogueUpdateReq {
    pub id: String,
    pub title: Option<String>,
    pub keyword: Option<String>,
    pub model_name: Option<String>,
    pub system_prompt: Option<String>,
    pub model_config: Option<String>,
    pub is_pinned: Option<i64>,
    pub is_archived: Option<i64>,
    pub session_id: Option<String>,
    pub show_status: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DialogueSearchReq {
    pub keyword: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DialogueListReq {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChatDetailReq {
    pub uuid: String,
    pub before_id: Option<i64>,
    pub after_id: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TurboSearchReq {
    pub query: String,
    pub top_k: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_req_parses_plugin_dialogue_fields() {
        let req: ChatReq = serde_json::from_str(
            r#"{"prompt":"hi","plugin_uuid":"11111111-1111-4111-8111-111111111111","show_status":2}"#,
        )
        .unwrap();
        assert_eq!(
            req.plugin_uuid.as_deref(),
            Some("11111111-1111-4111-8111-111111111111")
        );
        assert_eq!(req.show_status, Some(2));
        assert!(req.dialogue_id.is_none());

        let req: ChatReq =
            serde_json::from_str(r#"{"prompt":"hi","dialogue_id":"custom-id"}"#).unwrap();
        assert_eq!(req.dialogue_id.as_deref(), Some("custom-id"));
        assert!(req.plugin_uuid.is_none());
    }
}
