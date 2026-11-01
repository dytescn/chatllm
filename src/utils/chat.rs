// src/utils/chat.rs
//! 最简对话流程：
//! 收到消息 → 查询 kv 的 `chat` 分类 → 取命中记录
//! → 记录里带 plugin 就执行插件，否则直接返回对话内容。
//!
//! 调用方式全部走本地动态库：
//! - kv：`utils::kv` → `./libary/kv_lib.dll`
//! - 插件：`designplugin_lib` 安装到 `./plugins/<name>/`，这里按该约定 dlopen。

use anyhow::{bail, Context, Result};
use libloading::Library;
use serde_json::{json, Value};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::PathBuf;

use crate::utils::kv;

/// 消息命中数量。
const TOP_K: i64 = 5;
/// 插件未指定符号时的默认入口。
const DEFAULT_SYMBOL: &str = "chat";
/// 与插件 `write_json_out` 保持一致的输出缓冲。
const OUT_BUF_SIZE: usize = 65536;

#[cfg(windows)]
const DYLIB_EXT: &str = "dll";
#[cfg(not(windows))]
const DYLIB_EXT: &str = "so";

type FfiFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

/// 处理一条消息，返回 assistant 回复。
pub fn handle(input: &str) -> Result<String> {
    let data = kv::search_chat(input, TOP_K)?;
    let list = data
        .get("list")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let Some(hit) = list.first() else {
        return Ok(format!("未找到相关对话内容：{input}"));
    };

    if let Some((plugin, symbol)) = plugin_of(hit) {
        return call_plugin(&plugin, &symbol, input);
    }

    Ok(content_of(hit).unwrap_or_else(|| format!("未找到相关对话内容：{input}")))
}

/// 从命中记录里解析插件：`meta.plugin`，或 `value` 里的 `lib_name`。
fn plugin_of(hit: &Value) -> Option<(String, String)> {
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

fn content_of(hit: &Value) -> Option<String> {
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

/// 执行插件：dlopen 后调用 `post_<symbol>`，返回响应 `data.content`。
fn call_plugin(plugin: &str, symbol: &str, input: &str) -> Result<String> {
    let path = candidates(plugin)
        .into_iter()
        .find(|path| path.exists())
        .with_context(|| {
            format!("找不到插件 {plugin}（./plugins/{plugin}/ 或 ./libary/）")
        })?;

    let lib = unsafe { Library::new(&path) }
        .with_context(|| format!("加载插件 {} 失败", path.display()))?;
    let entry = format!("post_{}", symbol);
    let function: libloading::Symbol<'_, FfiFn> = unsafe {
        lib.get(entry.as_bytes())
            .with_context(|| format!("dlsym {} 失败（{}）", entry, path.display()))?
    };

    let request = json!({ "prompt": input }).to_string();
    let req = CString::new(request).context("插件请求 JSON 含 \\0")?;
    let mut out = vec![0u8; OUT_BUF_SIZE];
    let ret = unsafe { function(req.as_ptr(), out.as_mut_ptr() as *mut c_char) };
    if ret == -2 {
        bail!("插件 {} 输出超过 64 KiB", plugin);
    }
    if ret != 0 {
        bail!("插件 {} FFI 返回 {}", plugin, ret);
    }

    let out = unsafe { CStr::from_ptr(out.as_ptr() as *const c_char) }
        .to_str()
        .context("插件响应非 UTF-8")?;
    let value: Value = serde_json::from_str(out)
        .with_context(|| format!("插件 {} 响应解析失败: {}", plugin, out))?;

    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 && code != 200 {
        let msg = value
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        bail!("插件 {} 错误 (code={}): {}", plugin, code, msg);
    }

    Ok(value
        .get("data")
        .and_then(|data| data.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string())
}
