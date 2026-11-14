// src/utils/run_flow.rs
//! 正常对话流程：`think → plugins → reply`。
//!
//! FFI 层只负责参数解析与响应写回；新建 / 续聊、写 user 与 assistant 消息、
//! 各阶段检索与调用都集中在这里。

use crate::ffi::types::ChatFlow;
use crate::utils::dialogue;
use crate::utils::errcode::ChatError;
use crate::utils::message;
use crate::utils::reply::Reply;
use crate::utils::{chat_plugins, think};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

/// 执行一次正常对话。
pub fn run(req: &ChatFlow) -> Result<Value, ChatError> {
    // think 阶段：用默认分类关键词 base 检索 chat_think，把当前文本传给插件，
    // 汇总 prompts / outputs 并记录起止时间。
    let think = think::run(&req.prompt, think::TOP_K);

    let think_plugin_uuid = think.hit.as_ref().and_then(|hit| hit.plugin_uuid.clone());
    let system_prompt = think
        .hit
        .as_ref()
        .and_then(|hit| think::skills_for(hit.plugin_uuid.as_deref(), hit.plugin_name.as_deref()));

    // 只有调用方显式传入的 plugin_uuid 才能作为对话 id；think 命中的 uuid
    // 只用于标识插件 / 读取 skills，不能复用为对话 id，否则命中同一插件的
    // 多个对话会互相覆盖。
    let dialogue_plugin_uuid = req.plugin_uuid.as_deref();
    let response_plugin_uuid = req
        .plugin_uuid
        .clone()
        .or_else(|| think_plugin_uuid.clone());

    let (uuid, created) = dialogue::open_dialogue(
        &req.prompt,
        req.uuid.as_deref(),
        dialogue_plugin_uuid,
        req.title.as_deref(),
        req.show_status,
        system_prompt.as_deref(),
    )?;

    // chat_plugins 阶段：检索关键词直接来自 think 的 prompts，
    // 输入文本使用 think 的 outputs；每条以上一条的结束时间开始。
    let plugin_inputs = think.outputs.clone();
    let chat_phase = chat_plugins::run(&think.prompts, &plugin_inputs, think.end_time);
    let plugin_result = chat_plugins::join_results(&chat_phase.outputs);

    // reply 阶段：合并插件结果生成最终 assistant 内容。
    let reply_start = now_ms();
    let reply = Reply::from_result(plugin_result.clone(), &req.prompt);
    let reply_end = now_ms();

    dialogue::append_assistant(&uuid, &reply.content)?;

    let round = json!([
        {
            "think": {
                "start_time": think.start_time,
                "input": { "content": think.input },
                "prompts": think.prompts.clone(),
                "calls": [],
                "outputs": think.outputs.clone(),
                "end_time": think.end_time,
                "status": think.status
            },
            "plugins": chat_phase.to_json(),
            "reply": {
                "start_time": reply_start,
                "input": { "content": plugin_result.unwrap_or_default() },
                "calls": [],
                "outputs": [reply.content],
                "end_time": reply_end,
                "status": "ok"
            }
        }
    ]);
    message::append_round(&uuid, round).map_err(|e| ChatError::Append(e.to_string()))?;

    Ok(json!({
        "uuid": uuid,
        "created": created,
        "prompt": req.prompt,
        "max_tokens": req.max_tokens,
        "route": Value::Null,
        "plugin_uuid": response_plugin_uuid,
    }))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
