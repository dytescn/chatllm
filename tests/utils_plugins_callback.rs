// tests/utils_plugins_callback.rs —— utils::plugins_callback 的回调与响应处理测试
use chatllm::utils::plugins_callback::{
    candidates, check_plugin_ret, content_of_response, ffi_entry, parse_plugin_response,
    plugin_call_active, PluginCallGuard,
};
use serde_json::json;

#[test]
fn plugin_call_is_inactive_outside_invocation() {
    assert!(!plugin_call_active());
}

#[test]
fn rejects_nested_plugin_call() {
    let outer = PluginCallGuard::enter().expect("首层调用应成功");
    assert!(plugin_call_active());
    assert!(PluginCallGuard::enter().is_err(), "同线程重入应被拒绝");
    drop(outer);
    assert!(!plugin_call_active());
}

#[test]
fn guard_recovers_after_reentry_error() {
    {
        let _outer = PluginCallGuard::enter().expect("首层调用应成功");
        assert!(PluginCallGuard::enter().is_err());
    }
    // 外层释放后应恢复可调用状态。
    let _again = PluginCallGuard::enter().expect("外层释放后应可再次进入");
}

#[test]
fn plugin_call_active_is_thread_local() {
    let _outer = PluginCallGuard::enter().expect("首层调用应成功");
    let active = std::thread::spawn(plugin_call_active).join().unwrap();
    assert!(!active, "重入标记不应影响其他线程");
}

#[test]
fn content_of_response_extracts_string() {
    assert_eq!(
        content_of_response(&json!({ "data": { "content": "hello" } })),
        "hello"
    );
    assert_eq!(
        content_of_response(&json!({ "data": { "content": "  " } })),
        "  "
    );
}

#[test]
fn content_of_response_defaults_empty() {
    assert_eq!(content_of_response(&json!({})), "");
    assert_eq!(content_of_response(&json!({ "data": {} })), "");
    assert_eq!(
        content_of_response(&json!({ "data": { "content": 1 } })),
        ""
    );
}

#[test]
fn ffi_entry_adds_post_prefix() {
    assert_eq!(ffi_entry("chat"), "post_chat");
    assert_eq!(ffi_entry("do_test"), "post_do_test");
}

#[test]
fn check_plugin_ret_maps_codes() {
    assert!(check_plugin_ret("demo", 0).is_ok());

    let err = check_plugin_ret("demo", -2).unwrap_err().to_string();
    assert!(err.contains("64 KiB"));

    let err = check_plugin_ret("demo", 7).unwrap_err().to_string();
    assert!(err.contains("FFI 返回 7"));
}

#[test]
fn parse_plugin_response_accepts_200_and_0() {
    let value = parse_plugin_response("demo", r#"{"code":200,"data":{"content":"ok"}}"#).unwrap();
    assert_eq!(value["data"]["content"], "ok");

    let value = parse_plugin_response("demo", r#"{"code":0,"data":{}}"#).unwrap();
    assert_eq!(value["code"], 0);
}

#[test]
fn parse_plugin_response_rejects_error_code() {
    let err = parse_plugin_response("demo", r#"{"code":500,"msg":"boom"}"#)
        .unwrap_err()
        .to_string();
    assert!(err.contains("500"));
    assert!(err.contains("boom"));
}

#[test]
fn parse_plugin_response_rejects_bad_json() {
    assert!(parse_plugin_response("demo", "not json").is_err());
}

#[test]
fn candidates_follow_install_layout() {
    let ext = std::env::consts::DLL_EXTENSION;
    let paths = candidates("demo");
    let text: Vec<String> = paths
        .iter()
        .map(|path| path.display().to_string().replace('\\', "/"))
        .collect();
    assert_eq!(text.len(), 4);
    assert!(text[0].ends_with(&format!("plugins/demo/demo.{ext}")));
    assert!(text[1].ends_with(&format!("plugins/demo/libdemo.{ext}")));
    assert!(text[2].ends_with(&format!("libary/demo.{ext}")));
    assert!(text[3].ends_with(&format!("libary/libdemo.{ext}")));
}
