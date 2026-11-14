// tests/utils_kv.rs —— utils::kv 的响应解析与请求构造测试
use chatllm::utils::kv::{
    build_search_request, check_ret, clamp_top_k, extract_list, ffi_symbol, is_error_code,
    parse_response, preview, take_list, KvError, COLLECTION_CHAT, COLLECTION_CHAT_PLUGINS,
    COLLECTION_PLUGIN, COLLECTION_PROMPT,
};
use serde_json::{json, Value};

#[test]
fn ffi_symbol_uses_kv_prefix() {
    assert_eq!(ffi_symbol("search"), "kv_search");
    assert_eq!(ffi_symbol("collection"), "kv_collection");
    assert_eq!(ffi_symbol("embed"), "kv_embed");
}

#[test]
fn parse_response_success_returns_data() {
    let out = r#"{"code":200,"data":{"list":[{"key":"plugin-cdr-check-txt","score":0.94}]},"msg":"success"}"#;
    let data = parse_response("search", out).unwrap();
    assert_eq!(data["list"][0]["key"], "plugin-cdr-check-txt");
}

#[test]
fn parse_response_error_code() {
    let out = r#"{"code":400600,"data":{},"msg":"collection not found"}"#;
    let err = parse_response("search", out).unwrap_err();
    assert!(err.to_string().contains("400600"));
    assert!(err.to_string().contains("collection not found"));
}

#[test]
fn parse_response_error_carries_kv_error() {
    let out = r#"{"code":400600,"data":{},"msg":"unknown collection: chat_plugins"}"#;
    let err = parse_response("search", out).unwrap_err();

    let kv_error = err.downcast_ref::<KvError>().expect("应保留结构化错误");
    assert_eq!(kv_error.symbol, "search");
    assert_eq!(kv_error.code, 400600);
    assert_eq!(kv_error.msg, "unknown collection: chat_plugins");

    assert!(is_error_code(&err, 400600));
    assert!(!is_error_code(&err, 400601));
}

#[test]
fn parse_response_missing_code_is_error() {
    let out = r#"{"data":{"list":[]},"msg":"success"}"#;
    let err = parse_response("search", out).unwrap_err();
    assert!(err.to_string().contains("code=-1"));
}

#[test]
fn parse_response_bad_json() {
    assert!(parse_response("search", "not json").is_err());
}

#[test]
fn parse_response_empty() {
    assert!(parse_response("search", "").is_err());
}

#[test]
fn extract_list_returns_array() {
    let data = json!({"list":[{"key":"a"},{"key":"b"}],"total":2});
    let list = extract_list(&data);
    assert_eq!(list.len(), 2);
    assert_eq!(list[1]["key"], "b");
}

#[test]
fn extract_list_tolerates_missing_or_invalid() {
    assert!(extract_list(&json!({})).is_empty());
    assert!(extract_list(&json!({"list":null})).is_empty());
    assert!(extract_list(&json!({"list":{}})).is_empty());
}

#[test]
fn clamp_top_k_bounds() {
    assert_eq!(clamp_top_k(-5), 1);
    assert_eq!(clamp_top_k(0), 1);
    assert_eq!(clamp_top_k(5), 5);
    assert_eq!(clamp_top_k(1000), 100);
}

#[test]
fn build_search_request_clamps_and_keeps_optional_fields() {
    let keys = vec!["a".to_string(), "b".to_string()];
    let request = build_search_request("plugin", "查询", 500, 0.4, Some(&keys), true);

    assert_eq!(request["collection"], "plugin");
    assert_eq!(request["query"], "查询");
    assert_eq!(request["top_k"], json!(100));
    assert_eq!(request["min_score"], 0.4);
    assert_eq!(request["candidate_keys"], json!(["a", "b"]));
    assert_eq!(request["with_value"], true);
}

#[test]
fn build_search_request_serializes_missing_candidates_as_null() {
    let request = build_search_request("chat", "q", 3, 0.0, None, false);
    assert!(request["candidate_keys"].is_null());
    assert_eq!(request["top_k"], json!(3));
}

#[test]
fn take_list_moves_array() {
    let data = json!({"list":[{"key":"a"}],"total":1});
    let list = take_list(data);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["key"], "a");
}

#[test]
fn take_list_tolerates_other_shapes() {
    assert!(take_list(json!({"list": {}})).is_empty());
    assert!(take_list(json!({"list": null})).is_empty());
    assert!(take_list(json!({})).is_empty());
    assert!(take_list(Value::Null).is_empty());
}

#[test]
fn check_ret_maps_return_codes() {
    assert!(check_ret("kv_search", 0, 1024 * 1024).is_ok());

    let err = check_ret("kv_search", -1, 1024 * 1024)
        .unwrap_err()
        .to_string();
    assert!(err.contains("为空指针"));

    let err = check_ret("kv_search", -2, 1024 * 1024)
        .unwrap_err()
        .to_string();
    assert!(err.contains("1024 KiB"));

    let err = check_ret("kv_search", 7, 64).unwrap_err().to_string();
    assert!(err.contains("FFI 返回 7"));
}

#[test]
fn preview_truncates_by_char() {
    assert_eq!(preview("abc", 5), "abc");
    assert_eq!(preview("abcdef", 3), "abc...");
    assert_eq!(preview("中文内容", 2), "中文...");
}

#[test]
fn collection_constants_are_stable() {
    assert_eq!(COLLECTION_CHAT, "chat");
    assert_eq!(COLLECTION_PLUGIN, "plugin");
    assert_eq!(COLLECTION_PROMPT, "prompt");
    assert_eq!(COLLECTION_CHAT_PLUGINS, "chat_plugins");
}
