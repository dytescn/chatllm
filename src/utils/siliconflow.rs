// src/utils/siliconflow.rs
use anyhow::{Context, Result};
use libloading::Library;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::OnceLock;

/// siliconflow 动态库路径（相对 cwd = /home/funx/app/piksel）
#[cfg(windows)]
const SO_PATH: &str = "./libary/libsiliconflow_t2t.dll";
#[cfg(not(windows))]
const SO_PATH: &str = "./libary/libsiliconflow_t2t.so";

const OUT_BUF_SIZE: usize = 65536;

type PostChatFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

static CHAT_LIB: OnceLock<Library> = OnceLock::new();
static CHAT_FN: OnceLock<PostChatFn> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatResult {
    pub content: String,
    /// 供应商响应 id；chatllm 写入 `dialogue.session_id`
    pub id: Option<String>,
}

fn load_chat_fn() -> Result<PostChatFn> {
    let lib = unsafe { Library::new(SO_PATH) }.with_context(|| format!("加载 {} 失败", SO_PATH))?;
    let f = unsafe {
        *lib.get::<PostChatFn>(b"post_chat")
            .context("dlsym post_chat 失败")?
    };
    let _ = CHAT_LIB.set(lib);
    eprintln!("[siliconflow] 加载 {} 成功", SO_PATH);
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

fn call(req: Value) -> Result<Value> {
    let f = chat_fn()?;
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

    Ok(value)
}

/// 发送完整 messages（system + 历史 + 当前 user）。
pub fn chat_messages(messages: Vec<ChatMessage>, session_id: Option<&str>) -> Result<ChatResult> {
    if messages.is_empty() {
        anyhow::bail!("messages 不能为空");
    }

    let mut req = serde_json::json!({ "messages": messages });
    if let Some(session_id) = session_id.filter(|s| !s.is_empty()) {
        req["session_id"] = serde_json::json!(session_id);
    }

    let value = call(req)?;
    let data = value.get("data").cloned().unwrap_or(Value::Null);
    let content = data
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if content.is_empty() {
        anyhow::bail!("siliconflow 返回 content 为空");
    }

    let id = data.get("id").and_then(Value::as_str).map(str::to_string);

    Ok(ChatResult { content, id })
}

/// 兼容旧调用：单轮 prompt，返回 content。
pub fn chat_single(prompt: &str) -> Result<String> {
    let result = chat_messages(vec![ChatMessage::user(prompt)], None)?;
    Ok(result.content)
}
