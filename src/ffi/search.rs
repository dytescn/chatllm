use crate::ffi::types::TurboSearchReq;
use crate::utils::cjson::{write_error, write_success};
use crate::utils::search::search_query;
use crate::utils::threading::run_with_large_stack;
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

#[unsafe(no_mangle)]
pub extern "C" fn post_turbo_search_query(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(s) => s,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };

    let req: TurboSearchReq = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };

    let query = req.query;
    let top_k = req.top_k.unwrap_or(0);
    match run_with_large_stack(move || search_query(&query, top_k)) {
        Ok(data) => write_success(json_out, "success", data),
        Err(e) => write_error(json_out, 400500, &format!("turbo search failed: {:#}", e)),
    }
}
