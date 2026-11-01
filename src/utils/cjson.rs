use serde_json::{json, Value};
use std::ffi::{c_char, c_int, CStr};

/// 接口尚未实现时返回的业务错误码；业务逻辑补齐后移除。
pub const ERR_NOT_IMPLEMENTED: i32 = 400500;

/// 读取 C 侧传入的 NUL 结尾 UTF-8 JSON 文本。
///
/// `json_in` 为空返回 `400100`，非 UTF-8 返回 `400101`。
pub fn read_body<'a>(json_in: *const c_char) -> Result<&'a str, c_int> {
    if json_in.is_null() {
        return Err(400100);
    }
    unsafe { CStr::from_ptr(json_in).to_str().map_err(|_| 400101) }
}

/// 输入类错误码对应的提示文本。
pub fn err_msg(code: c_int) -> &'static str {
    match code {
        400100 => "json_in is null",
        400101 => "invalid utf-8",
        _ => "unknown error",
    }
}

/// 返回“接口未实现”错误，`symbol` 为导出函数名。
pub fn not_implemented(json_out: *mut c_char, symbol: &str) -> c_int {
    write_error(
        json_out,
        ERR_NOT_IMPLEMENTED,
        &format!("{} not implemented", symbol),
    )
}

/// 写 JSON 到 C buffer
pub fn write_json_out(json_out: *mut c_char, value: Value) -> c_int {
    let out_str = value.to_string();
    let bytes = out_str.as_bytes();

    unsafe {
        if json_out.is_null() {
            return -1;
        }

        if bytes.len() >= 65536 {
            return -2;
        }

        std::ptr::copy_nonoverlapping(bytes.as_ptr(), json_out as *mut u8, bytes.len());

        *json_out.add(bytes.len()) = 0;
    }

    0
}

/// 返回错误 JSON
pub fn write_error(json_out: *mut c_char, code: i32, msg: &str) -> c_int {
    write_json_out(
        json_out,
        json!({
            "code": code,
            "data": {},
            "msg": msg
        }),
    )
}

/// 返回成功 JSON
pub fn write_success(json_out: *mut c_char, msg: &str, data: Value) -> c_int {
    write_json_out(
        json_out,
        json!({
            "code": 200,
            "data": data,
            "msg": msg
        }),
    )
}
