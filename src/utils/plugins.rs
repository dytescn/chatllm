// src/utils/plugins.rs
//! 插件阶段：查询 kv_lib 的 `chat_plugin` 分类，并调用命中的插件。
//!
//! 输入是 think 阶段按 `|` 分割后选出的查询词；输出是插件返回的 `content`，
//! 交给 `reply` 阶段组装最终回复。
//!
//! 调用方式全部走本地动态库：
//! - kv：`utils::kv` → `./libary/kv_lib.dll`
//! - 插件：`designplugin_lib` 安装到 `./plugins/<name>/`，这里按该约定 dlopen。

use anyhow::{anyhow, bail, Context, Result};
use libloading::Library;
use serde_json::{json, Value};
use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use crate::utils::cloud::Supplier;
use crate::utils::kv;

/// 插件检索命中数量。
pub const TOP_K: i64 = 5;
/// 插件未指定符号时的默认入口。
const DEFAULT_SYMBOL: &str = "chat";
/// 与插件 `write_json_out` 保持一致的输出缓冲。
const OUT_BUF_SIZE: usize = 65536;

#[cfg(windows)]
const DYLIB_EXT: &str = "dll";
#[cfg(target_os = "linux")]
const DYLIB_EXT: &str = "so";
#[cfg(target_os = "macos")]
const DYLIB_EXT: &str = "dylib";

type FfiFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

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

/// 处理一条查询，返回插件结果或命中内容的回退文本。
pub fn handle(input: &str) -> Result<String> {
    println!("[plugins] handle 输入: {:?}", preview(input, 300));
    let list = match kv::search_chat_plugins(input, TOP_K) {
        Ok(list) => list,
        Err(error) => {
            println!("[plugins] chat_plugin 查询失败: {:#}", error);
            return Err(error);
        }
    };
    println!("[plugins] chat_plugin 命中 {} 条", list.len());
    for (index, hit) in list.iter().enumerate() {
        println!(
            "[plugins] hit[{}] key={:?}, score={}, meta={}, value={:?}",
            index,
            hit.get("key").and_then(Value::as_str),
            hit.get("score").and_then(Value::as_f64).unwrap_or(0.0),
            hit.get("meta")
                .map(|v| preview(&v.to_string(), 300))
                .unwrap_or_else(|| "null".to_string()),
            hit.get("value")
                .and_then(Value::as_str)
                .map(|v| preview(v, 300))
        );
    }

    let Some(hit) = list.first() else {
        println!("[plugins] 未命中插件，返回回退文本");
        return Ok(format!("未找到相关对话内容：{input}"));
    };

    if let Some((plugin, symbol)) = plugin_of(hit) {
        println!(
            "[plugins] 选中插件: plugin={:?}, symbol={:?}",
            plugin, symbol
        );
        return call_plugin(&plugin, &symbol, input);
    }

    let fallback = content_of(hit).unwrap_or_else(|| format!("未找到相关对话内容：{input}"));
    println!(
        "[plugins] 命中记录无插件，返回内容: {:?}",
        preview(&fallback, 300)
    );
    Ok(fallback)
}

/// 逐段查询并执行插件；返回每段的结果文本。
///
/// think 阶段按 `|` 拆分出的数组直接传入这里；单段失败不会阻断其它段，
/// 错误文本也会作为该段结果保留，最终统一交给 reply。
pub fn handle_many(queries: &[&str]) -> Vec<String> {
    println!("[plugins] handle_many 输入数组: {:?}", queries);
    let results = queries
        .iter()
        .enumerate()
        .map(|(index, query)| {
            println!(
                "[plugins] 处理第 {} 段: {:?}",
                index + 1,
                preview(query, 200)
            );
            match handle(query) {
                Ok(content) => {
                    println!(
                        "[plugins] 第 {} 段结果: {:?}",
                        index + 1,
                        preview(&content, 300)
                    );
                    content
                }
                Err(error) => {
                    println!("[plugins] 第 {} 段失败 ({query}): {:#}", index + 1, error);
                    format!("插件处理失败：{:#}", error)
                }
            }
        })
        .collect::<Vec<_>>();
    println!("[plugins] handle_many 完成，共 {} 段结果", results.len());
    results
}

/// 合并多段插件结果；空结果返回 `None`。
pub fn join_results(results: &[String]) -> Option<String> {
    let text = results
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.is_empty() {
        println!("[plugins] join_results: 无有效结果");
        None
    } else {
        println!("[plugins] join_results: {:?}", preview(&text, 500));
        Some(text)
    }
}

