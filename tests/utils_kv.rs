// tests/utils_kv.rs —— utils::kv::parse_response 的响应解析测试
use chatllm::utils::kv::parse_response;

#[test]
fn parse_response_success_returns_data() {
    let out = r#"{"code":200,"data":{"list":[{"key":"plugin-cdr-check-txt","score":0.94}]},"msg":"success"}"#;
    let data = parse_response("search", out).unwrap();
    assert_eq!(data["list"][0]["key"], "plugin-cdr-check-txt");
}

#[test]
fn parse_response_error_code() {
    let out = r#"{"code":400600,"data":{},"msg":"collection not found"}"#;
    let err = parse_response("search", out).unwrap_err().to_string();
    assert!(err.contains("400600"));
    assert!(err.contains("collection not found"));
}

#[test]
fn parse_response_bad_json() {
    assert!(parse_response("search", "not json").is_err());
}

#[test]
fn parse_response_empty() {
    assert!(parse_response("search", "").is_err());
}
