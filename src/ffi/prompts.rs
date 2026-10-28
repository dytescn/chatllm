use crate::utils::cjson::{write_error, write_success};
use crate::utils::prompts::embed_prompts;
use crate::utils::threading::run_with_large_stack;
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
struct EmbedPromptsReq {
    limit: Option<usize>,
    offset: Option<usize>,
}

#[unsafe(no_mangle)]
pub extern "C" fn post_embed_prompts(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = if json_in.is_null() {
        ""
    } else {
        match read_body(json_in) {
            Ok(s) => s,
            Err(code) => return write_error(json_out, code, err_msg(code)),
        }
    };

    let req: EmbedPromptsReq = if body.trim().is_empty() {
        EmbedPromptsReq {
            limit: None,
            offset: None,
        }
    } else {
        match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
        }
    };

    let limit = req.limit;
    let offset = req.offset;
    match run_with_large_stack(move || embed_prompts(limit, offset)) {
        Ok(data) => write_success(json_out, "success", data),
        Err(e) => write_error(json_out, 400500, &format!("embed prompts failed: {:#}", e)),
    }
}
