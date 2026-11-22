// src/utils/chat_plugins.rs
//! chat_plugins 阶段：按 prompts 逐条检索 `chat_plugins` 并执行插件回调。
//!
//! 输入：`prompts[]`（来自 think 阶段的检索关键词）、`inputs[]`（think 输出，
//! 作为传给插件的文本）、开始时间。
//! 对每个 `prompts[i]`：
//!
//! 1. 用关键词检索 `chat_plugins`，只取 Top1；
//! 2. 把 `prompts[i]` / `inputs[i]` 传给该插件的任务流回调；
//! 3. 记录本条 `start_time` / `end_time`，由 [`ChatPluginsOutcome::to_json`]
//!    写入当前轮次的 `plugins.calls`；
//! 4. 下一条的开始时间取上一条的结束时间；插件返回 `data.input` 时可更新
//!    下一条的输入（`inputs` 可变）。
//!
//! 最终对外只返回计算后的 `outputs[]`（不再返回 `prompts[]`）。
//!
//! 依赖：`kv`（集合检索）、`designdb`（taskflow）、插件管理接口见
//! [`crate::utils::plugins_lib`]；插件方法回调见
//! [`crate::utils::plugins_callback`]。

use serde_json::{json, Value};

use crate::utils::time::now_ms;
use crate::utils::{kv, plugins_callback, taskflow};

/// 插件检索命中数量：每条 prompt 只取一条。
pub const TOP_K: i64 = 5;
/// 阶段使用的 kv 集合。
pub const COLLECTION: &str = kv::COLLECTION_CHAT_PLUGINS;

/// 单条 `prompts[i]` 的执行记录，写入轮次记录的 `plugins.calls`。
#[derive(Debug, Clone, Default)]
pub struct ChatPluginCall {
    pub step: usize,
    pub prompt: String,
    pub input: String,
    pub plugin: Option<String>,
    pub plugin_uuid: Option<String>,
    pub callbacks: Vec<String>,
    pub start_time: i64,
    pub end_time: i64,
    pub status: &'static str,
    pub response: String,
    pub error: Option<String>,
}

impl ChatPluginCall {
    pub fn to_json(&self) -> Value {
        json!({
            "step": self.step,
            "start_time": self.start_time,
            "plugin": self.plugin,
            "plugin_uuid": self.plugin_uuid,
            "prompt": self.prompt,
            "params": { "prompt": self.input, "keyword": self.prompt },
            "callbacks": self.callbacks,
            "response": self.response,
            "end_time": self.end_time,
            "status": self.status,
            "error": self.error,
        })
    }
}

/// chat_plugins 阶段结果。
#[derive(Debug, Clone, Default)]
pub struct ChatPluginsOutcome {
    pub start_time: i64,
    pub end_time: i64,
    /// `ok` / `skipped` / `error`。
    pub status: &'static str,
    /// 本阶段实际使用的输入（插件可通过 `data.input` 更新）。
    pub inputs: Vec<String>,
    /// 每条 prompt 的插件输出。
    pub outputs: Vec<String>,
    /// 逐条执行记录。
    pub calls: Vec<ChatPluginCall>,
}

/// plugins 阶段交给 reply 的结果。
#[derive(Debug, Clone, Default)]
pub struct PluginsRes {
    pub contents: Vec<String>,
}

impl ChatPluginsOutcome {
    /// 提取传给 reply 的 `contents`。
    pub fn res(&self) -> PluginsRes {
        PluginsRes {
            contents: self.outputs.clone(),
        }
    }
}

