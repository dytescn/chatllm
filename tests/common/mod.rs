// tests/common/mod.rs —— 三个 FFI 接口的调用辅助
#![allow(dead_code)]

use serde_json::Value;
use std::ffi::{c_char, c_int, CStr, CString};
use std::sync::Once;

const TESTENV: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testenv");
const TESTENV_LIB: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testenv/libary/database.dll");
const TESTENV_DB: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testenv/database/dialogue.db");

static INIT: Once = Once::new();

/// 数据调用测试使用 `testenv` 下的宿主库与数据库。
///
/// `database.dll` 以相对路径加载 `./libary/*.dll` 与 `./database/*.db`，
/// 因此首次调用时把进程工作目录切到 `testenv`；缺少宿主文件时返回 false（测试跳过）。
pub fn db_ready() -> bool {
    if !std::path::Path::new(TESTENV_LIB).exists() || !std::path::Path::new(TESTENV_DB).exists() {
        return false;
    }
    INIT.call_once(|| {
        std::env::set_current_dir(TESTENV).expect("切换到 testenv 失败");
    });
    true
}

pub fn skip(reason: &str) {
    eprintln!(
        "跳过 {}：testenv/libary/database.dll 或 testenv/database/dialogue.db 不存在",
        reason
    );
}

/// 调用一个 FFI 接口，打印请求与响应，返回解析后的响应 JSON。
pub fn call(entry: extern "C" fn(*const c_char, *mut c_char) -> c_int, body: &str) -> Value {
    let input = CString::new(body).expect("请求含 \\0");
    let mut out = [0 as c_char; 65536];
    let rc = entry(input.as_ptr(), out.as_mut_ptr());
    let out = unsafe { CStr::from_ptr(out.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    println!("request : {}", body);
    println!("rc      : {}", rc);
    println!("response: {}", out);
    assert_eq!(rc, 0);
    serde_json::from_str(&out).expect("响应不是合法 JSON")
}
