// tests/utils_plugins.rs —— utils::plugins 的命中记录解析测试
use chatllm::utils::plugins::{content_of, join_results, plugin_of, TOP_K};
use serde_json::json;

#[test]
fn reads_plugin_from_meta() {
    let hit = json!({
        "value": "{\"lib_name\":\"ignored\"}",
        "meta": { "plugin": "do_test_chat", "ffi_symbol": "chat" }
    });
    assert_eq!(
        plugin_of(&hit),
        Some(("do_test_chat".to_string(), "chat".to_string()))
    );
}

#[test]
fn reads_plugin_from_value_json() {
    let hit = json!({
        "value": "{\"lib_name\":\"do_test_chat\",\"ffi_symbol\":\"chat\"}"
    });
    assert_eq!(
        plugin_of(&hit),
        Some(("do_test_chat".to_string(), "chat".to_string()))
    );
}

#[test]
fn defaults_symbol_to_chat() {
    let hit = json!({ "meta": { "plugin": "do_test_chat" } });
    assert_eq!(
        plugin_of(&hit),
        Some(("do_test_chat".to_string(), "chat".to_string()))
    );

    let hit = json!({ "value": "{\"lib_name\":\"do_test_chat\"}" });
    assert_eq!(
        plugin_of(&hit),
        Some(("do_test_chat".to_string(), "chat".to_string()))
    );
}

#[test]
fn returns_none_without_plugin_metadata() {
    assert_eq!(plugin_of(&json!({})), None);
    assert_eq!(plugin_of(&json!({ "meta": {} })), None);
    assert_eq!(plugin_of(&json!({ "value": "not-json" })), None);
    assert_eq!(plugin_of(&json!({ "value": "{\"name\":\"x\"}" })), None);
}

#[test]
fn trims_content_and_filters_empty() {
    assert_eq!(
        content_of(&json!({ "value": "  hello  " })),
        Some("hello".to_string())
    );
    assert_eq!(content_of(&json!({ "value": "   " })), None);
    assert_eq!(content_of(&json!({})), None);
}

#[test]
fn searches_five_chat_plugins_by_default() {
    assert_eq!(TOP_K, 5);
}

#[test]
fn plugin_call_is_inactive_outside_invocation() {
    assert!(!chatllm::utils::plugins::plugin_call_active());
}

#[test]
fn joins_multiple_results_and_skips_empty() {
    let results = vec!["first".to_string(), "   ".to_string(), "second".to_string()];
    assert_eq!(join_results(&results).as_deref(), Some("first\n\nsecond"));
    assert_eq!(join_results(&[]), None);
    assert_eq!(join_results(&["  ".to_string()]), None);
}
