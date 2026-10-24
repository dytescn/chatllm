// src/ffi/chat.rs
use crate::ffi::run::run_chat;
use crate::ffi::types::{
    ChatDetailReq, ChatReq, DialogueListReq, DialogueSearchReq, DialogueUpdateReq,
};
use crate::utils::cjson::{write_error, write_success};
use crate::utils::dialogue::{
    is_valid_uuid, list_dialogue, list_messages, search_dialogue, update_dialogue,
    DialoguePatch,
};
use serde_json::json;
use std::ffi::{c_char, c_int, CStr};

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

// 1. chat
#[unsafe(no_mangle)]
pub extern "C" fn post_chat(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(s) => s,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: ChatReq = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };

    match run_chat(
        &req.prompt,
        req.uuid.as_deref(),
        req.name.as_deref(),
        req.keyword.as_deref(),
    ) {
        Ok(out) => write_success(
            json_out,
            "success",
            json!({
                "uuid": out.uuid,
                "created": out.created,
                "prompt": req.prompt,
                "content": out.content,
                "content_html": out.content_html,
                "max_tokens": req.max_tokens,
                "keyword": req.keyword,
            }),
        ),
        Err(e) => write_error(json_out, e.code(), &e.message()),
    }
}

// 2. dialogue_list
#[unsafe(no_mangle)]
pub extern "C" fn post_dialogue_list(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = if json_in.is_null() {
        ""
    } else {
        match read_body(json_in) {
            Ok(s) => s,
            Err(code) => return write_error(json_out, code, err_msg(code)),
        }
    };

    let req: DialogueListReq = if body.trim().is_empty() {
        DialogueListReq { page: None, page_size: None }
    } else {
        match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return write_error(json_out, 400106, &format!("invalid json: {}", e)),
        }
    };

    match list_dialogue(req.page, req.page_size) {
        Ok(data) => write_success(json_out, "success", data),
        Err(e) => write_error(json_out, 400302, &format!("list dialogue failed: {:#}", e)),
    }
}

// 3. dialogue_update
#[unsafe(no_mangle)]
pub extern "C" fn post_dialogue_update(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(s) => s,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: DialogueUpdateReq = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return write_error(json_out, 400104, &format!("invalid json: {}", e)),
    };
    if !is_valid_uuid(&req.id) {
        return write_error(json_out, 400103, "invalid uuid");
    }

    let patch = DialoguePatch {
        title: req.title.as_deref(),
        keyword: req.keyword.as_deref(),
        model_name: req.model_name.as_deref(),
        system_prompt: req.system_prompt.as_deref(),
        model_config: req.model_config.as_deref(),
        is_pinned: req.is_pinned,
        is_archived: req.is_archived,
    };
    match update_dialogue(&req.id, &patch) {
        Ok(_) => write_success(json_out, "success", json!({ "id": req.id })),
        Err(e) => write_error(json_out, 400303, &format!("update dialogue failed: {:#}", e)),
    }
}

// 4. dialogue_search
#[unsafe(no_mangle)]
pub extern "C" fn post_dialogue_search(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = if json_in.is_null() {
        ""
    } else {
        match read_body(json_in) {
            Ok(s) => s,
            Err(code) => return write_error(json_out, code, err_msg(code)),
        }
    };

    let req: DialogueSearchReq = if body.trim().is_empty() {
        DialogueSearchReq { keyword: None, page: None, page_size: None }
    } else {
        match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return write_error(json_out, 400105, &format!("invalid json: {}", e)),
        }
    };

    match search_dialogue(req.keyword.as_deref().unwrap_or(""), req.page, req.page_size) {
        Ok(data) => write_success(json_out, "success", data),
        Err(e) => write_error(json_out, 400304, &format!("search dialogue failed: {:#}", e)),
    }
}

// 5. chat_detail（游标分页）
#[unsafe(no_mangle)]
pub extern "C" fn post_chat_detail(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(s) => s,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: ChatDetailReq = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return write_error(json_out, 400107, &format!("invalid json: {}", e)),
    };

    match list_messages(&req.uuid, req.before_id, req.after_id, req.limit) {
        Ok(data) => write_success(json_out, "success", data),
        Err(e) => write_error(json_out, 400306, &format!("chat_detail failed: {:#}", e)),
    }
}
