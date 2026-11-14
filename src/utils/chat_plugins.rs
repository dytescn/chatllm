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
//! 依赖：`kv`（集合检索）、`designdb`（taskflow）、插件动态库（`./plugins/` 或
//! `./libary/`）；插件管理接口见 [`crate::utils::plugins_lib`]。

use anyhow::{anyhow, bail, Context, Result};
use libloading::Library;
use serde_json::{json, Value};
use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::utils::{kv, taskflow};

/// 插件检索命中数量：每条 prompt 只取一条。
pub const TOP_K: i64 = 5;
/// 插件未指定符号时的默认入口。
const DEFAULT_SYMBOL: &str = "chat";
/// 与插件 `write_json_out` 保持一致的输出缓冲。
const OUT_BUF_SIZE: usize = 65536;

/// 阶段使用的 kv 集合。
pub const COLLECTION: &str = kv::COLLECTION_CHAT_PLUGINS;

#[cfg(windows)]
const DYLIB_EXT: &str = "dll";
#[cfg(target_os = "linux")]
const DYLIB_EXT: &str = "so";
#[cfg(target_os = "macos")]
const DYLIB_EXT: &str = "dylib";

type FfiFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

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
    fn to_json(&self) -> Value {
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

impl ChatPluginsOutcome {
    /// 生成轮次记录里的 `plugins` 段。
    pub fn to_json(&self) -> Value {
        json!({
            "start_time": self.start_time,
            "inputs": self.inputs,
            "calls": self.calls.iter().map(ChatPluginCall::to_json).collect::<Vec<_>>(),
            "outputs": self.outputs,
            "end_time": self.end_time,
            "status": self.status,
        })
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
    if plugin_call_active() {
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
        match call_plugin_response(&plugin, &callback, &request) {
            Ok(response) => {
                let content = content_of_response(&response);
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

/// 截断长文本，避免日志刷屏。
pub fn preview(value: &str, max: usize) -> String {
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

/// 读取元数据里的插件入口，缺省为 [`DEFAULT_SYMBOL`]。
pub fn symbol_of(meta: &Value) -> String {
    meta.get("ffi_symbol")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_SYMBOL)
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

/// 按 `designplugin_lib` 的安装约定探测插件动态库。
pub fn candidates(plugin: &str) -> [PathBuf; 4] {
    [
        PathBuf::from(format!("./plugins/{0}/{0}.{1}", plugin, DYLIB_EXT)),
        PathBuf::from(format!("./plugins/{0}/lib{0}.{1}", plugin, DYLIB_EXT)),
        PathBuf::from(format!("./libary/{0}.{1}", plugin, DYLIB_EXT)),
        PathBuf::from(format!("./libary/lib{0}.{1}", plugin, DYLIB_EXT)),
    ]
}

/// 已加载的插件库缓存：插件名 → 动态库。
///
/// 加载后不主动卸载，避免反复 `dlopen` / `dlclose` 触发 loader lock，
/// 也让插件内的静态状态得以保留。
static PLUGIN_LIBS: OnceLock<Mutex<HashMap<String, Arc<Library>>>> = OnceLock::new();

fn lock_plugins() -> Result<MutexGuard<'static, HashMap<String, Arc<Library>>>> {
    PLUGIN_LIBS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| anyhow!("插件库缓存已损坏"))
}

/// 加载（或复用）插件动态库。
///
/// `dlopen` 不持缓存锁：插件加载可能触发 loader lock，持锁加载会与其他
/// 线程的锁形成交叉等待。锁只用于命中 / 回填缓存。
fn plugin_library(plugin: &str) -> Result<Arc<Library>> {
    if let Some(lib) = lock_plugins()?.get(plugin).cloned() {
        return Ok(lib);
    }

    let path = candidates(plugin)
        .into_iter()
        .find(|path| path.exists())
        .with_context(|| format!("找不到插件 {plugin}（./plugins/{plugin}/ 或 ./libary/）"))?;
    println!("[chat_plugins] 插件路径: {}", path.display());
    // SAFETY: 按安装约定加载受信任的插件动态库。
    let lib = Arc::new(
        unsafe { Library::new(&path) }
            .with_context(|| format!("加载插件 {} 失败", path.display()))?,
    );

    let mut cache = lock_plugins()?;
    // 双检：并发首加载时保留先写入的库，当前 Arc 在无人使用时自然释放。
    if let Some(existing) = cache.get(plugin) {
        return Ok(Arc::clone(existing));
    }
    cache.insert(plugin.to_string(), Arc::clone(&lib));
    Ok(lib)
}

thread_local! {
    /// 当前线程正在执行的插件调用层数。
    static PLUGIN_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// 当前线程是否正在执行插件。
///
/// 插件回调 chatllm 时，对话流程会再次进入插件调用；调用方可在
/// 前置阶段直接拒绝，避免递归加载与引擎锁死。
pub fn plugin_call_active() -> bool {
    PLUGIN_DEPTH.with(|depth| depth.get() > 0)
}

/// 插件调用重入保护：同一线程只允许一层插件调用。
pub struct PluginCallGuard;

impl PluginCallGuard {
    pub fn enter() -> Result<Self> {
        PLUGIN_DEPTH.with(|depth| {
            if depth.get() > 0 {
                bail!("检测到插件重入调用（当前线程已在执行插件）");
            }
            depth.set(depth.get() + 1);
            Ok(PluginCallGuard)
        })
    }
}

impl Drop for PluginCallGuard {
    fn drop(&mut self) {
        PLUGIN_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// 执行插件：dlopen 后调用 `post_<symbol>`，返回响应 `data.content`。
pub fn call_plugin(plugin: &str, symbol: &str, input: &str) -> Result<String> {
    let response = call_plugin_response(plugin, symbol, &json!({ "prompt": input }))?;
    Ok(content_of_response(&response))
}

/// 执行插件并返回完整响应 JSON（含 `code` / `data` / `msg`）。
///
/// 需要读取 `data.prompts` / `data.outputs` / `data.input` 等字段时使用本函数。
pub fn call_plugin_response(plugin: &str, symbol: &str, request: &Value) -> Result<Value> {
    invoke_plugin(plugin, symbol, request)
}

/// 取插件响应 `data.content`。
pub fn content_of_response(value: &Value) -> String {
    value
        .get("data")
        .and_then(|data| data.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// 插件入口符号：`chat` → `post_chat`。
pub fn ffi_entry(symbol: &str) -> String {
    format!("post_{symbol}")
}

/// 检查插件 FFI 返回值：`0` 已写入，`-2` 缓冲区不足。
pub fn check_plugin_ret(plugin: &str, ret: c_int) -> Result<()> {
    match ret {
        0 => Ok(()),
        -2 => bail!("插件 {} 输出超过 {} KiB", plugin, OUT_BUF_SIZE / 1024),
        other => bail!("插件 {} FFI 返回 {}", plugin, other),
    }
}

/// 解析插件响应；`code` 为 `200` 或 `0` 都视为成功。
pub fn parse_plugin_response(plugin: &str, out: &str) -> Result<Value> {
    let value: Value = serde_json::from_str(out)
        .with_context(|| format!("插件 {} 响应解析失败: {}", plugin, out))?;

    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 && code != 200 {
        let msg = value
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        println!("[chat_plugins] 插件错误: code={}, msg={}", code, msg);
        bail!("插件 {} 错误 (code={}): {}", plugin, code, msg);
    }

    Ok(value)
}

/// 解析插件入口，必要时回退到 `post_chat`。
fn plugin_entry<'a>(
    lib: &'a Library,
    plugin: &str,
    symbol: &str,
) -> Result<libloading::Symbol<'a, FfiFn>> {
    let entry = ffi_entry(symbol);
    // SAFETY: 入口签名按 `extern "C" fn(*const c_char, *mut c_char) -> c_int` 使用。
    match unsafe { lib.get::<FfiFn>(entry.as_bytes()) } {
        Ok(function) => Ok(function),
        // 兼容旧 scan 记录：ffi_symbol 可能是 mcp tool 名，而插件实际入口是 post_chat。
        Err(first) if symbol != DEFAULT_SYMBOL => {
            println!(
                "[chat_plugins] dlsym {} 失败，回退 post_chat: {}",
                entry, first
            );
            unsafe { lib.get::<FfiFn>(b"post_chat") }.with_context(|| {
                format!("dlsym {} / post_chat 失败（{}）: {}", entry, plugin, first)
            })
        }
        Err(first) => Err(anyhow!("dlsym {} 失败（{}）: {}", entry, plugin, first)),
    }
}

/// 调用插件并返回完整响应 JSON（含 `code` / `data` / `msg`）。
fn invoke_plugin(plugin: &str, symbol: &str, request: &Value) -> Result<Value> {
    // 重入保护：插件回调 chatllm 时不允许再次进入插件调用。
    let _guard = PluginCallGuard::enter()?;
    let body = request.to_string();
    println!(
        "[chat_plugins] invoke_plugin: plugin={:?}, symbol={:?}, request={:?}",
        plugin,
        symbol,
        preview(&body, 400)
    );

    let lib = plugin_library(plugin)?;
    let function = plugin_entry(&lib, plugin, symbol)?;

    let req = CString::new(body).context("插件请求 JSON 含 \\0")?;
    let mut out = vec![0u8; OUT_BUF_SIZE];
    // SAFETY: 请求与输出缓冲区生命周期覆盖整个调用。
    let ret = unsafe { function(req.as_ptr(), out.as_mut_ptr() as *mut c_char) };
    println!("[chat_plugins] 插件 FFI 返回: {}", ret);
    check_plugin_ret(plugin, ret)?;

    let out = unsafe { CStr::from_ptr(out.as_ptr() as *const c_char) }
        .to_str()
        .context("插件响应非 UTF-8")?;
    println!("[chat_plugins] 插件原始响应: {:?}", preview(out, 500));
    parse_plugin_response(plugin, out)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
