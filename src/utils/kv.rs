// src/utils/kv.rs
//! kv_lib 调用封装。
//!
//! kv_lib 以 `cdylib` 形式导出 C ABI `kv_*` 符号，chatllm 通过 `dlopen`
//! 直接调用，不经过 cyctron 的 HTTP 层。导出表与接口说明见
//! `docs/kv_lib.md`。
//!
//! 只封装当前流程需要的接口：集合、检索与统计；其余接口可按
//! [`call_ffi`] 的方式扩展。

use anyhow::{anyhow, bail, Context, Result};
use libloading::Library;
use serde_json::{json, Value};
use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::{c_char, c_int};
use std::sync::OnceLock;

/// kv_lib 在 libary/ 下的文件名（由 kv_lib/dytes.json 的 exports 决定）。
#[cfg(windows)]
const KV_LIB_PATH: &str = "./libary/kv_lib.dll";
#[cfg(target_os = "linux")]
const KV_LIB_PATH: &str = "./libary/kv_lib.so";
#[cfg(target_os = "macos")]
const KV_LIB_PATH: &str = "./libary/kv_lib.dylib";

/// kv_lib 0.1.5 的导出符号前缀：`kv_collection` / `kv_embed` / `kv_search`。
///
/// 直接 `dlopen` 时按导出表精确匹配，0.1.5 起不再带 `post_` 前缀。
pub const FFI_PREFIX: &str = "kv_";

/// `kv_collection` / `kv_embed` 的输出缓冲，与 kv_lib 保持一致。
const OUT_BUF_SIZE: usize = 65536;

/// `kv_search` 的输出缓冲：kv_lib 要求至少 1 MiB，否则返回 `-2`。
const SEARCH_OUT_BUF_SIZE: usize = 1024 * 1024;

/// `kv_search` 的 `top_k` 取值范围。
const TOP_K_MIN: i64 = 1;
const TOP_K_MAX: i64 = 100;

/// 集合不存在（`kv_collection` stats / `kv_search`）。
pub const CODE_COLLECTION_NOT_FOUND: i64 = 400600;

/// `chat` 集合：对话内容检索。
pub const COLLECTION_CHAT: &str = "chat";
/// 插件能力集合的固定名称。
pub const COLLECTION_PLUGIN: &str = "plugin";
/// 提示词集合的固定名称。
pub const COLLECTION_PROMPT: &str = "prompt";
/// 聊天插件集合（chat 分类插件）。
pub const COLLECTION_CHAT_PLUGINS: &str = "chat_plugins";

/// 默认相关度阈值；低于该分数的检索结果不视为命中。
pub const DEFAULT_MATCH_MIN_SCORE: f64 = 0.35;

type FfiFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

static KV_LIB: OnceLock<Library> = OnceLock::new();

/// kv_lib 返回的业务错误，保留 `code` 便于调用方区分“集合不存在”等情况。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvError {
    pub symbol: String,
    pub code: i64,
    pub msg: String,
}

impl fmt::Display for KvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "kv_lib {} 错误 (code={}): {}",
            self.symbol, self.code, self.msg
        )
    }
}

impl std::error::Error for KvError {}

/// 由逻辑名构造实际导出符号：`search` → `kv_search`。
pub fn ffi_symbol(symbol: &str) -> String {
    format!("{FFI_PREFIX}{symbol}")
}

/// 判断 error 是否为 kv_lib 的指定错误码。
pub fn is_error_code(error: &anyhow::Error, code: i64) -> bool {
    error
        .downcast_ref::<KvError>()
        .is_some_and(|error| error.code == code)
}

