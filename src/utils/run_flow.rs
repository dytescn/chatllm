// src/utils/run_flow.rs
//! 正常对话流程：`check → think → plugins → reply`。
//!
//! FFI 层只负责参数解析与响应写回；新建 / 续聊、写 user 与 assistant 消息、
//! 各阶段检索与调用都集中在这里。

use crate::ffi::types::ChatFlow;
use crate::utils::check;
use crate::utils::dialogue;
use crate::utils::errcode::ChatError;
use crate::utils::message;
use crate::utils::reply::Reply;
use crate::utils::{plugins, reply_base, think};
use serde_json::{json, Value};

/// 执行一次正常对话。
pub fn run(req: &ChatFlow) -> Result<Value, ChatError> {
    // check 阶段：配置缺失时直接返回，不创建对话、不写消息。
    if let Some(message) = check::missing_supplier_reply() {
        println!("[run_flow] check 未通过，直接返回设置提示");
        return Ok(json!({
            "uuid": null,
            "created": false,
            "prompt": req.prompt,
            "max_tokens": req.max_tokens,
            "route": check::SETTING_ROUTE,
            "content": message,
            "content_html": "",
        }));
    }

    // think 阶段：命中 chat_think 后取出资源包 uuid/name，用于定位对话与 skills。
    let hit = match think::find(&req.prompt, think::TOP_K) {
        Ok(hit) => hit,
        Err(error) => {
            println!("[run_flow] think 检索失败: {:#}", error);
            None
        }
    };
    let thought = hit
        .as_ref()
        .and_then(|hit| hit.value.as_deref())
        .filter(|value| !value.trim().is_empty());
    let segments = thought.map(think::segments).unwrap_or_default();
    let think_plugin_uuid = hit.as_ref().and_then(|hit| hit.plugin_uuid.clone());
    let system_prompt = hit
        .as_ref()
        .and_then(|hit| hit.plugin_name.as_deref())
        .and_then(think::skills_for);

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

    // plugins 阶段：think 拆出的每一段独立检索并执行插件。
    let results = plugins::handle_many(&segments);
    let plugin_result = plugins::join_results(&results);

    // reply 阶段：合并插件结果生成最终 assistant 内容。
    let reply = Reply::from_result(plugin_result.clone(), &req.prompt);
    if !reply.used_reply_plugin {
        if let Some(input) = plugin_result.as_deref() {
            record_reply_exchange(input, &reply.content);
        }
    }

    dialogue::append_assistant(&uuid, &reply.content)?;

    Ok(json!({
        "uuid": uuid,
        "created": created,
        "prompt": req.prompt,
        "max_tokens": req.max_tokens,
        "route": Value::Null,
        "plugin_uuid": response_plugin_uuid,
    }))
}

/// 截断长文本，避免日志刷屏。
fn preview(value: &str, max: usize) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

/// 把 plugins 结果与 reply_base 清洗结果写入 reply_base 系统对话。
///
/// 对话 id 使用 reply_base uuid；不存在时创建，并把 skills 写入 system_prompt。
fn record_reply_exchange(input: &str, output: &str) {
    let Some(uuid) = reply_base::uuid() else {
        println!("[reply] reply_base 未安装，跳过系统对话写入");
        return;
    };
    let skills = reply_base::skills();
    println!(
        "[reply] 写入 reply_base 系统对话: uuid={}, system={}, input={:?}, output={:?}",
        uuid,
        skills.is_some(),
        preview(input, 200),
        preview(output, 200)
    );

    match dialogue::get_dialogue(&uuid) {
        Ok(Some(_)) => {}
        Ok(None) => {
            let title = "reply_base 系统对话";
            if let Err(error) =
                dialogue::create_dialogue_with(Some(&uuid), title, 2, skills.as_deref())
            {
                println!("[reply] 创建 reply_base 系统对话失败: {:#}", error);
                return;
            }
            println!("[reply] 已创建 reply_base 系统对话: {}", uuid);
        }
        Err(error) => {
            println!("[reply] 查询 reply_base 系统对话失败: {:#}", error);
            return;
        }
    }

    if !input.trim().is_empty() {
        if let Err(error) = message::append_message(&uuid, "user", input) {
            println!("[reply] 写入 reply_base user 消息失败: {:#}", error);
        }
    }
    if !output.trim().is_empty() {
        if let Err(error) = message::append_message(&uuid, "assistant", output) {
            println!("[reply] 写入 reply_base assistant 消息失败: {:#}", error);
        }
    }
}
