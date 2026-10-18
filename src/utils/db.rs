// src/utils/db.rs
use anyhow::{Context, Result};
use serde_json::Value;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::OnceLock;

/// database.so 路径（相对 cwd = /home/funx/app/piksel）
const DB_SO_PATH: &str = "./libary/database.so";

/// 输出缓冲区大小
const OUT_BUF_SIZE: usize = 65536;

type PostDbQueryFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

static DB_QUERY_FN: OnceLock<PostDbQueryFn> = OnceLock::new();

fn load_db_query() -> Result<PostDbQueryFn> {
    // 优先 RTLD_DEFAULT（piksel 可能已加载 database.so）
    let sym_name = CString::new("post_db_query").unwrap();
    let existing = unsafe { libc::dlsym(libc::RTLD_DEFAULT, sym_name.as_ptr()) };
    if !existing.is_null() {
        let f: PostDbQueryFn = unsafe { std::mem::transmute(existing) };
        eprintln!("[db] 使用 RTLD_DEFAULT 找到 post_db_query");
        return Ok(f);
    }

    // 显式 dlopen
    let path = CString::new(DB_SO_PATH).context("构造 .so 路径失败")?;
    let handle =
        unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL) };
    if handle.is_null() {
        let err = unsafe {
            let e = libc::dlerror();
            if e.is_null() {
                "unknown dlopen error".to_string()
            } else {
                CStr::from_ptr(e).to_string_lossy().into_owned()
            }
        };
        anyhow::bail!("dlopen {} 失败: {}", DB_SO_PATH, err);
    }

    let ptr = unsafe { libc::dlsym(handle, sym_name.as_ptr()) };
    if ptr.is_null() {
        anyhow::bail!("dlsym post_db_query 失败");
    }

    let f: PostDbQueryFn = unsafe { std::mem::transmute(ptr) };
    eprintln!("[db] 显式 dlopen {} 成功", DB_SO_PATH);
    Ok(f)
}

fn db_query_fn() -> Result<PostDbQueryFn> {
    if let Some(f) = DB_QUERY_FN.get() {
        return Ok(*f);
    }
    let f = load_db_query()?;
    let _ = DB_QUERY_FN.set(f);
    Ok(f)
}

/// 同步执行 db_query。
/// `db_name` 为库名（不含 .db），database.so 内部拼成 ./database/<db_name>.db
pub fn db_query(db_name: &str, sql: &str) -> Result<Vec<Value>> {
    let f = db_query_fn()?;

    let req = serde_json::json!({
        "path": db_name,
        "sql": sql
    });
    let req_c = CString::new(req.to_string()).context("请求 JSON 含 \\0")?;

    let mut out_buf = vec![0u8; OUT_BUF_SIZE];

    let ret = unsafe { f(req_c.as_ptr(), out_buf.as_mut_ptr() as *mut c_char) };

    let out_str = unsafe { CStr::from_ptr(out_buf.as_ptr() as *const c_char) }
        .to_str()
        .context("响应非 UTF-8")?;

    if out_str.is_empty() {
        anyhow::bail!("db_query 返回空（ret={}）", ret);
    }

    let value: Value = serde_json::from_str(out_str)
        .with_context(|| format!("db_query 响应解析失败: {}", out_str))?;

    let code = value.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code != 200 && code != 0 {
        let msg = value
            .get("msg")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown");
        anyhow::bail!("db_query 错误 (code={}): {}", code, msg);
    }

    Ok(extract_rows(&value))
}

fn extract_rows(v: &Value) -> Vec<Value> {
    if let Some(arr) = v.get("data").and_then(|d| d.as_array()) {
        return arr.clone();
    }
    if let Some(arr) = v.get("data").and_then(|d| d.get("list")).and_then(|l| l.as_array()) {
        return arr.clone();
    }
    if let Some(arr) = v.get("data").and_then(|d| d.get("rows")).and_then(|r| r.as_array()) {
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
