// src/ffi/types.rs
use serde::{Deserialize, Serialize};

/// 对话元信息（`update_dialogue` 入参）
#[derive(Debug, Deserialize, Serialize)]
pub struct Dialogue {
    /// 对话 id；用于定位要更新的对话。兼容宿主传入 `id`。
    #[serde(alias = "id")]
    pub uuid: Option<String>,
    /// 插件 UUID：等价于对话 id
    pub plugin_uuid: Option<String>,
    /// 显示状态：1 正常，2 不显示
    pub show_status: Option<i64>,
    /// 对话标题
    pub title: Option<String>,
    /// 是否置顶：0 否，1 是
    pub is_pinned: Option<i64>,
    /// 是否归档：0 否，1 是
    pub is_archived: Option<i64>,
    /// 最大 token 数；仅在响应中回显，不参与对话逻辑
    pub max_tokens: Option<i32>,
}

/// 读取一条对话及其消息（`get_dialogue` 入参）
#[derive(Debug, Deserialize, Serialize)]
pub struct DialogueQuery {
    /// 对话 id（必填）
    pub uuid: String,
    /// 消息游标：读取 id < before_id 的更早消息
    pub before_id: Option<i64>,
    /// 消息游标：读取 id > after_id 的更新消息
    pub after_id: Option<i64>,
    /// 消息条数上限，默认 20，最大 100
    pub limit: Option<i64>,
}

/// 对话内容（`post_message` 入参）
#[derive(Debug, Deserialize, Serialize)]
pub struct ChatFlow {
    /// 用户输入
    pub prompt: String,
    /// 续聊已有对话的 id；非空时优先于新建
    pub uuid: Option<String>,
    /// 插件 UUID：新建时作为对话 id
    pub plugin_uuid: Option<String>,
    /// 新建对话的显示状态：1 正常，2 不显示
    pub show_status: Option<i64>,
    /// 对话标题；为空时取 prompt 前 20 字符
    pub title: Option<String>,
    /// 最大 token 数；仅在响应中回显，不参与对话逻辑
    pub max_tokens: Option<i32>,
}

/// 对话列表查询（`patch_dialogue` 入参）
#[derive(Debug, Deserialize, Serialize)]
pub struct DialogueListReq {
    /// 标题关键词；非空时按标题搜索
    pub keyword: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}
