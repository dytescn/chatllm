// src/ffi/types.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct ChatReq {
    pub prompt: String,
    pub max_tokens: Option<i32>,
    pub uuid: Option<String>,
    pub name: Option<String>,
    pub keyword: Option<String>,
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

