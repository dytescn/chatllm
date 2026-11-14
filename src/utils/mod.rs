pub mod chat_plugins; // chat_plugins 阶段的逐条插件调用
pub mod cjson; // ffi对话接口
pub mod db; // 结构化数据库
pub mod designdb; // 对象数据库
pub mod dialogue; // 对话内容
pub mod errcode; // 业务错误码
pub mod kv; // 用于对象查询
pub mod message; // 对话消息
pub mod plugins_lib; // designplugin_lib 的 FFI 客户端
pub mod reply; // 助手回复组装
pub mod run_flow; // 正常对话的流程
pub mod taskflow; // 插件任务流读取
pub mod think; // 模型思考内容
