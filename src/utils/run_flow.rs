// src/utils/run_flow.rs
//! 对话流程：`contents → think → plugins → reply → content`。
//!
//! - 输入已存在的 `dialogue_uid` 与文字内容；
//! - think 生成 `think_res(prompts, contents)`；
//! - plugins 接收 `think_res`，生成 `plugins_res(contents)`；
//! - reply 处理 `plugins_res.contents`，返回最终内容。
//!
//! 本模块只负责流程，不负责对话的生命周期：新建 / 续聊、写 user 消息由
//! [`crate::utils::chat::chat_message`] 完成，FFI 层只做参数解析与响应写回。
//!
//! 轮次记录按阶段增量写入：think 完成后先落库，plugins / reply 完成后补齐。
//! 各阶段的轮次 JSON 由对应模块生成（think / taskflow / reply）。

use crate::ffi::types::ChatDialogue;
use crate::utils::dialogue;
use crate::utils::errcode::ChatError;
use crate::utils::message;
use crate::utils::reply;
use crate::utils::{chat_plugins, skills, think};
use serde_json::{json, Value};

use crate::utils::time::now_ms;

/// 在已有对话上执行一次正常对话。
///
/// `req.uuid` 由调用方（[`crate::utils::chat::chat_message`]）保证已存在，
/// 本函数不再校验其真实性。
pub fn run(req: &ChatDialogue) -> Result<Value, ChatError> {
    // 输入：本轮文字内容。
    let contents = vec![req.content.clone()];

    // 对话生命周期由调用方负责，这里直接使用。
    let dialogue_uid = req.uuid.as_deref().unwrap_or_default();

    // think 阶段：命中结果用于读取 skills，并写入对话的 system prompt。
    let think = think::think_run(&contents[0], think::TOP_K);

    let system_prompt = think
        .plugins_uid
        .as_deref()
        .and_then(|uid| skills::skills_for(Some(uid), None));

    if let Some(system_prompt) = system_prompt.as_deref() {
        dialogue::update_system_prompt(dialogue_uid, system_prompt)
            .map_err(|e| ChatError::Load(e.to_string()))?;
    }

    // think 已完成：先写入轮次记录，plugins / reply 阶段再补齐。
    let think_json = think.to_round_json();
    let mut round_content = json!([{ "think": think_json.clone() }]);
    let round = message::start_round(dialogue_uid, &contents[0], round_content.clone())
        .map_err(|e| ChatError::Append(e.to_string()))?;
    message::append_phase(dialogue_uid, "think", think_json)
        .map_err(|e| ChatError::Append(e.to_string()))?;

    // think → plugins：prompts 用于检索，outputs 作为输入。
    let chat_phase = chat_plugins::run(&think.prompts, &think.outputs, think.end_time);
    let plugins_res = chat_phase.res();

    let plugins_json = chat_phase.to_json();
    round_content[0]["plugins"] = plugins_json.clone();
    message::update_round(dialogue_uid, &round, &round_content, None)
        .map_err(|e| ChatError::Append(e.to_string()))?;
    message::append_phase(dialogue_uid, "plugins", plugins_json)
        .map_err(|e| ChatError::Append(e.to_string()))?;

    // plugins_res.contents → reply → 最终内容。
    let reply_input = chat_plugins::join_results(&plugins_res.contents).unwrap_or_default();
    let reply_start = now_ms();
    let reply = reply::from_contents(&plugins_res.contents, &contents[0]);
    let reply_end = now_ms();

    let reply_json = reply.to_round_json(reply_start, reply_end, &reply_input);
    round_content[0]["reply"] = reply_json.clone();
    message::update_round(dialogue_uid, &round, &round_content, Some(&reply.content))
        .map_err(|e| ChatError::Append(e.to_string()))?;
    message::append_phase(dialogue_uid, "reply", reply_json)
        .map_err(|e| ChatError::Append(e.to_string()))?;

    dialogue::append_assistant(dialogue_uid, &reply.content)?;

    Ok(json!({
        "uuid": dialogue_uid,
        "content": reply.content,
    }))
}
