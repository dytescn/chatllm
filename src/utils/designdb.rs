// src/utils/designdb.rs
//! designdb（PoloDB）调用封装。
//!
//! designdb 以 `cdylib` 形式提供 `post_db_*` 符号，chatllm 通过 `dlopen` 直接调用，
//! 不经过 cyctron 的 HTTP 层。逻辑库名固定为 `chatllm`。
//!
//! 目前只依赖 designdb 1.0.0 已发布的 5 个符号（query / insert / update / delete /
//! create）。带条件计数与索引由本模块用分页查询在应用侧兜底，等 designdb 提供
//! `db_count` / `db_create_index` 后再替换。

use anyhow::{bail, Context, Result};
use libloading::Library;
use serde_json::{json, Value};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::OnceLock;

#[cfg(windows)]
const DESIGNDB_LIB: &str = "./libary/libdesigndb.dll";
#[cfg(target_os = "linux")]
const DESIGNDB_LIB: &str = "./libary/libdesigndb.so";
#[cfg(target_os = "macos")]
const DESIGNDB_LIB: &str = "./libary/libdesigndb.dylib";

/// 与 designdb 的 `write_json_out` 保持一致。
const OUT_BUF_SIZE: usize = 65536;

/// chatllm 唯一逻辑库名（旧单库实现，保留说明；新实现按对话 UUID 分库）。
/// 旧数据库 `chatllm` 由迁移脚本处理。

/// 分页扫描的页大小。designdb 单次响应上限 64 KiB，取 50 条留出余量。
const SCAN_PAGE_SIZE: u64 = 50;

type FfiFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

static LIB: OnceLock<Library> = OnceLock::new();
static QUERY_FN: OnceLock<FfiFn> = OnceLock::new();
static INSERT_FN: OnceLock<FfiFn> = OnceLock::new();
static UPDATE_FN: OnceLock<FfiFn> = OnceLock::new();
static DELETE_FN: OnceLock<FfiFn> = OnceLock::new();

fn ensure_library() -> Result<&'static Library> {
    if let Some(lib) = LIB.get() {
        return Ok(lib);
    }
    let lib = unsafe { Library::new(DESIGNDB_LIB) }
        .with_context(|| format!("加载 {} 失败", DESIGNDB_LIB))?;
    let _ = LIB.set(lib);
    println!("[designdb] 加载 {} 成功", DESIGNDB_LIB);
    LIB.get().context("designdb 库未初始化")
}

fn symbol(name: &[u8]) -> Result<FfiFn> {
    let lib = ensure_library()?;
    let function: libloading::Symbol<'_, FfiFn> = unsafe {
        lib.get(name)
            .with_context(|| format!("dlsym {} 失败", String::from_utf8_lossy(name)))?
    };
    Ok(*function)
}

fn query_fn() -> Result<FfiFn> {
    if let Some(f) = QUERY_FN.get() {
        return Ok(*f);
    }
    let f = symbol(b"post_db_query")?;
    let _ = QUERY_FN.set(f);
    Ok(f)
}

fn insert_fn() -> Result<FfiFn> {
    if let Some(f) = INSERT_FN.get() {
        return Ok(*f);
    }
    let f = symbol(b"post_db_insert")?;
    let _ = INSERT_FN.set(f);
    Ok(f)
}

fn update_fn() -> Result<FfiFn> {
    if let Some(f) = UPDATE_FN.get() {
        return Ok(*f);
    }
    let f = symbol(b"post_db_update")?;
    let _ = UPDATE_FN.set(f);
    Ok(f)
}

fn delete_fn() -> Result<FfiFn> {
    if let Some(f) = DELETE_FN.get() {
        return Ok(*f);
    }
    let f = symbol(b"post_db_delete")?;
    let _ = DELETE_FN.set(f);
    Ok(f)
}

