use crate::ffi::run::run_inference_capture;
use crate::ffi::types::ChatReq;
use crate::utils::cjson::{write_error, write_success};
use serde_json::json;
use std::ffi::{c_char, c_int, CStr};

#[unsafe(no_mangle)]
pub extern "C" fn post_chatllm(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    if json_in.is_null() {
        return write_error(json_out, 400100, "json_in is null");
    }
    let body = unsafe {
        match CStr::from_ptr(json_in).to_str() {
            Ok(s) => s,
            Err(_) => return write_error(json_out, 400101, "invalid utf-8"),
        }
    };
    let req: ChatReq = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };
    let (output_text, prompt_tokens, gen_tokens) = match run_inference_capture(&req.prompt) {
        Ok(res) => res,
        Err(e) => return write_error(json_out, 400200, &format!("llm infer failed: {}", e)),
    };
    write_success(
        json_out,
        "success",
        json!({
            "input": req.prompt,
            "output": output_text,
            "prompt_tokens": prompt_tokens,
            "gen_tokens": gen_tokens
        }),
    )
}
