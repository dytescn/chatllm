// src/utils/plugin.rs
//! 设计插件 / 聊天插件调用封装。
//!
//! 通过 `dlopen` 直接加载插件动态库并调用 `post_<ffi_symbol>`，不经过 cyctron
//! 的 HTTP 层。插件路径按以下顺序探测：
//!
//! 1. `./plugins/<lib_name>/<lib_name>.dll`
//! 2. `./plugins/<lib_name>/lib<lib_name>.dll`
//! 3. `./libary/<lib_name>.dll`
//! 4. `./libary/lib<lib_name>.dll`

use anyhow::{bail, Context, Result};
use libloading::Library;
use serde_json::Value;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::PathBuf;

#[cfg(windows)]
const DYLIB_EXT: &str = "dll";
#[cfg(target_os = "linux")]
const DYLIB_EXT: &str = "so";
#[cfg(target_os = "macos")]
const DYLIB_EXT: &str = "dylib";

/// 与插件 `write_json_out` 保持一致的输出缓冲。
const OUT_BUF_SIZE: usize = 65536;

type FfiFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

fn candidates(lib_name: &str) -> Vec<PathBuf> {
    vec![
        PathBuf::from(format!("./plugins/{0}/{0}.{1}", lib_name, DYLIB_EXT)),
        PathBuf::from(format!("./plugins/{0}/lib{0}.{1}", lib_name, DYLIB_EXT)),
        PathBuf::from(format!("./libary/{0}.{1}", lib_name, DYLIB_EXT)),
        PathBuf::from(format!("./libary/lib{0}.{1}", lib_name, DYLIB_EXT)),
    ]
}

/// 按约定顺序找到插件动态库。
pub fn resolve_path(lib_name: &str) -> Option<PathBuf> {
    candidates(lib_name).into_iter().find(|path| path.exists())
}

/// 调用插件的 `post_<ffi_symbol>`，返回响应中的 `data`。
pub fn call(lib_name: &str, ffi_symbol: &str, request: &Value) -> Result<Value> {
    let path = resolve_path(lib_name).with_context(|| {
        format!(
            "找不到插件 {} 的动态库（已尝试 ./plugins/{} 与 ./libary/）",
            lib_name, lib_name
        )
    })?;

    let lib = unsafe { Library::new(&path) }
        .with_context(|| format!("加载插件 {} 失败", path.display()))?;
    let symbol = format!("post_{}", ffi_symbol);
    let function: libloading::Symbol<'_, FfiFn> = unsafe {
        lib.get(symbol.as_bytes())
            .with_context(|| format!("dlsym {} 失败（{}）", symbol, path.display()))?
    };

    let req = CString::new(request.to_string()).context("插件请求 JSON 含 \\0")?;
    let mut out_buf = vec![0u8; OUT_BUF_SIZE];
    let ret = unsafe { function(req.as_ptr(), out_buf.as_mut_ptr() as *mut c_char) };
    if ret == -2 {
        bail!("插件 {} 输出超过 64 KiB", lib_name);
    }
    if ret != 0 {
        bail!("插件 {} FFI 返回 {}", lib_name, ret);
    }

    let out = unsafe { CStr::from_ptr(out_buf.as_ptr() as *const c_char) }
        .to_str()
        .context("插件响应非 UTF-8")?;
    let value: Value = serde_json::from_str(out)
        .with_context(|| format!("插件 {} 响应解析失败: {}", lib_name, out))?;

    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 && code != 200 {
        let msg = value
            .get("msg")
            .and_then(Value::as_str)
            .filter(|msg| !msg.trim().is_empty())
            .unwrap_or("插件返回错误");
        bail!("{}", msg);
    }

    Ok(value.get("data").cloned().unwrap_or(Value::Null))
}

#[cfg(test)]
mod tests {
    use super::resolve_path;

    #[test]
    fn unknown_plugin_has_no_path() {
        assert!(resolve_path("definitely_missing_plugin_xyz").is_none());
    }
}
