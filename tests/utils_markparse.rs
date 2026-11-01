// tests/utils_markparse.rs —— utils::markparse::extract_html 的响应解析测试
use chatllm::utils::markparse::extract_html;

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