fn ensure_library() -> Result<&'static Library> {
    if let Some(lib) = KV_LIB.get() {
        return Ok(lib);
    }

    // SAFETY: 只加载进程内的 kv_lib，符号按导出的 C ABI 签名调用。
    let lib = unsafe { Library::new(KV_LIB_PATH) }
        .with_context(|| format!("加载 {} 失败", KV_LIB_PATH))?;
    // 并发初始化时另一个线程可能先 set 成功；此时用已注册的实例。
    if KV_LIB.set(lib).is_ok() {
        println!("[kv_lib] 加载 {} 成功", KV_LIB_PATH);
    }
    KV_LIB.get().context("kv_lib 库未初始化")
}

/// 检查 FFI 返回值：`0` 已写入，`-1` 输出指针为空，`-2` 缓冲区不足。
pub fn check_ret(symbol: &str, ret: c_int, buf_size: usize) -> Result<()> {
    match ret {
        0 => Ok(()),
        -1 => bail!("kv_lib {} json_out 为空指针", symbol),
        -2 => bail!("kv_lib {} 输出超过 {} KiB", symbol, buf_size / 1024),
        other => bail!("kv_lib {} FFI 返回 {}", symbol, other),
    }
}

/// 解析 kv_lib 的 JSON 响应并取出 `data`。
///
/// 纯函数，便于单测。业务错误（`code != 200`）转为 [`KvError`]。
pub fn parse_response(symbol: &str, out: &str) -> Result<Value> {
    if out.trim().is_empty() {
        println!("[kv] {} 返回空", symbol);
        bail!("kv_lib {} 返回空", symbol);
    }

    let value: Value = serde_json::from_str(out)
        .with_context(|| format!("kv_lib {} 响应解析失败: {}", symbol, out))?;

    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    println!("[kv] {} 响应 code={}", symbol, code);
    if code != 200 {
        let msg = value
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        return Err(anyhow!(KvError {
            symbol: symbol.to_string(),
            code,
            msg,
        }));
    }

    Ok(value.get("data").cloned().unwrap_or(Value::Null))
}

