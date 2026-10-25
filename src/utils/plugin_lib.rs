//! plugin_lib 的 dlopen 调用入口（与 database/markparse 一致）。
//!
//! 直接调用完整符号：`get_functions` / `patch_function`。

use anyhow::{Context, Result};
use libloading::{Library, Symbol};
use serde_json::Value;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::OnceLock;

#[cfg(windows)]
const LIB_PATH: &str = "./libary/plugin_lib.dll";
#[cfg(not(windows))]
const LIB_PATH: &str = "./libary/plugin_lib.so";

const OUT_BUF_SIZE: usize = 65536;

type FfiFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

static LIB: OnceLock<std::result::Result<Library, String>> = OnceLock::new();

fn library() -> Result<&'static Library> {
    let cell = LIB.get_or_init(|| {
        // SAFETY: plugin_lib 由本应用安装，加载与符号调用在同一进程内完成。
        unsafe { Library::new(LIB_PATH) }.map_err(|error| format!("加载 {LIB_PATH} 失败: {error}"))
    });

    match cell {
        Ok(library) => Ok(library),
        Err(error) => anyhow::bail!("{error}"),
    }
}

fn call(symbol: &str, request: Value) -> Result<Value> {
    let library = library()?;
    // SAFETY: 符号类型与 plugin_lib 的 C ABI 一致。
    let function: Symbol<'_, FfiFn> = unsafe { library.get(symbol.as_bytes()) }
        .with_context(|| format!("dlsym {symbol} 失败"))?;

    let request = CString::new(request.to_string()).context("请求 JSON 含 NUL")?;
    let mut output = vec![0u8; OUT_BUF_SIZE];

    let ret = unsafe { function(request.as_ptr(), output.as_mut_ptr() as *mut c_char) };
    if ret != 0 {
        anyhow::bail!("plugin_lib {symbol} 返回 {ret}（缓冲区不足或指针为空）");
    }

    let text = unsafe { CStr::from_ptr(output.as_ptr() as *const c_char) }
        .to_str()
        .context("plugin_lib 响应非 UTF-8")?;
    let value: Value =
        serde_json::from_str(text).with_context(|| format!("plugin_lib {symbol} 响应解析失败: {text}"))?;

    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 200 {
        let message = value
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        anyhow::bail!("plugin_lib {symbol} 失败 ({code}): {message}");
    }

    Ok(value.get("data").cloned().unwrap_or(Value::Null))
}

pub fn get_functions(request: Value) -> Result<Value> {
    call("get_functions", request)
}

pub fn patch_function(request: Value) -> Result<Value> {
    call("patch_function", request)
}