/// 执行 chat_plugins 阶段：逐条检索、调用插件并记录时间。
///
/// `start_time` 是第一条的开始时间；后续每条以上一条的 `end_time` 开始。
/// 未命中或调用失败只影响对应条目，不中断其余 prompt。
pub fn run(prompts: &[String], inputs: &[String], start_time: i64) -> ChatPluginsOutcome {
    println!(
        "[chat_plugins] run 开始: prompts={}, inputs={}, start_time={}",
        prompts.len(),
        inputs.len(),
        start_time
    );

    // 重入保护：插件回调 chatllm 时不再进入本阶段。
    if plugins_callback::plugin_call_active() {
        let end_time = now_ms();
        println!("[chat_plugins] 检测到插件重入调用，跳过本阶段");
        return ChatPluginsOutcome {
            start_time,
            end_time,
            status: "error",
            inputs: inputs.to_vec(),
            ..ChatPluginsOutcome::default()
        };
    }

    let mut effective_inputs = inputs.to_vec();
    let mut calls = Vec::with_capacity(prompts.len());
    let mut outputs = Vec::with_capacity(prompts.len());
    let mut cursor = start_time;

    for (index, prompt) in prompts.iter().enumerate() {
        let input = input_at(&effective_inputs, index);
        let (call, next_input) = run_one(index, prompt, &input, cursor, prompts, &effective_inputs);
        cursor = call.end_time;

        if let (Some(next_input), Some(slot)) = (next_input, effective_inputs.get_mut(index + 1)) {
            *slot = next_input;
        }

        outputs.push(call.response.clone());
        calls.push(call);
    }

    let end_time = if prompts.is_empty() {
        start_time
    } else {
        cursor
    };
    let status = stage_status(&calls);
    println!(
        "[chat_plugins] run 结束: status={}, outputs={}, 耗时 {} ms",
        status,
        outputs.len(),
        end_time - start_time
    );

    ChatPluginsOutcome {
        start_time,
        end_time,
        status,
        inputs: effective_inputs,
        outputs,
        calls,
    }
}

/// 执行单条 prompt，返回调用记录与插件给出的下一条输入（`data.input`）。
fn run_one(
    index: usize,
    prompt: &str,
    input: &str,
    start_time: i64,
    prompts: &[String],
    inputs: &[String],
) -> (ChatPluginCall, Option<String>) {
    let mut call = ChatPluginCall {
        step: index + 1,
        prompt: prompt.to_string(),
        input: input.to_string(),
        start_time,
        status: "skipped",
        ..ChatPluginCall::default()
    };
    println!(
        "[chat_plugins] [{}] prompt={:?}, input={:?}",
        call.step, prompt, input
    );

    let list = match kv::search_chat_plugins(prompt, TOP_K) {
        Ok(list) => list,
        Err(error) => {
            println!("[chat_plugins] [{}] 检索失败: {:#}", call.step, error);
            call.status = "error";
            call.error = Some(error.to_string());
            call.end_time = now_ms();
            return (call, None);
        }
    };

    // 只使用 Top1 命中。
    let Some(hit) = list.first() else {
        println!("[chat_plugins] [{}] 未命中 {}", call.step, COLLECTION);
        call.response = format!("未找到相关对话内容：{input}");
        call.end_time = now_ms();
        return (call, None);
    };

    call.plugin_uuid = plugin_uuid_of(hit);
    call.plugin = plugin_of(hit).map(|(plugin, _)| plugin);
    let (Some(plugin_uuid), Some(plugin)) = (call.plugin_uuid.clone(), call.plugin.clone()) else {
        println!("[chat_plugins] [{}] 命中记录缺少插件信息", call.step);
        call.end_time = now_ms();
        return (call, None);
    };

    let taskflows = match taskflow::for_plugin(&plugin_uuid) {
        Ok(items) => items,
        Err(error) => {
            println!(
                "[chat_plugins] [{}] taskflow 读取失败: {:#}",
                call.step, error
            );
            call.status = "error";
            call.error = Some(error.to_string());
            call.end_time = now_ms();
            return (call, None);
        }
    };

    let mut results = Vec::new();
    let mut next_input = None;
    let mut failed = false;

    for item in &taskflows {
        let Some(callback) = taskflow::callback_of(item) else {
            continue;
        };
        call.callbacks.push(callback.clone());
        let request = plugin_request(prompt, input, prompts, inputs, index);
        match plugins_callback::call_plugin_response(&plugin, &callback, &request) {
            Ok(response) => {
                let content = plugins_callback::content_of_response(&response);
                if !content.trim().is_empty() {
                    results.push(content);
                }
                if next_input.is_none() {
                    next_input = response
                        .pointer("/data/input")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
            }
            Err(error) => {
                failed = true;
                println!(
                    "[chat_plugins] [{}] 回调 {} 失败: {:#}",
                    call.step, callback, error
                );
                results.push(format!("插件处理失败：{:#}", error));
            }
        }
    }

    call.response = join_results(&results).unwrap_or_default();
    call.status = if failed {
        "error"
    } else if results.is_empty() {
        "skipped"
    } else {
        "ok"
    };
    call.end_time = now_ms();
    (call, next_input)
}

/// 构造单条插件请求：关键词、输入、当前 prompts / inputs 与步骤。
pub fn plugin_request(
    prompt: &str,
    input: &str,
    prompts: &[String],
    inputs: &[String],
    index: usize,
) -> Value {
    json!({
        "prompt": input,
        "keyword": prompt,
        "prompts": prompts,
        "inputs": inputs,
        "step": index + 1
    })
}

/// 取第 `index` 条输入；越界时复用最后一条，空数组返回空串。
pub fn input_at(inputs: &[String], index: usize) -> String {
    inputs
        .get(index)
        .or_else(|| inputs.last())
        .cloned()
        .unwrap_or_default()
}

/// 汇总各条状态：任一条 error 即 error；全部 skipped 才是 skipped。
pub fn stage_status(calls: &[ChatPluginCall]) -> &'static str {
    if calls.is_empty() {
        return "skipped";
    }
    if calls.iter().any(|call| call.status == "error") {
        "error"
    } else if calls.iter().all(|call| call.status == "skipped") {
        "skipped"
    } else {
        "ok"
    }
}