/// 取出 `data.list` 数组的拷贝；缺失或类型不符时返回空列表。
///
/// 需要避免拷贝时用 [`take_list`]。
pub fn extract_list(data: &Value) -> Vec<Value> {
    data.get("list")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// 从响应 `data` 中取走 `list` 数组，失败时返回空列表。
///
/// 与 [`extract_list`] 不同，这里移动数组本身，适合检索结果这类可能较大的场景。
pub fn take_list(data: Value) -> Vec<Value> {
    match data {
        Value::Object(mut map) => match map.remove("list") {
            Some(Value::Array(list)) => list,
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// 调用任意 `kv_<symbol>` FFI，返回响应中的 `data`。
pub fn call_ffi(symbol: &str, request: &Value) -> Result<Value> {
    call_ffi_with_size(symbol, request, OUT_BUF_SIZE)
}

/// 按指定输出缓冲大小调用 FFI。
fn call_ffi_with_size(symbol: &str, request: &Value, buf_size: usize) -> Result<Value> {
    let name = ffi_symbol(symbol);
    let body = request.to_string();
    println!(
        "[kv] call_ffi: symbol={}, request={:?}",
        name,
        preview(&body, 300)
    );

    let lib = ensure_library()?;
    // SAFETY: kv_lib 的导出函数都是 `extern "C" fn(*const c_char, *mut c_char) -> c_int`。
    let function: libloading::Symbol<'_, FfiFn> = unsafe {
        lib.get(name.as_bytes())
            .with_context(|| format!("kv_lib 缺少导出符号 {}（版本不匹配？）", name))?
    };

    let req = CString::new(body).context("请求 JSON 含 \\0")?;
    let mut out_buf = vec![0u8; buf_size];
    let ret = unsafe { function(req.as_ptr(), out_buf.as_mut_ptr() as *mut c_char) };
    check_ret(&name, ret, buf_size)?;

    let out = unsafe { CStr::from_ptr(out_buf.as_ptr() as *const c_char) }
        .to_str()
        .context("kv_lib 响应非 UTF-8")?;
    parse_response(symbol, out)
}

/// `kv_collection`（action=list）：集合列表。
pub fn collection_list(page: Option<i64>, page_size: Option<i64>) -> Result<Value> {
    call_ffi(
        "collection",
        &json!({ "action": "list", "page": page, "page_size": page_size }),
    )
}

/// `kv_collection`（action=stats）：集合 / 分片 / 索引统计；`collection` 为空时返回全部。
pub fn stats(collection: Option<&str>) -> Result<Value> {
    call_ffi(
        "collection",
        &json!({ "action": "stats", "collection": collection }),
    )
}

/// 把 `top_k` 夹到 kv_lib 支持的 `1..=100`。
pub fn clamp_top_k(top_k: i64) -> i64 {
    top_k.clamp(TOP_K_MIN, TOP_K_MAX)
}

/// 构造 `kv_search` 请求；`top_k` 会先经 [`clamp_top_k`] 夹取。
pub fn build_search_request(
    collection: &str,
    query: &str,
    top_k: i64,
    min_score: f64,
    candidate_keys: Option<&[String]>,
    with_value: bool,
) -> Value {
    json!({
        "collection": collection,
        "query": query,
        "top_k": clamp_top_k(top_k),
        "min_score": min_score,
        "candidate_keys": candidate_keys,
        "with_value": with_value
    })
}

/// `kv_search`：按文本相似度检索。
///
/// `top_k` 会夹到 kv_lib 支持的 `1..=100`；`candidate_keys` 用于先由业务
/// 元数据缩小范围，再点查向量。
#[allow(clippy::too_many_arguments)]
pub fn search(
    collection: &str,
    query: &str,
    top_k: i64,
    min_score: f64,
    candidate_keys: Option<&[String]>,
    with_value: bool,
) -> Result<Value> {
    let request = build_search_request(
        collection,
        query,
        top_k,
        min_score,
        candidate_keys,
        with_value,
    );
    call_ffi_with_size("search", &request, SEARCH_OUT_BUF_SIZE)
}

/// 在指定集合中检索并直接返回候选列表。
fn search_hits(collection: &str, query: &str, top_k: i64) -> Result<Vec<Value>> {
    let data = search(collection, query, top_k, match_min_score(), None, true)?;
    Ok(take_list(data))
}

/// 在 `chat` 集合中检索对话内容，返回原始 `data`（含 `list`）。
pub fn search_chat(query: &str, top_k: i64) -> Result<Value> {
    search(COLLECTION_CHAT, query, top_k, 0.0, None, true)
}

/// 在 `plugin` 集合中检索插件能力，返回候选列表。
///
/// 每条候选至少包含 `key` / `score`；`with_value=true` 时还带出
/// `value`（插件元数据 JSON）与 `meta`。
pub fn search_plugins(query: &str, top_k: i64) -> Result<Vec<Value>> {
    search_hits(COLLECTION_PLUGIN, query, top_k)
}

/// 在 `chat_plugins` 集合中检索聊天插件，返回候选列表。
///
/// 集合不存在时返回空列表，由插件安装流程负责建库和写入记录。
pub fn search_chat_plugins(query: &str, top_k: i64) -> Result<Vec<Value>> {
    println!(
        "[kv] search_chat_plugins: query={:?}, top_k={}",
        preview(query, 200),
        top_k
    );
    match search_hits(COLLECTION_CHAT_PLUGINS, query, top_k) {
        Ok(list) => {
            println!("[kv] search_chat_plugins 命中 {} 条", list.len());
            Ok(list)
        }
        Err(error) if is_error_code(&error, CODE_COLLECTION_NOT_FOUND) => {
            println!("[kv] chat_plugins 集合不存在，返回空结果");
            Ok(Vec::new())
        }
        Err(error) => Err(error),
    }
}

/// 按字符截断字符串，超长时追加省略号。
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

/// think / plugin / reply 检索使用的相关度阈值。
pub fn match_min_score() -> f64 {
    DEFAULT_MATCH_MIN_SCORE
}
