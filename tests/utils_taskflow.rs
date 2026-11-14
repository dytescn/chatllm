// tests/utils_taskflow.rs —— utils::taskflow 的元素过滤与回调解析
use chatllm::utils::taskflow::{callback_of, filter_for_plugin, plugin_uuid_of};
use serde_json::json;

#[test]
fn filters_elements_by_plugin_uuid() {
    let items = vec![
        json!({ "uid": "a", "plugin_uuid": "p1" }),
        json!({ "uid": "b", "plugin_uuid": "p2" }),
        json!({ "uid": "c" }),
    ];
    let matched = filter_for_plugin(&items, "p1");
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0]["uid"], "a");
    assert!(filter_for_plugin(&items, "  ").is_empty());
    assert!(filter_for_plugin(&items, "missing").is_empty());
}

#[test]
fn reads_callback_in_priority_order() {
    assert_eq!(
        callback_of(&json!({ "callback": "run", "ffi_symbol": "chat" })).as_deref(),
        Some("run")
    );
    assert_eq!(
        callback_of(&json!({ "ffi_symbol": "chat", "method": "m" })).as_deref(),
        Some("chat")
    );
    assert_eq!(
        callback_of(&json!({ "method": "m", "uid": "u" })).as_deref(),
        Some("m")
    );
    assert_eq!(callback_of(&json!({ "uid": "u" })).as_deref(), Some("u"));
    assert_eq!(callback_of(&json!({ "callback": "  " })), None);
    assert_eq!(callback_of(&json!({})), None);
}

#[test]
fn reads_plugin_uuid_with_fallback() {
    assert_eq!(
        plugin_uuid_of(&json!({ "plugin_uuid": "p1", "uuid": "u1" })).as_deref(),
        Some("p1")
    );
    assert_eq!(
        plugin_uuid_of(&json!({ "uuid": "u1" })).as_deref(),
        Some("u1")
    );
    assert_eq!(plugin_uuid_of(&json!({ "uuid": "  " })), None);
}
