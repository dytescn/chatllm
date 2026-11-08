// src/ffi/call_back.rs
//
// 直接调用插件的 FFI 入口（`post_call_chat`）。
//
// 与 `ffi::chat::post_message` 不同：不执行 think_base / reply，按 `category`
// 在 kv 中定位插件并把 `{plugins_uid, category, prompts, context}` 原样交给
// 插件，返回插件响应的 `data`。实现见 `utils::call_flow`。
use crate::ffi::types::CallChatReq;
use crate::utils::call_flow;
use crate::utils::cjson::{err_msg, read_body, write_error, write_success};
use std::ffi::{c_char, c_int};

/// 直接调用插件。
///
/// FFI-Symbol：`call_chat`
/// 入参：`CallChatReq`（`category` 必填；`plugins_uid` / `prompts` / `context` 可选）
/// 出参：插件响应中的 `data`（原样返回）
#[unsafe(no_mangle)]
pub extern "C" fn post_call_chat(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(body) => body,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: CallChatReq = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };

    match call_flow::run(&req) {
        Ok(data) => write_success(json_out, "success", data),
        Err(e) => write_error(json_out, e.code(), &e.message()),
    }
}
