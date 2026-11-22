// src/utils/plugins_callback.rs
//! 插件方法回调：加载插件动态库并调用 `post_<symbol>`。
//!
//! `think` / `chat_plugins` / `reply` 阶段都通过这里执行插件方法；本模块只负责
//! 插件库缓存、重入保护与 FFI 调用，不涉及检索与阶段流程。

use anyhow::{anyhow, bail, Context, Result};
use libloading::Library;
use serde_json::{json, Value};
use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

/// 插件未指定符号时的默认入口。
pub const DEFAULT_SYMBOL: &str = "chat";
/// 与插件 `write_json_out` 保持一致的输出缓冲。
const OUT_BUF_SIZE: usize = 65536;

#[cfg(windows)]
const DYLIB_EXT: &str = "dll";
#[cfg(target_os = "linux")]
const DYLIB_EXT: &str = "so";
#[cfg(target_os = "macos")]
const DYLIB_EXT: &str = "dylib";

type FfiFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

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
    println!("[plugins_callback] 插件路径: {}", path.display());
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
        println!("[plugins_callback] 插件错误: code={}, msg={}", code, msg);
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
                "[plugins_callback] dlsym {} 失败，回退 post_chat: {}",
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
        "[plugins_callback] invoke: plugin={:?}, symbol={:?}, request={:?}",
        plugin, symbol, body
    );

    let lib = plugin_library(plugin)?;
    let function = plugin_entry(&lib, plugin, symbol)?;

    let req = CString::new(body).context("插件请求 JSON 含 \\0")?;
    let mut out = vec![0u8; OUT_BUF_SIZE];
    // SAFETY: 请求与输出缓冲区生命周期覆盖整个调用。
    let ret = unsafe { function(req.as_ptr(), out.as_mut_ptr() as *mut c_char) };
    println!("[plugins_callback] 插件 FFI 返回: {}", ret);
    check_plugin_ret(plugin, ret)?;

    let out = unsafe { CStr::from_ptr(out.as_ptr() as *const c_char) }
        .to_str()
        .context("插件响应非 UTF-8")?;
    println!("[plugins_callback] 插件原始响应: {:?}", out);
    parse_plugin_response(plugin, out)
}
