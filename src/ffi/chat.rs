// src/ffi/chat.rs
//
// chatllm 对外只有三个 FFI 接口，调用约定见 `docs/ffi.md`：
// `int (*)(const char *json_in, char *json_out);`
// 本层只做输入解析与错误码映射，业务逻辑在 `utils::dialogue`。
use crate::ffi::types::{ChatFlow, Dialogue, DialogueListReq, DialogueQuery};
use crate::utils::cjson::{err_msg, read_body, write_error, write_success};
use crate::utils::dialogue;
use serde_json::json;
use std::ffi::{c_char, c_int};

/// 对话列表。
///
/// FFI-Symbol：`patch_dialogue`
/// 入参：`DialogueListReq`（`keyword` / `page` / `page_size`，均可空，允许空 body）
/// 出参：`{ list, page, page_size, total }`
#[unsafe(no_mangle)]
pub extern "C" fn patch_dialogue(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = if json_in.is_null() {
        ""
    } else {
        match read_body(json_in) {
            Ok(body) => body,
            Err(code) => return write_error(json_out, code, err_msg(code)),
        }
    };
    let req: DialogueListReq = if body.trim().is_empty() {
        DialogueListReq {
            keyword: None,
            page: None,
            page_size: None,
        }
    } else {
        match serde_json::from_str(body) {
            Ok(req) => req,
            Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
        }
    };

    match dialogue::query_dialogue(req.keyword.as_deref(), req.page, req.page_size) {
        Ok(data) => write_success(json_out, "success", data),
        Err(e) => write_error(json_out, 400302, &format!("list dialogue failed: {:#}", e)),
    }
}

/// 读取一条对话及其消息。
///
/// FFI-Symbol：`get_dialogue`
/// 入参：`DialogueQuery`（`uuid` 必填，`before_id` / `after_id` / `limit` 可选）
/// 出参：`{ dialogue: {...}, messages: { list, limit, first_id, last_id, has_more_before, has_more_after } }`
#[unsafe(no_mangle)]
pub extern "C" fn get_dialogue(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(body) => body,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: DialogueQuery = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };

    match dialogue::dialogue_detail(&req.uuid, req.before_id, req.after_id, req.limit) {
        Ok(data) => write_success(json_out, "success", data),
        Err(e) => write_error(json_out, e.code(), &e.message()),
    }
}

/// 更新对话信息。
///
/// FFI-Symbol：`update_dialogue`
/// 入参：`Dialogue`（`uuid` 定位对话，`title` / `show_status` 为待更新项）
/// 出参：`{ id }`
#[unsafe(no_mangle)]
pub extern "C" fn update_dialogue(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(body) => body,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: Dialogue = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };

    match dialogue::update_chat(
        req.uuid.as_deref(),
        req.title.as_deref(),
        req.show_status,
        req.is_pinned,
        req.is_archived,
    ) {
        Ok(id) => write_success(json_out, "success", json!({ "id": id })),
        Err(e) => write_error(json_out, e.code(), &e.message()),
    }
}

/// 创建一条对话。
///
/// FFI-Symbol：`post_message`
/// 入参：`ChatFlow`（`prompt` 必填；`uuid` 非空时续聊已有对话）
/// 出参：`{ uuid, created, prompt, max_tokens }`
#[unsafe(no_mangle)]
pub extern "C" fn post_message(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(body) => body,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: ChatFlow = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };

    match dialogue::open_chat(
        &req.prompt,
        req.uuid.as_deref(),
        req.plugin_uuid.as_deref(),
        req.title.as_deref(),
        req.show_status,
    ) {
        Ok((uuid, created)) => write_success(
            json_out,
            "success",
            json!({
                "uuid": uuid,
                "created": created,
                "prompt": req.prompt,
                "max_tokens": req.max_tokens,
            }),
        ),
        Err(e) => write_error(json_out, e.code(), &e.message()),
    }
}
