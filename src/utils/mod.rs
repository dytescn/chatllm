pub mod call_flow; // 回调对话的流程
pub mod check; // 对话前置检查（AI 供应商是否已设置）
pub mod cjson; // ffi对话接口
pub mod cloud; // 云端模型调用
pub mod db; // 结构化数据库
pub mod designdb; // 对象数据库
pub mod dialogue; // 对话内容
pub mod errcode; // 业务错误码
pub mod kv; // 用于对象查询
pub mod markparse; // 用于内容解析
pub mod message; // 对话消息
pub mod plugins; // 最简对话流程（kv chat 分类 + 插件）
pub mod reply; // 助手回复组装
pub mod reply_base; // reply_base 资源包与结果清洗
pub mod run_flow; // 正常对话的流程
pub mod think; // 模型思考内容
