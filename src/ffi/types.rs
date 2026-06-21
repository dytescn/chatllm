use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct ChatReq {
    pub prompt: String,
    pub max_tokens: Option<i32>,
}
