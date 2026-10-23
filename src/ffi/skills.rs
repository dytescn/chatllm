use crate::utils::cjson::{write_error, write_success};
use crate::utils::skills::{create_skill, get_skill, list_skills, patch_skill, update_skill};
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
struct SkillsListReq {
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SkillsGetReq {
    id: Option<i64>,
    uid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SkillsCreateReq {
    uid: Option<String>,
    content: String,
    version: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SkillsUpdateReq {
    id: Option<i64>,
    uid: Option<String>,
    content: String,
    version: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SkillsPatchReq {
    id: Option<i64>,
    uid: Option<String>,
    content: Option<String>,
    version: Option<i64>,
}

#[unsafe(no_mangle)]
pub extern "C" fn post_skills_list(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = if json_in.is_null() {
        ""
    } else {
        match read_body(json_in) {
            Ok(s) => s,
            Err(code) => return write_error(json_out, code, err_msg(code)),
        }
    };

    let req: SkillsListReq = if body.trim().is_empty() {
        SkillsListReq {
            page: None,
            page_size: None,
        }
    } else {
        match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
        }
    };

    match list_skills(req.page, req.page_size) {
        Ok(data) => write_success(json_out, "success", data),
        Err(e) => write_error(json_out, 400300, &format!("skills list failed: {:#}", e)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn post_skills_get(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(s) => s,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: SkillsGetReq = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };

    match get_skill(req.id, req.uid.as_deref()) {
        Ok(Some(data)) => write_success(json_out, "success", data),
        Ok(None) => write_error(json_out, 404, "skill not found"),
        Err(e) => write_error(json_out, 400300, &format!("skills get failed: {:#}", e)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn post_skills_post(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(s) => s,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: SkillsCreateReq = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };
    if req.content.trim().is_empty() {
        return write_error(json_out, 400103, "content is required");
    }

    match create_skill(req.uid.as_deref(), &req.content, req.version) {
        Ok(data) => write_success(json_out, "success", data),
        Err(e) => write_error(json_out, 400300, &format!("skills create failed: {:#}", e)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn post_skills_update(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(s) => s,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: SkillsUpdateReq = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };
    if req.id.is_none() && req.uid.as_deref().map(str::is_empty).unwrap_or(true) {
        return write_error(json_out, 400104, "id or uid is required");
    }
    if req.content.trim().is_empty() {
        return write_error(json_out, 400103, "content is required");
    }

    match update_skill(req.id, req.uid.as_deref(), &req.content, req.version) {
        Ok(Some(data)) => write_success(json_out, "success", data),
        Ok(None) => write_error(json_out, 404, "skill not found"),
        Err(e) => write_error(json_out, 400300, &format!("skills update failed: {:#}", e)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn post_skills_patch(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(s) => s,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: SkillsPatchReq = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };
    if req.id.is_none() && req.uid.as_deref().map(str::is_empty).unwrap_or(true) {
        return write_error(json_out, 400104, "id or uid is required");
    }

    match patch_skill(
        req.id,
        req.uid.as_deref(),
        req.content.as_deref(),
        req.version,
    ) {
        Ok(Some(data)) => write_success(json_out, "success", data),
        Ok(None) => write_error(json_out, 404, "skill not found"),
        Err(e) => write_error(json_out, 400300, &format!("skills patch failed: {:#}", e)),
    }
}
