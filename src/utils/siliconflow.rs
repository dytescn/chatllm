// src/utils/siliconflow.rs
use anyhow::{Context, Result};
use serde_json::Value;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::OnceLock;

/// siliconflow 动态库路径（相对 cwd = /home/funx/app/piksel）
const SO_PATH: &str = "./libary/libsiliconflow_t2t.so";

const OUT_BUF_SIZE: usize = 65536;

type PostChatFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

static CHAT_FN: OnceLock<PostChatFn> = OnceLock::new();

fn load_chat_fn() -> Result<PostChatFn> {
    let sym_name = CString::new("post_chat").unwrap();
    let path = CString::new(SO_PATH).context("构造 .so 路径失败")?;

    // 关键：只 dlopen 指定路径，RTLD_LOCAL 防止符号外溢，绝不走 RTLD_DEFAULT
    let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if handle.is_null() {
        let err = unsafe {
            let e = libc::dlerror();
            if e.is_null() {
                "unknown dlopen error".to_string()
            } else {
                CStr::from_ptr(e).to_string_lossy().into_owned()
            }
        };
        anyhow::bail!("dlopen {} 失败: {}", SO_PATH, err);
    }

    let ptr = unsafe { libc::dlsym(handle, sym_name.as_ptr()) };
    if ptr.is_null() {
        anyhow::bail!("dlsym post_chat 失败");
    }

    let f: PostChatFn = unsafe { std::mem::transmute(ptr) };
    eprintln!("[siliconflow] 显式 dlopen {} 成功", SO_PATH);
    Ok(f)
}

fn chat_fn() -> Result<PostChatFn> {
    if let Some(f) = CHAT_FN.get() {
        return Ok(*f);
    }
    let f = load_chat_fn()?;
    let _ = CHAT_FN.set(f);
    Ok(f)
}

pub fn chat_single(prompt: &str) -> Result<String> {
    let f = chat_fn()?;

    let req = serde_json::json!({ "prompt": prompt });
    let req_c = CString::new(req.to_string()).context("请求 JSON 含 \\0")?;

    let mut out_buf = vec![0u8; OUT_BUF_SIZE];
    let ret = unsafe { f(req_c.as_ptr(), out_buf.as_mut_ptr() as *mut c_char) };

    let out_str = unsafe { CStr::from_ptr(out_buf.as_ptr() as *const c_char) }
        .to_str()
        .context("siliconflow 响应非 UTF-8")?;

    if out_str.is_empty() {
        anyhow::bail!("siliconflow 返回空（ret={}）", ret);
    }

    let value: Value = serde_json::from_str(out_str)
        .with_context(|| format!("siliconflow 响应解析失败: {}", out_str))?;

    let code = value.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code != 200 && code != 0 {
        let msg = value
            .get("msg")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown");
        anyhow::bail!("siliconflow 错误 (code={}): {}", code, msg);
    }

    let content = value
        .get("data")
        .and_then(|d| d.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    if content.is_empty() {
        anyhow::bail!("siliconflow 返回 content 为空");
    }
    Ok(content)
}