fn call(function: FfiFn, req: Value) -> Result<Value> {
    println!(
        "[designdb] call: path={:?}, collection={:?}",
        req.get("path").and_then(Value::as_str),
        req.get("collection").and_then(Value::as_str)
    );
    let req_c = CString::new(req.to_string()).context("designdb 请求 JSON 含 \\0")?;
    let mut out_buf = vec![0u8; OUT_BUF_SIZE];
    let ret = unsafe { function(req_c.as_ptr(), out_buf.as_mut_ptr() as *mut c_char) };

    let out_str = unsafe { CStr::from_ptr(out_buf.as_ptr() as *const c_char) }
        .to_str()
        .context("designdb 响应非 UTF-8")?;

    if out_str.is_empty() {
        bail!("designdb 返回空（ret={}）", ret);
    }

    let value: Value = serde_json::from_str(out_str)
        .with_context(|| format!("designdb 响应解析失败: {}", out_str))?;

    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    println!("[designdb] call 响应 code={}", code);
    if code != 200 && code != 0 {
        let msg = value
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        bail!("designdb 错误 (code={}): {}", code, msg);
    }

    Ok(value)
}

fn data_of(value: &Value) -> Value {
    value.get("data").cloned().unwrap_or(Value::Null)
}

pub fn query(
    path: &str,
    collection: &str,
    filter: Option<Value>,
    sort: Option<Value>,
    skip: Option<u64>,
    limit: Option<u64>,
) -> Result<Vec<Value>> {
    println!(
        "[designdb] query: path={:?}, collection={:?}, skip={:?}, limit={:?}",
        path, collection, skip, limit
    );
    let mut req = json!({
        "path": path,
        "collection": collection,
    });
    if let Some(v) = filter {
        req["filter"] = v;
    }
    if let Some(v) = sort {
        req["sort"] = v;
    }
    if let Some(v) = skip {
        req["skip"] = json!(v);
    }
    if let Some(v) = limit {
        req["limit"] = json!(v);
    }

    let value = call(query_fn()?, req)?;
    Ok(data_of(&value).as_array().cloned().unwrap_or_default())
}

/// 分页读取全部匹配文档。
///
/// designdb 1.0.0 没有过滤计数，先由应用侧扫描；数据量增长后应替换为
/// `db_count`（见 `sys/chatllm/db_todo.md` DT-02）。
pub fn query_all(
    path: &str,
    collection: &str,
    filter: Option<Value>,
    sort: Option<Value>,
) -> Result<Vec<Value>> {
    let mut all = Vec::new();
    let mut skip = 0u64;

    loop {
        let batch = query(
            path,
            collection,
            filter.clone(),
            sort.clone(),
            Some(skip),
            Some(SCAN_PAGE_SIZE),
        )?;
        let count = batch.len();
        all.extend(batch);
        if count < SCAN_PAGE_SIZE as usize {
            break;
        }
        skip += SCAN_PAGE_SIZE;
    }

    Ok(all)
}

/// 是否存在匹配文档（最多读 1 条）。
pub fn exists(path: &str, collection: &str, filter: Option<Value>) -> Result<bool> {
    let found = !query(path, collection, filter, None, None, Some(1))?.is_empty();
    println!("[designdb] exists: {} -> {}", collection, found);
    Ok(found)
}

pub fn insert(path: &str, collection: &str, doc: Value) -> Result<()> {
    println!(
        "[designdb] insert: path={:?}, collection={:?}",
        path, collection
    );
    let req = json!({
        "path": path,
        "collection": collection,
        "doc": doc,
    });
    call(insert_fn()?, req)?;
    Ok(())
}

pub fn update(path: &str, collection: &str, filter: Option<Value>, update: Value) -> Result<()> {
    println!(
        "[designdb] update: path={:?}, collection={:?}",
        path, collection
    );
    let mut req = json!({
        "path": path,
        "collection": collection,
        "update": update,
    });
    if let Some(v) = filter {
        req["filter"] = v;
    }
    call(update_fn()?, req)?;
    Ok(())
}

pub fn delete(path: &str, collection: &str, filter: Option<Value>) -> Result<()> {
    println!(
        "[designdb] delete: path={:?}, collection={:?}",
        path, collection
    );
    let mut req = json!({
        "path": path,
        "collection": collection,
    });
    if let Some(v) = filter {
        req["filter"] = v;
    }
    let _ = call(delete_fn()?, req)?;
    Ok(())
}
