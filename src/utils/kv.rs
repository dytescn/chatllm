// src/utils/kv.rs
//! kv_lib 调用封装。
//!
//! kv_lib 以 `cdylib` 形式提供 `post_kv_*` 符号，chatllm 通过 `dlopen`
//! 直接调用，不经过 cyctron 的 HTTP 层。HTTP 调用方式见 `docs/kv_lib.md`。
//!
//! 只封装当前流程需要的接口：集合、读写、检索与统计；其余接口可按
//! `call_ffi` 的方式扩展。

use anyhow::{bail, Context, Result};
use libloading::Library;
use serde_json::{json, Value};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::OnceLock;

/// kv_lib 在 libary/ 下的文件名（由 kv_lib/dytes.json 的 exports 决定）。
#[cfg(windows)]
const KV_LIB_PATH: &str = "./libary/kv_lib.dll";
#[cfg(not(windows))]
const KV_LIB_PATH: &str = "./libary/kv_lib.so";

/// 与 kv_lib 的 `write_json_out` 保持一致。
const OUT_BUF_SIZE: usize = 65536;

/// 插件能力集合的固定名称。
pub const COLLECTION_PLUGIN: &str = "plugin";
/// 提示词集合的固定名称。
pub const COLLECTION_PROMPT: &str = "prompt";
/// 聊天插件集合（chat 分类插件）。
pub const COLLECTION_CHAT_PLUGIN: &str = "chat_plugin";
/// 聊天插件集合的默认维度（qwen3-embed）。
const EMBED_DIM: i64 = 1024;

type FfiFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

static KV_LIB: OnceLock<Library> = OnceLock::new();

fn ensure_library() -> Result<&'static Library> {
    if let Some(lib) = KV_LIB.get() {
        return Ok(lib);
    }

    let lib = unsafe { Library::new(KV_LIB_PATH) }
        .with_context(|| format!("加载 {} 失败", KV_LIB_PATH))?;
    let _ = KV_LIB.set(lib);
    eprintln!("[kv_lib] 加载 {} 成功", KV_LIB_PATH);
    KV_LIB.get().context("kv_lib 库未初始化")
}

/// 检查 FFI 返回值：`0` 已写入，`-2` 缓冲区不足。
fn check_ret(symbol: &str, ret: c_int) -> Result<()> {
    if ret == -2 {
        bail!("kv_lib {} 输出超过 64 KiB", symbol);
    }
    if ret != 0 {
        bail!("kv_lib {} FFI 返回 {}", symbol, ret);
    }
    Ok(())
}

/// 解析 kv_lib 的 JSON 响应并取出 `data`。
///
/// 纯函数，便于单测。业务错误（`code != 200`）转为 `Err`。
pub fn parse_response(symbol: &str, out: &str) -> Result<Value> {
    if out.trim().is_empty() {
        bail!("kv_lib {} 返回空", symbol);
    }

    let value: Value = serde_json::from_str(out)
        .with_context(|| format!("kv_lib {} 响应解析失败: {}", symbol, out))?;

    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 200 {
        let msg = value
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        bail!("kv_lib {} 错误 (code={}): {}", symbol, code, msg);
    }

    Ok(value.get("data").cloned().unwrap_or(Value::Null))
}

/// 调用任意 `post_kv_<symbol>`，返回响应中的 `data`。
pub fn call_ffi(symbol: &str, request: &Value) -> Result<Value> {
    let lib = ensure_library()?;
    let name = format!("post_kv_{}", symbol);
    let function: libloading::Symbol<'_, FfiFn> = unsafe {
        lib.get(name.as_bytes())
            .with_context(|| format!("dlsym {} 失败", name))?
    };

    let req = CString::new(request.to_string()).context("请求 JSON 含 \\0")?;
    let mut out_buf = vec![0u8; OUT_BUF_SIZE];
    let ret = unsafe { function(req.as_ptr(), out_buf.as_mut_ptr() as *mut c_char) };
    check_ret(symbol, ret)?;

    let out = unsafe { CStr::from_ptr(out_buf.as_ptr() as *const c_char) }
        .to_str()
        .context("kv_lib 响应非 UTF-8")?;
    parse_response(symbol, out)
}

/// `kv_collection_list`：集合列表与统计。
pub fn collection_list(page: Option<i64>, page_size: Option<i64>) -> Result<Value> {
    call_ffi(
        "collection_list",
        &json!({ "page": page, "page_size": page_size }),
    )
}

/// `kv_get`：按 key 读取记录；缺失的 key 放在 `missing` 中。
pub fn get(collection: &str, keys: &[&str], with_vector: bool) -> Result<Value> {
    call_ffi(
        "get",
        &json!({
            "collection": collection,
            "keys": keys,
            "with_vector": with_vector
        }),
    )
}