/// 从 `chat_plugins` 命中记录中解析插件 uuid。
///
/// 兼容 `meta.plugin_uuid`、`value.plugin_uuid`、`value.uuid` 与记录顶层字段。
pub fn plugin_uuid_of(hit: &Value) -> Option<String> {
    let meta = hit.get("meta");
    let value = hit
        .get("value")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str::<Value>(text).ok());

    str_at(meta, "plugin_uuid")
        .or_else(|| str_at(value.as_ref(), "plugin_uuid"))
        .or_else(|| str_at(value.as_ref(), "uuid"))
        .or_else(|| str_at(Some(hit), "plugin_uuid"))
        .or_else(|| str_at(Some(hit), "uuid"))
}

fn str_at(value: Option<&Value>, key: &str) -> Option<String> {
    value
        .and_then(|v| v.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// 合并多段结果；空结果返回 `None`。
pub fn join_results(results: &[String]) -> Option<String> {
    let text = results
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// 读取元数据里的插件入口，缺省为 [`DEFAULT_SYMBOL`]。
pub fn symbol_of(meta: &Value) -> String {
    meta.get("ffi_symbol")
        .and_then(Value::as_str)
        .unwrap_or(plugins_callback::DEFAULT_SYMBOL)
        .to_string()
}

/// 从命中记录里解析插件：`meta.plugin`，或 `value` 里的 `lib_name`。
pub fn plugin_of(hit: &Value) -> Option<(String, String)> {
    if let Some(meta) = hit.get("meta") {
        if let Some(plugin) = meta.get("plugin").and_then(Value::as_str) {
            return Some((plugin.to_string(), symbol_of(meta)));
        }
    }

    let meta: Value = serde_json::from_str(hit.get("value")?.as_str()?).ok()?;
    let plugin = meta.get("lib_name").and_then(Value::as_str)?;
    Some((plugin.to_string(), symbol_of(&meta)))
}

/// 取命中记录里的非空 `value`，用于没有插件时的回退文本。
pub fn content_of(hit: &Value) -> Option<String> {
    hit.get("value")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}
