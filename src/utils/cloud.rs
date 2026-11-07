// src/utils/cloud.rs
//! 云端模型调用客户端。
//!
//! 供应商选择、DeepSeek / SiliconFlow 的 HTTP 调用都已迁移到独立的
//! `cloud_lib` 系统库（`./libary/cloud_lib`）。chatllm 只负责：
//! - 读取 `info.db.setting` 做对话前置检查；
//! - 把 `supplier + prompt + messages` 交给 `cloud_lib` 的 `post_chat`。
//!
//! 这样 chatllm 不再直接加载供应商库，避免插件回调与 chatllm 互相加载。
//!
//! 协议：
//! 请求 `{"supplier": "...", "prompt": "...", "messages": [...]}`，
//! 响应 `{"code":0,"data":{"content":"...","id":"...","model":"...","usage":{...}}}`。

use crate::utils::db::db_query;
use anyhow::{bail, Context, Result};
use libloading::Library;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

/// `info.db` 的逻辑库名。
const INFO_DB: &str = "info";
/// 云端系统库输出缓冲区，与 `post_chat` 约定一致。
const OUT_BUF_SIZE: usize = 65536;
/// 未配置 `ai_supplier` 时使用的供应商。
const DEFAULT_SUPPLIER: &str = "deepseek";
/// 云端系统库调用符号。
const ENTRY_SYMBOL: &[u8] = b"post_chat";
/// 云端系统库名（位于 `./libary/`）。
const CLOUD_LIB_NAME: &str = "cloud_lib";
#[cfg(windows)]
const DYLIB_EXT: &str = "dll";
#[cfg(target_os = "linux")]
const DYLIB_EXT: &str = "so";
#[cfg(target_os = "macos")]
const DYLIB_EXT: &str = "dylib";

type FfiFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

/// 云端供应商。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Supplier {
    Deepseek,
    SiliconFlow,
}

impl Supplier {
    /// 精确解析供应商代码；空值、未知值返回 `None`。
    ///
    /// 兼容插件名带 `lib` 前缀的情况（`libdeepseek` / `libsiliconflow_t2t`）。
    pub fn from_exact_code(code: &str) -> Option<Self> {
        let code = code.trim();
        let code = code.strip_prefix("lib").unwrap_or(code);
        match code {
            "deepseek" => Some(Supplier::Deepseek),
            "siliconflow_t2t" => Some(Supplier::SiliconFlow),
            _ => None,
        }
    }

    /// `ai_supplier` 为空或未知时回退 `deepseek`。
    pub fn from_code(code: Option<&str>) -> Self {
        let code = code
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_SUPPLIER);
        Self::from_exact_code(code).unwrap_or(Supplier::Deepseek)
    }

    pub fn code(self) -> &'static str {
        match self {
            Supplier::Deepseek => "deepseek",
            Supplier::SiliconFlow => "siliconflow_t2t",
        }
    }

    /// 两家供应商都已合并到同一个 `cloud_lib`。
    pub fn lib_name(self) -> &'static str {
        CLOUD_LIB_NAME
    }

    /// `info.db.setting` 中保存 token 的 key。
    pub fn token_key(self) -> &'static str {
        match self {
            Supplier::Deepseek => "deepseek_app_key",
            Supplier::SiliconFlow => "silicon_app_key",
        }
    }

    /// token 缺失时的错误文案。
    pub fn missing_token_message(self) -> String {
        format!(
            "AI 配置错误：未配置 {} token：{}",
            self.code(),
            self.token_key()
        )
    }
}

/// 云端返回。
#[derive(Debug, Clone)]
pub struct CloudReply {
    pub content: String,
    pub model: Option<String>,
    pub session_id: Option<String>,
    pub usage: Value,
}

/// 云端系统库候选路径：只查 `./libary/`。
pub fn library_candidates(lib_name: &str) -> Vec<PathBuf> {
    let base = lib_name.strip_prefix("lib").unwrap_or(lib_name);
    vec![
        PathBuf::from(format!("./libary/{}.{}", base, DYLIB_EXT)),
        PathBuf::from(format!("./libary/lib{}.{}", base, DYLIB_EXT)),
    ]
}

/// 找到云端系统库的动态库路径。
pub fn resolve_library_path(lib_name: &str) -> Option<PathBuf> {
    let path = library_candidates(lib_name)
        .into_iter()
        .find(|path| path.exists());
    println!("[cloud] resolve_library_path: {:?} -> {:?}", lib_name, path);
    path
}

/// 读取 `info.db.setting` 中的配置项；不存在时返回 `None`。
pub fn setting(key: &str) -> Result<Option<String>> {
    let key = key.replace('\'', "''");
    let rows = db_query(
        INFO_DB,
        &format!("SELECT value FROM setting WHERE key = '{}' LIMIT 1", key),
    )?;
    let value = rows
        .first()
        .and_then(|row| row.get("value"))
        .and_then(Value::as_str)
        .map(str::to_string);
    println!(
        "[cloud] setting: key={:?}, found={}, len={}",
        key,
        value.is_some(),
        value.as_deref().map(str::len).unwrap_or(0)
    );
    Ok(value)
}

