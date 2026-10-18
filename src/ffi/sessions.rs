// src/ffi/sessions.rs
// 表 session（库 "dialogue"）
use crate::utils::cjson::{write_error, write_success};
use crate::utils::dialogue::DialoguePatch; // 若不需要可删
use crate::utils::db::db_query;
use serde_json::{json, Value};
use std::ffi::{c_char, c_int, CStr};

const DB: &str = "dialogue";
const TABLE: &str = "session";

fn read_body<'a>(json_in: *const c_char) -> Result<&'a str, c_int> {
    if json_in.is_null() {
        return Err(400100);
    }
    unsafe { CStr::from_ptr(json_in).to_str().map_err(|_| 400101) }
}

fn err_msg(code: c_int) -> &'static str {
    match code {
        400100 => "json_in is null",
        400101 => "invalid utf-8",
        _ => "unknown error",
    }
}

fn parse_sql(body: &str) -> Result<String, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("invalid json: {}", e))?;
    let sql = v.get("sql").and_then(|s| s.as_str()).unwrap_or("");
    if sql.is_empty() {
        return Err("missing field: sql".to_string());
    }
    Ok(sql.to_string())
}

fn exec(json_in: *const c_char, json_out: *mut c_char, err_code: c_int) -> c_int {
    let body = match read_body(json_in) {
        Ok(s) => s,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let sql = match parse_sql(body) {
        Ok(s) => s,
        Err(e) => return write_error(json_out, 400102, &e),
    };
    match db_query(DB, &sql) {
        Ok(rows) => write_success(json_out, "success", json!(rows)),
        Err(e) => write_error(json_out, err_code, &format!("[{}] {:#}", TABLE, e)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn post_session_list(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    exec(json_in, json_out, 400301)
}

#[unsafe(no_mangle)]
pub extern "C" fn post_session_create(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    exec(json_in, json_out, 400302)
}

#[unsafe(no_mangle)]
pub extern "C" fn post_session_update(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    exec(json_in, json_out, 400303)
}

#[unsafe(no_mangle)]
pub extern "C" fn post_session_delete(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    exec(json_in, json_out, 400304)
}
