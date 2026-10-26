//! 素材内容向量与检索 FFI。
//!
//!   POST /chatllm + FFI-Symbol: material_embed  → post_material_embed
//!   POST /chatllm + FFI-Symbol: material_search → post_material_search

use crate::utils::cjson::{write_error, write_success};
use crate::utils::material_embed::{self, DeleteReq, EmbedReq, SearchReq};
use std::ffi::{c_char, c_int, CStr};

fn read_body<'a>(json_in: *const c_char) -> Result<&'a str, c_int> {
    if json_in.is_null() {
        return Err(400100);
    }
    unsafe { CStr::from_ptr(json_in).to_str().map_err(|_| 400101) }
}

fn error_code(message: &str) -> c_int {
    if message.contains("模型")
        || message.contains("向量")
        || message.contains("embedding")
        || message.contains("GGUF")
    {
        400502
    } else if message.contains("database") || message.contains("SQL") {
        400504
    } else {
        400500
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn post_material_embed(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(body) => body,
        Err(code) => return write_error(json_out, code, "json_in is null or invalid utf-8"),
    };

    let req: EmbedReq = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(error) => {
            return write_error(json_out, 400500, &format!("invalid json: {error}"));
        }
    };

    match material_embed::embed(req) {
        Ok(data) => write_success(json_out, "success", data),
        Err(error) => {
            let message = format!("{error:#}");
            write_error(json_out, error_code(&message), &message)
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn post_material_search(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(body) => body,
        Err(code) => return write_error(json_out, code, "json_in is null or invalid utf-8"),
    };

    let req: SearchReq = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(error) => {
            return write_error(json_out, 400500, &format!("invalid json: {error}"));
        }
    };

    match material_embed::search(req) {
        Ok(data) => write_success(json_out, "success", data),
        Err(error) => {
            let message = format!("{error:#}");
            write_error(json_out, error_code(&message), &message)
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn post_material_delete(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(body) => body,
        Err(code) => return write_error(json_out, code, "json_in is null or invalid utf-8"),
    };

    let req: DeleteReq = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(error) => {
            return write_error(json_out, 400500, &format!("invalid json: {error}"));
        }
    };

    match material_embed::delete(req) {
        Ok(data) => write_success(json_out, "success", data),
        Err(error) => {
            let message = format!("{error:#}");
            write_error(json_out, error_code(&message), &message)
        }
    }
}
