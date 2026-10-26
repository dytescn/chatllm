// src/utils/db.rs
use anyhow::{Context, Result};
use libloading::Library;
use serde_json::Value;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::OnceLock;

/// database.so 路径（相对 cwd = /home/funx/app/piksel）
#[cfg(windows)]
const DB_SO_PATH: &str = "./libary/database.dll";
#[cfg(not(windows))]
const DB_SO_PATH: &str = "./libary/database.so";

/// 输出缓冲区大小
const OUT_BUF_SIZE: usize = 65536;

type PostDbQueryFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;
type PostDbInsertFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;
type PostDbUpdateFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

static DB_LIB: OnceLock<Library> = OnceLock::new();
static DB_QUERY_FN: OnceLock<PostDbQueryFn> = OnceLock::new();
static DB_INSERT_FN: OnceLock<PostDbInsertFn> = OnceLock::new();
static DB_UPDATE_FN: OnceLock<PostDbUpdateFn> = OnceLock::new();

fn ensure_library() -> Result<&'static Library> {
    if let Some(lib) = DB_LIB.get() {
        return Ok(lib);
    }

    let lib =
        unsafe { Library::new(DB_SO_PATH) }.with_context(|| format!("加载 {} 失败", DB_SO_PATH))?;
    let _ = DB_LIB.set(lib);
    eprintln!("[db] 加载 {} 成功", DB_SO_PATH);

    DB_LIB.get().context("database 库未初始化")
}

fn db_query_fn() -> Result<PostDbQueryFn> {
    if let Some(f) = DB_QUERY_FN.get() {
        return Ok(*f);
    }
    let lib = ensure_library()?;
    let f = unsafe {
        *lib.get::<PostDbQueryFn>(b"post_db_query")
            .context("dlsym post_db_query 失败")?
    };
    let _ = DB_QUERY_FN.set(f);
    Ok(f)
}

fn db_insert_fn() -> Result<PostDbInsertFn> {
    if let Some(f) = DB_INSERT_FN.get() {
        return Ok(*f);
    }
    let lib = ensure_library()?;
    let f = unsafe {
        *lib.get::<PostDbInsertFn>(b"post_db_insert")
            .context("dlsym post_db_insert 失败")?
    };
    let _ = DB_INSERT_FN.set(f);
    Ok(f)
}

fn db_update_fn() -> Result<PostDbUpdateFn> {
    if let Some(f) = DB_UPDATE_FN.get() {
        return Ok(*f);
    }
    let lib = ensure_library()?;
    let f = unsafe {
        *lib.get::<PostDbUpdateFn>(b"post_db_update")
            .context("dlsym post_db_update 失败")?
    };
    let _ = DB_UPDATE_FN.set(f);
    Ok(f)
}

fn call_ffi(
    function: unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int,
    db_name: &str,
    sql: &str,
) -> Result<Value> {
    let req = serde_json::json!({
        "path": db_name,
        "sql": sql
    });
    let req_c = CString::new(req.to_string()).context("请求 JSON 含 \\0")?;

    let mut out_buf = vec![0u8; OUT_BUF_SIZE];
    let ret = unsafe { function(req_c.as_ptr(), out_buf.as_mut_ptr() as *mut c_char) };

    let out_str = unsafe { CStr::from_ptr(out_buf.as_ptr() as *const c_char) }
        .to_str()
        .context("响应非 UTF-8")?;

    if out_str.is_empty() {
        anyhow::bail!("database 返回空（ret={}）", ret);
    }

    let value: Value = serde_json::from_str(out_str)
        .with_context(|| format!("database 响应解析失败: {}", out_str))?;

    let code = value.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code != 200 && code != 0 {
        let msg = value
            .get("msg")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown");
        anyhow::bail!("database 错误 (code={}): {}", code, msg);
    }

    Ok(value)
}

/// 同步执行 db_query。
/// `db_name` 为库名（不含 .db），database.so 内部拼成 ./database/<db_name>.db
pub fn db_query(db_name: &str, sql: &str) -> Result<Vec<Value>> {
    let value = call_ffi(db_query_fn()?, db_name, sql)?;
    Ok(extract_rows(&value))
}

/// 执行写语句（CREATE / INSERT / UPSERT / UPDATE）。
pub fn db_execute(db_name: &str, sql: &str) -> Result<()> {
    let _ = call_ffi(db_insert_fn()?, db_name, sql)?;
    Ok(())
}

/// 执行更新 / 删除语句。
pub fn db_update(db_name: &str, sql: &str) -> Result<()> {
    let _ = call_ffi(db_update_fn()?, db_name, sql)?;
    Ok(())
}

fn extract_rows(v: &Value) -> Vec<Value> {
    if let Some(arr) = v.get("data").and_then(|d| d.as_array()) {
        return arr.clone();
    }
    if let Some(arr) = v
        .get("data")
        .and_then(|d| d.get("list"))
        .and_then(|l| l.as_array())
    {
        return arr.clone();
    }
    if let Some(arr) = v
        .get("data")
        .and_then(|d| d.get("rows"))
        .and_then(|r| r.as_array())
    {
        return arr.clone();
    }
    if let Some(arr) = v.get("list").and_then(|l| l.as_array()) {
        return arr.clone();
    }
    if let Some(arr) = v.get("rows").and_then(|r| r.as_array()) {
        return arr.clone();
    }
    Vec::new()
}
