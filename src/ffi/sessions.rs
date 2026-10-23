use crate::utils::cjson::{write_error, write_success};
use crate::utils::sessions::{
    create_session, get_session, list_sessions, patch_session, update_session,
};
use serde::Deserialize;
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

#[derive(Debug, Deserialize)]
struct SessionsListReq {
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SessionsGetReq {
    id: String,
}

#[derive(Debug, Deserialize)]
struct SessionsCreateReq {
    id: Option<String>,
    title: Option<String>,
    model: Option<String>,
    system_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SessionsUpdateReq {
    id: String,
    title: Option<String>,
    model: Option<String>,
    system_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SessionsPatchReq {
    id: String,
    title: Option<String>,
    model: Option<String>,
    system_prompt: Option<String>,
}

#[unsafe(no_mangle)]
pub extern "C" fn post_sessions_list(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = if json_in.is_null() {
        ""
    } else {
        match read_body(json_in) {
            Ok(s) => s,
            Err(code) => return write_error(json_out, code, err_msg(code)),
        }
    };

    let req: SessionsListReq = if body.trim().is_empty() {
        SessionsListReq {
            page: None,
            page_size: None,
        }
    } else {
        match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
        }
    };

    match list_sessions(req.page, req.page_size) {
        Ok(data) => write_success(json_out, "success", data),
        Err(e) => write_error(json_out, 400310, &format!("sessions list failed: {:#}", e)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn post_sessions_get(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(s) => s,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: SessionsGetReq = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };
    if req.id.trim().is_empty() {
        return write_error(json_out, 400103, "id is required");
    }

    match get_session(&req.id) {
        Ok(Some(data)) => write_success(json_out, "success", data),
        Ok(None) => write_error(json_out, 404, "session not found"),
        Err(e) => write_error(json_out, 400310, &format!("sessions get failed: {:#}", e)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn post_sessions_post(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(s) => s,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: SessionsCreateReq = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };

    match create_session(
        req.id.as_deref(),
        req.title.as_deref(),
        req.model.as_deref(),
        req.system_prompt.as_deref(),
    ) {
        Ok(data) => write_success(json_out, "success", data),
        Err(e) => write_error(
            json_out,
            400310,
            &format!("sessions create failed: {:#}", e),
        ),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn post_sessions_update(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(s) => s,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: SessionsUpdateReq = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };
    if req.id.trim().is_empty() {
        return write_error(json_out, 400103, "id is required");
    }

    match update_session(
        &req.id,
        req.title.as_deref(),
        req.model.as_deref(),
        req.system_prompt.as_deref(),
    ) {
        Ok(Some(data)) => write_success(json_out, "success", data),
        Ok(None) => write_error(json_out, 404, "session not found"),
        Err(e) => write_error(
            json_out,
            400310,
            &format!("sessions update failed: {:#}", e),
        ),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn post_sessions_patch(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(s) => s,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: SessionsPatchReq = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };
    if req.id.trim().is_empty() {
        return write_error(json_out, 400103, "id is required");
    }

    match patch_session(
        &req.id,
        req.title.as_deref(),
        req.model.as_deref(),
        req.system_prompt.as_deref(),
    ) {
        Ok(Some(data)) => write_success(json_out, "success", data),
        Ok(None) => write_error(json_out, 404, "session not found"),
        Err(e) => write_error(json_out, 400310, &format!("sessions patch failed: {:#}", e)),
    }
}