/// 从命中记录里解析插件：`meta.plugin`，或 `value` 里的 `lib_name`。
pub fn plugin_of(hit: &Value) -> Option<(String, String)> {
    if let Some(meta) = hit.get("meta") {
        if let Some(plugin) = meta.get("plugin").and_then(Value::as_str) {
            let symbol = meta
                .get("ffi_symbol")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_SYMBOL);
            return Some((plugin.to_string(), symbol.to_string()));
        }
    }

    let value = hit.get("value").and_then(Value::as_str)?;
    let meta: Value = serde_json::from_str(value).ok()?;
    let plugin = meta.get("lib_name").and_then(Value::as_str)?;
    let symbol = meta
        .get("ffi_symbol")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_SYMBOL);
    Some((plugin.to_string(), symbol.to_string()))
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
fn candidates(plugin: &str) -> Vec<PathBuf> {
    vec![
        PathBuf::from(format!("./plugins/{0}/{0}.{1}", plugin, DYLIB_EXT)),
        PathBuf::from(format!("./plugins/{0}/lib{0}.{1}", plugin, DYLIB_EXT)),
        PathBuf::from(format!("./libary/{0}.{1}", plugin, DYLIB_EXT)),
        PathBuf::from(format!("./libary/lib{0}.{1}", plugin, DYLIB_EXT)),
    ]
}

/// 已加载的插件库缓存：插件名 → 动态库。
///
/// 与 `cloud` 的供应商库缓存一致，加载后不主动卸载，避免反复
/// `dlopen` / `dlclose` 触发 loader lock，也让插件内的静态状态得以保留。
static PLUGIN_LIBS: OnceLock<Mutex<HashMap<String, Arc<Library>>>> = OnceLock::new();

/// 加载（或复用）插件动态库。
///
/// `dlopen` 不持缓存锁：插件加载可能触发 loader lock，持锁加载会与其他
/// 线程的锁形成交叉等待。锁只用于命中 / 回填缓存。
fn plugin_library(plugin: &str) -> Result<Arc<Library>> {
    let cache = PLUGIN_LIBS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(lib) = cache
        .lock()
        .map_err(|_| anyhow!("插件库缓存已损坏"))?
        .get(plugin)
        .cloned()
    {
        return Ok(lib);
    }

    let path = candidates(plugin)
        .into_iter()
        .find(|path| path.exists())
        .with_context(|| format!("找不到插件 {plugin}（./plugins/{plugin}/ 或 ./libary/）"))?;
    println!("[plugins] 插件路径: {}", path.display());
    let lib = Arc::new(
        unsafe { Library::new(&path) }
            .with_context(|| format!("加载插件 {} 失败", path.display()))?,
    );

    let mut cache = cache.lock().map_err(|_| anyhow!("插件库缓存已损坏"))?;
    // 双检：并发首加载时保留先写入的库，当前 Arc 在无人使用时自然释放。
    if let Some(existing) = cache.get(plugin) {
        return Ok(Arc::clone(existing));
    }
    cache.insert(plugin.to_string(), Arc::clone(&lib));
    Ok(lib)
}

thread_local! {
    /// 当前线程正在执行的插件调用层数。
    static PLUGIN_DEPTH: Cell<usize> = Cell::new(0);
}

/// 当前线程是否正在执行插件。
///
/// 插件回调 chatllm 时，`post_call_chat` 会再次进入插件调用；调用方可在
/// 前置阶段直接拒绝，避免递归加载与引擎锁死。
pub fn plugin_call_active() -> bool {
    PLUGIN_DEPTH.with(|depth| depth.get() > 0)
}

/// 插件调用重入保护：同一线程只允许一层插件调用。
pub(crate) struct PluginCallGuard;

