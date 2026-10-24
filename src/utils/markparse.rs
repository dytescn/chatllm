// src/utils/markparse.rs
// Markdown 转译：dlopen ./libary/parse.dll，调用 markparse 的 post_markdown_to_html。
use anyhow::{Context, Result};
use libloading::Library;
use serde_json::Value;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::OnceLock;

/// markparse 在 libary/ 下的文件名（由 markparse/dytes.json 的 exports 决定）
#[cfg(windows)]
const MARKPARSE_PATH: &str = "./libary/parse.dll";
#[cfg(not(windows))]
const MARKPARSE_PATH: &str = "./libary/parse.so";

const OUT_BUF_SIZE: usize = 65536;

type ToHtmlFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

static MARKPARSE_LIB: OnceLock<Library> = OnceLock::new();
static TO_HTML_FN: OnceLock<ToHtmlFn> = OnceLock::new();

fn load_to_html() -> Result<ToHtmlFn> {
    let lib = unsafe { Library::new(MARKPARSE_PATH) }
        .with_context(|| format!("加载 {} 失败", MARKPARSE_PATH))?;
    let f = unsafe {
        *lib.get::<ToHtmlFn>(b"post_markdown_to_html")
            .context("dlsym post_markdown_to_html 失败")?
    };
    let _ = MARKPARSE_LIB.set(lib);
    eprintln!("[markparse] 加载 {} 成功", MARKPARSE_PATH);
    Ok(f)
}

fn to_html_fn() -> Result<ToHtmlFn> {
    if let Some(f) = TO_HTML_FN.get() {
        return Ok(*f);
    }
    let f = load_to_html()?;
    let _ = TO_HTML_FN.set(f);
    Ok(f)
}

/// 解析 markparse 的 JSON 响应，取出 data.html。
/// 纯函数，便于单测。
pub fn extract_html(out: &str) -> Result<String> {
    let value: Value =
        serde_json::from_str(out).with_context(|| format!("markparse 响应解析失败: {}", out))?;

    let code = value.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code != 200 && code != 0 {
        let msg = value
            .get("msg")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown");
        anyhow::bail!("markparse 错误 (code={}): {}", code, msg);
    }

    Ok(value
        .get("data")
        .and_then(|d| d.get("html"))
        .and_then(|h| h.as_str())
        .unwrap_or("")
        .to_string())
}

/// 把 Markdown 原文转成 HTML。
pub fn to_html(markdown: &str) -> Result<String> {
    let f = to_html_fn()?;

    let req = serde_json::json!({ "source": markdown });
    let req_c = CString::new(req.to_string()).context("请求 JSON 含 \0")?;

    let mut out_buf = vec![0u8; OUT_BUF_SIZE];
    let ret = unsafe { f(req_c.as_ptr(), out_buf.as_mut_ptr() as *mut c_char) };
    if ret != 0 {
        anyhow::bail!("markparse FFI 返回 {}", ret);
    }

    let out_str = unsafe { CStr::from_ptr(out_buf.as_ptr() as *const c_char) }
        .to_str()
        .context("markparse 响应非 UTF-8")?;

    if out_str.is_empty() {
        anyhow::bail!("markparse 返回空（ret={}）", ret);
    }

    extract_html(out_str)
}

#[cfg(test)]
mod tests {
    use super::extract_html;

    #[test]
    fn extract_html_success() {
        let out = r#"{"code":200,"data":{"html":"<h1>hi</h1>"},"msg":"success"}"#;
        assert_eq!(extract_html(out).unwrap(), "<h1>hi</h1>");
    }

    #[test]
    fn extract_html_error_code() {
        let out = r#"{"code":400201,"data":{},"msg":"expected value"}"#;
        let err = extract_html(out).unwrap_err().to_string();
        assert!(err.contains("400201"));
    }

    #[test]
    fn extract_html_bad_json() {
        assert!(extract_html("not json").is_err());
    }
}
