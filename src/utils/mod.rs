pub mod chat;    // 最简对话流程（kv chat 分类 + 插件）
pub mod cjson; // ffi对话接口
pub mod cloud;   // 云端模型调用
pub mod db;    // 结构化数据库
pub mod designdb;  // 对象数据库
pub mod dialogue;  // 对话内容
pub mod errcode;   // 业务错误码
pub mod kv;       // 用于对象查询
pub mod markparse;  // 用于内容解析
pub mod message;  // 对话消息
pub mod reply;    // 助手回复组装
pub mod think;    // 模型思考内容