impl PluginCallGuard {
    pub(crate) fn enter() -> Result<Self> {
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
    let response = invoke_plugin(plugin, symbol, &json!({ "prompt": input }))?;
    Ok(content_of_response(&response))
}

/// 执行插件并原样返回响应 `data`。
///
/// 用于 `post_call_chat`：插件接收调用方给出的完整 JSON，而不是 `prompt` 包装。
pub fn call_plugin_data(plugin: &str, symbol: &str, request: &Value) -> Result<Value> {
    println!(
        "[plugins] call_plugin_data: plugin={:?}, symbol={:?}, request={:?}",
        plugin,
        symbol,
        preview(&request.to_string(), 400)
    );
    let response = invoke_plugin(plugin, symbol, request)?;
    Ok(response.get("data").cloned().unwrap_or(Value::Null))
}

/// 取插件响应 `data.content`。
fn content_of_response(value: &Value) -> String {
    value
        .get("data")
        .and_then(|data| data.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// 调用插件并返回完整响应 JSON（含 `code` / `data` / `msg`）。
fn invoke_plugin(plugin: &str, symbol: &str, request: &Value) -> Result<Value> {
    // 重入保护：插件回调 chatllm 时不允许再次进入插件调用。
    let _guard = PluginCallGuard::enter()?;
    println!(
        "[plugins] invoke_plugin: plugin={:?}, symbol={:?}, request={:?}",
        plugin,
        symbol,
        preview(&request.to_string(), 400)
    );
    // 供应商插件统一走 cloud，不再由插件各自 dlopen 供应商库。
    if let Some(supplier) = Supplier::from_exact_code(plugin) {
        println!("[plugins] 供应商插件走 cloud: {:?}", supplier);
        let input = request
            .get("prompt")
            .and_then(Value::as_str)
            .or_else(|| request.get("prompts").and_then(Value::as_str))
            .map(str::to_string)
            .unwrap_or_else(|| request.to_string());
        let reply = match crate::utils::cloud::chat_with(supplier, &input, &[]) {
            Ok(reply) => reply,
            Err(error) => {
                println!("[plugins] cloud 调用失败: {:#}", error);
                return Err(error);
            }
        };
        println!(
            "[plugins] cloud 返回: content={:?}, model={:?}",
            preview(&reply.content, 300),
            reply.model
        );
        return Ok(json!({
            "code": 200,
            "data": { "content": reply.content },
            "msg": "success",
        }));
    }

    let lib = plugin_library(plugin)?;
    let entry = format!("post_{}", symbol);
    let function: libloading::Symbol<'_, FfiFn> = unsafe {
        match lib.get::<FfiFn>(entry.as_bytes()) {
            Ok(function) => function,
            Err(first) if symbol != "chat" => {
                // 兼容旧 scan 记录：ffi_symbol 可能是 mcp tool 名，而插件实际入口是 post_chat。
                println!("[plugins] dlsym {} 失败，回退 post_chat: {}", entry, first);
                lib.get::<FfiFn>(b"post_chat").with_context(|| {
                    format!(
                        "dlsym {} / post_chat 失败（{}）: {}",
                        entry, plugin, first
                    )
                })?
            }
            Err(first) => {
                return Err(anyhow!(
                    "dlsym {} 失败（{}）: {}",
                    entry, plugin, first
                ))
            }
        }
    };

    let request = request.to_string();
    println!("[plugins] 插件请求: {:?}", preview(&request, 400));
    let req = CString::new(request).context("插件请求 JSON 含 \\0")?;
    let mut out = vec![0u8; OUT_BUF_SIZE];
    let ret = unsafe { function(req.as_ptr(), out.as_mut_ptr() as *mut c_char) };
    println!("[plugins] 插件 FFI 返回: {}", ret);
    if ret == -2 {
        bail!("插件 {} 输出超过 64 KiB", plugin);
    }
    if ret != 0 {
        bail!("插件 {} FFI 返回 {}", plugin, ret);
    }

    let out = unsafe { CStr::from_ptr(out.as_ptr() as *const c_char) }
        .to_str()
        .context("插件响应非 UTF-8")?;
    println!("[plugins] 插件原始响应: {:?}", preview(out, 500));
    let value: Value = serde_json::from_str(out)
        .with_context(|| format!("插件 {} 响应解析失败: {}", plugin, out))?;

    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 && code != 200 {
        let msg = value
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        println!("[plugins] 插件错误: code={}, msg={}", code, msg);
        bail!("插件 {} 错误 (code={}): {}", plugin, code, msg);
    }

    println!(
        "[plugins] 插件 data: {:?}",
        preview(
            &value
                .get("data")
                .map(|data| data.to_string())
                .unwrap_or_default(),
            300
        )
    );
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_nested_plugin_call() {
        let outer = PluginCallGuard::enter().expect("首层调用应成功");
        assert!(plugin_call_active());
        assert!(PluginCallGuard::enter().is_err(), "同线程重入应被拒绝");
        drop(outer);
        assert!(!plugin_call_active());
    }

    #[test]
    fn guard_recovers_after_reentry_error() {
        {
            let _outer = PluginCallGuard::enter().expect("首层调用应成功");
            assert!(PluginCallGuard::enter().is_err());
        }
        // 外层释放后应恢复可调用状态。
        let _again = PluginCallGuard::enter().expect("外层释放后应可再次进入");
    }

    #[test]
    fn plugin_call_active_is_thread_local() {
        let _outer = PluginCallGuard::enter().expect("首层调用应成功");
        let active = std::thread::spawn(plugin_call_active).join().unwrap();
        assert!(!active, "重入标记不应影响其他线程");
    }

    #[test]
    fn content_of_response_extracts_string() {
        assert_eq!(
            content_of_response(&json!({ "data": { "content": "hello" } })),
            "hello"
        );
        assert_eq!(
            content_of_response(&json!({ "data": { "content": "  " } })),
            "  "
        );
    }

    #[test]
    fn content_of_response_defaults_empty() {
        assert_eq!(content_of_response(&json!({})), "");
        assert_eq!(content_of_response(&json!({ "data": {} })), "");
        assert_eq!(content_of_response(&json!({ "data": { "content": 1 } })), "");
    }
}