/// `kv_put`：批量写入 / 更新记录；带 `text` 时自动生成向量。
pub fn put(collection: &str, items: &[Value]) -> Result<Value> {
    call_ffi(
        "put",
        &json!({
            "collection": collection,
            "items": items
        }),
    )
}

/// `kv_list`：分页扫描集合记录。
pub fn list(
    collection: &str,
    prefix: Option<&str>,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<Value> {
    call_ffi(
        "list",
        &json!({
            "collection": collection,
            "prefix": prefix,
            "page": page,
            "page_size": page_size
        }),
    )
}

/// `kv_search`：按文本相似度检索。
///
/// `candidate_keys` 用于先由业务元数据缩小范围，再点查向量。
#[allow(clippy::too_many_arguments)]
pub fn search(
    collection: &str,
    query: &str,
    top_k: i64,
    min_score: f64,
    candidate_keys: Option<&[String]>,
    with_value: bool,
) -> Result<Value> {
    call_ffi(
        "search",
        &json!({
            "collection": collection,
            "query": query,
            "top_k": top_k,
            "min_score": min_score,
            "candidate_keys": candidate_keys,
            "with_value": with_value
        }),
    )
}

/// 在 `plugin` 集合中检索插件能力，返回候选列表。
///
/// 每条候选至少包含 `key` / `score`；`with_value=true` 时还带出
/// `value`（插件元数据 JSON）与 `meta`。
pub fn search_plugins(query: &str, top_k: i64) -> Result<Vec<Value>> {
    let data = search(COLLECTION_PLUGIN, query, top_k, 0.0, None, true)?;
    Ok(data
        .get("list")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// `kv_collection_put`：创建 / 更新集合。
///
/// 集合已存在时只更新传入字段，`created = false`，可安全重复调用。
pub fn collection_put(code: &str, name: &str, description: &str, sort: i64) -> Result<Value> {
    call_ffi(
        "collection_put",
        &json!({
            "code": code,
            "name": name,
            "description": description,
            "dim": EMBED_DIM,
            "model": "qwen3-embed",
            "shard_max_mb": 16,
            "index_min_rows": 0,
            "state": 0,
            "sort": sort
        }),
    )
}

/// 确保 `chat_plugin` 集合存在（首次调用时创建）。
pub fn ensure_chat_plugin_collection() -> Result<()> {
    collection_put(
        COLLECTION_CHAT_PLUGIN,
        "聊天插件",
        "chat 分类插件的能力描述向量",
        50,
    )?;
    Ok(())
}

/// 在 `chat_plugin` 集合中检索聊天插件，返回候选列表。
///
/// 集合不存在时会先创建；调用方的检索参数与 `search` 一致。
pub fn search_chat_plugins(query: &str, top_k: i64) -> Result<Vec<Value>> {
    ensure_chat_plugin_collection()?;
    let data = search(COLLECTION_CHAT_PLUGIN, query, top_k, 0.0, None, true)?;
    Ok(data
        .get("list")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// `kv_embed`：为已有记录补齐 / 刷新向量。
pub fn embed(
    collection: &str,
    keys: Option<&[String]>,
    limit: Option<i64>,
    offset: Option<i64>,
    force: bool,
) -> Result<Value> {
    call_ffi(
        "embed",
        &json!({
            "collection": collection,
            "keys": keys,
            "limit": limit,
            "offset": offset,
            "force": force
        }),
    )
}

/// `kv_stats`：集合 / 分片 / 索引统计；`collection` 为空时返回全部。
pub fn stats(collection: Option<&str>) -> Result<Value> {
    call_ffi("stats", &json!({ "collection": collection }))
}

#[cfg(test)]
mod tests {
    use super::parse_response;

    #[test]
    fn parse_response_success_returns_data() {
        let out = r#"{"code":200,"data":{"list":[{"key":"plugin-cdr-check-txt","score":0.94}]},"msg":"success"}"#;
        let data = parse_response("search", out).unwrap();
        assert_eq!(data["list"][0]["key"], "plugin-cdr-check-txt");
    }

    #[test]
    fn parse_response_error_code() {
        let out = r#"{"code":400600,"data":{},"msg":"collection not found"}"#;
        let err = parse_response("search", out).unwrap_err().to_string();
        assert!(err.contains("400600"));
        assert!(err.contains("collection not found"));
    }

    #[test]
    fn parse_response_bad_json() {
        assert!(parse_response("search", "not json").is_err());
    }

    #[test]
    fn parse_response_empty() {
        assert!(parse_response("search", "").is_err());
    }
}