/// 当前配置的供应商（`ai_supplier`）。
pub fn current_supplier() -> Result<Supplier> {
    let code = setting("ai_supplier")?;
    let supplier = Supplier::from_code(code.as_deref());
    println!("[cloud] current_supplier: {:?}", supplier);
    Ok(supplier)
}

/// 读取供应商 token，缺失时返回配置错误。
pub fn token(supplier: Supplier) -> Result<String> {
    let value = setting(supplier.token_key())?.unwrap_or_default();
    let value = value.trim();
    if value.is_empty() {
        println!("[cloud] token: supplier={:?}, missing", supplier);
        bail!("{}", supplier.missing_token_message());
    }
    println!(
        "[cloud] token: supplier={:?}, key={}, len={}",
        supplier,
        supplier.token_key(),
        value.len()
    );
    Ok(value.to_string())
}

/// 已加载的云端系统库缓存，避免每次调用重复 `dlopen`。
fn library(lib_name: &str) -> Result<Arc<Library>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Library>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache
        .lock()
        .map_err(|_| anyhow::anyhow!("系统库缓存已损坏"))?;

    if let Some(lib) = cache.get(lib_name) {
        return Ok(Arc::clone(lib));
    }

    let path = resolve_library_path(lib_name)
        .with_context(|| format!("找不到云端系统库 {}（应放在 ./libary/）", lib_name))?;
    let lib = unsafe { Library::new(&path) }
        .with_context(|| format!("加载云端系统库 {} 失败", path.display()))?;
    let lib = Arc::new(lib);
    cache.insert(lib_name.to_string(), Arc::clone(&lib));
    Ok(lib)
}

/// 调用 `cloud_lib` 的 `post_chat`。
pub fn chat_with(supplier: Supplier, prompt: &str, messages: &[Value]) -> Result<CloudReply> {
    println!(
        "[cloud] chat_with: supplier={:?}, prompt={:?}, messages={}",
        supplier,
        preview(prompt, 200),
        messages.len()
    );
    let lib = library(supplier.lib_name())?;
    let function: libloading::Symbol<'_, FfiFn> = unsafe {
        lib.get(ENTRY_SYMBOL).with_context(|| {
            format!(
                "云端系统库 {} 缺少符号 {}",
                supplier.lib_name(),
                String::from_utf8_lossy(ENTRY_SYMBOL)
            )
        })?
    };

    // 云端库要求 messages 非空；没有历史时至少携带当前 user 消息。
    let request_messages = if messages.is_empty() {
        vec![json!({ "role": "user", "content": prompt })]
    } else {
        messages.to_vec()
    };
    let request = json!({
        "supplier": supplier.code(),
        "prompt": prompt,
        "messages": request_messages,
    });
    let request = CString::new(request.to_string()).context("云端请求 JSON 含 \\0")?;
    let mut out = vec![0u8; OUT_BUF_SIZE];
    let ret = unsafe { function(request.as_ptr(), out.as_mut_ptr() as *mut c_char) };
    if ret == -1 {
        bail!("云端系统库 {} 输出指针为空", supplier.lib_name());
    }
    if ret == -2 {
        bail!("云端响应超过 64 KiB");
    }

    let out = unsafe { CStr::from_ptr(out.as_ptr() as *const c_char) }
        .to_str()
        .context("云端响应非 UTF-8")?;
    println!("[cloud] chat_with 原始响应: {:?}", preview(out, 500));
    let value: Value =
        serde_json::from_str(out).with_context(|| format!("云端响应解析失败: {}", out))?;

    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    println!("[cloud] chat_with 响应 code: {}", code);
    if code != 0 && code != 200 {
        let msg = value
            .get("msg")
            .and_then(Value::as_str)
            .filter(|msg| !msg.trim().is_empty())
            .unwrap_or("云端调用失败");
        bail!("{}", msg);
    }

    let data = value.get("data").cloned().unwrap_or(Value::Null);
    let data_obj = data.as_object();
    let content = data
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    println!("[cloud] chat_with content: {:?}", preview(&content, 300));
    Ok(CloudReply {
        content,
        model: data_obj
            .and_then(|obj| obj.get("model"))
            .and_then(Value::as_str)
            .map(str::to_string),
        session_id: data_obj
            .and_then(|obj| obj.get("session_id"))
            .and_then(Value::as_str)
            .map(str::to_string),
        usage: data_obj
            .and_then(|obj| obj.get("usage"))
            .cloned()
            .unwrap_or(Value::Null),
    })
}

/// 读取 `ai_supplier` 与 token，再调用 cloud。
pub fn chat(prompt: &str, messages: &[Value]) -> Result<CloudReply> {
    println!(
        "[cloud] chat: prompt={:?}, messages={}",
        preview(prompt, 200),
        messages.len()
    );
    let supplier = current_supplier()?;
    token(supplier)?;
    chat_with(supplier, prompt, messages)
}

/// 把消息对转成 `[{role, content}]`，空内容会被过滤。
pub fn messages_from(pairs: &[(&str, &str)]) -> Vec<Value> {
    let messages: Vec<Value> = pairs
        .iter()
        .filter(|(_, content)| !content.trim().is_empty())
        .map(|(role, content)| json!({ "role": role, "content": content }))
        .collect();
    println!("[cloud] messages_from: {} 条", messages.len());
    messages
}

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
