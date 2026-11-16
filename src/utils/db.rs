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
#[cfg(target_os = "linux")]
const DB_SO_PATH: &str = "./libary/database.so";
#[cfg(target_os = "macos")]
const DB_SO_PATH: &str = "./libary/database.dylib";

/// 输出缓冲区大小：database 1.0.6 约定 `json_out` 至少 1 MiB。
const OUT_BUF_SIZE: usize = 1024 * 1024;

type DbFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

static DB_LIB: OnceLock<Library> = OnceLock::new();
static DB_CREATE_FN: OnceLock<DbFn> = OnceLock::new();
static DB_QUERY_FN: OnceLock<DbFn> = OnceLock::new();
static DB_INSERT_FN: OnceLock<DbFn> = OnceLock::new();
static DB_UPDATE_FN: OnceLock<DbFn> = OnceLock::new();
static DB_DELETE_FN: OnceLock<DbFn> = OnceLock::new();

fn ensure_library() -> Result<&'static Library> {
    if let Some(lib) = DB_LIB.get() {
        return Ok(lib);
    }

    let lib =
        unsafe { Library::new(DB_SO_PATH) }.with_context(|| format!("加载 {} 失败", DB_SO_PATH))?;
    let _ = DB_LIB.set(lib);
    println!("[db] 加载 {} 成功", DB_SO_PATH);

    DB_LIB.get().context("database 库未初始化")
}

fn load_symbol(name: &[u8]) -> Result<DbFn> {
    let lib = ensure_library()?;
    let function = unsafe {
        *lib.get::<DbFn>(name)
            .with_context(|| format!("dlsym {} 失败", String::from_utf8_lossy(name)))?
    };
    Ok(function)
}

fn cached_fn(slot: &'static OnceLock<DbFn>, name: &[u8]) -> Result<DbFn> {
    if let Some(function) = slot.get() {
        return Ok(*function);
    }
    let function = load_symbol(name)?;
    let _ = slot.set(function);
    Ok(function)
}

fn db_create_fn() -> Result<DbFn> {
    cached_fn(&DB_CREATE_FN, b"db_create")
}

fn db_query_fn() -> Result<DbFn> {
    cached_fn(&DB_QUERY_FN, b"db_query")
}

fn db_insert_fn() -> Result<DbFn> {
    cached_fn(&DB_INSERT_FN, b"db_insert")
}

fn db_update_fn() -> Result<DbFn> {
    cached_fn(&DB_UPDATE_FN, b"db_update")
}

fn db_delete_fn() -> Result<DbFn> {
    cached_fn(&DB_DELETE_FN, b"db_delete")
}

/// 调用 database 的 `db_*` 接口，请求为完整 JSON 对象。
fn call_ffi(function: DbFn, db_name: &str, request: &Value) -> Result<Value> {
    println!(
        "[db] call_ffi: db={:?}, request={:?}",
        db_name,
        preview(&request.to_string(), 300)
    );
    let req_c = CString::new(request.to_string()).context("请求 JSON 含 \\0")?;

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
    println!("[db] call_ffi 响应 code={}", code);
    if code != 200 && code != 0 {
        let msg = value
            .get("msg")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown");
        anyhow::bail!("database 错误 (code={}): {}", code, msg);
    }

    Ok(value)
}

/// 构造 `path` + `sql` 请求。
fn sql_request(db_name: &str, sql: &str) -> Value {
    serde_json::json!({
        "path": db_name,
        "sql": sql
    })
}

/// 同步执行 db_query。
/// `db_name` 为库名（不含 .db），database.so 内部拼成 ./database/<db_name>.db
pub fn db_query(db_name: &str, sql: &str) -> Result<Vec<Value>> {
    let value = call_ffi(db_query_fn()?, db_name, &sql_request(db_name, sql))?;
    let rows = extract_rows(&value);
    println!("[db] db_query: db={:?}, rows={}", db_name, rows.len());
    Ok(rows)
}

/// 创建表（`db_create`）：`schema` 为列定义，表已存在时静默成功。
pub fn db_create(db_name: &str, table: &str, schema: &str) -> Result<()> {
    println!("[db] db_create: db={:?}, table={:?}", db_name, table);
    let request = serde_json::json!({
        "path": db_name,
        "table": table,
        "schema": schema
    });
    let _ = call_ffi(db_create_fn()?, db_name, &request)?;
    Ok(())
}

/// 执行写语句（CREATE / INSERT / UPSERT / UPDATE）。
pub fn db_execute(db_name: &str, sql: &str) -> Result<()> {
    println!("[db] db_execute: db={:?}", db_name);
    let _ = call_ffi(db_insert_fn()?, db_name, &sql_request(db_name, sql))?;
    Ok(())
}

/// 执行更新语句。
pub fn db_update(db_name: &str, sql: &str) -> Result<()> {
    println!("[db] db_update: db={:?}", db_name);
    let _ = call_ffi(db_update_fn()?, db_name, &sql_request(db_name, sql))?;
    Ok(())
}

/// 执行删除语句（`db_delete`）。
pub fn db_delete(db_name: &str, sql: &str) -> Result<()> {
    println!("[db] db_delete: db={:?}", db_name);
    let _ = call_ffi(db_delete_fn()?, db_name, &sql_request(db_name, sql))?;
    Ok(())
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
