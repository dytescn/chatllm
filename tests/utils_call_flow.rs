// tests/utils_call_flow.rs —— post_call_chat 的请求结构与前置校验测试
use chatllm::ffi::types::CallChatReq;
use chatllm::utils::call_flow;
use serde_json::json;

fn parse(body: &str) -> CallChatReq {
    serde_json::from_str(body).expect("请求应能解析")
}

#[test]
fn parses_full_request() {
    let req = parse(
        r#"{
            "plugins_uid": "6f2b8d41-9c3a-4e7b-8d5f-1a2c3e4b5d60",
            "category": "chat_plugin",
            "prompts": "整理会话",
            "context": { "messages": [] }
        }"#,
    );
    assert_eq!(
        req.plugins_uid.as_deref(),
        Some("6f2b8d41-9c3a-4e7b-8d5f-1a2c3e4b5d60")
    );
    assert_eq!(req.category, "chat_plugin");
    assert_eq!(req.prompts, "整理会话");
    assert_eq!(req.context, json!({ "messages": [] }));
}

#[test]
fn defaults_optional_fields() {
    let req = parse(r#"{"category":"chat_plugin"}"#);
    assert_eq!(req.plugins_uid, None);
    assert_eq!(req.prompts, "");
    assert_eq!(req.context, json!(null));
}

#[test]
fn requires_category() {
    let error = serde_json::from_str::<CallChatReq>(r#"{"prompts":"x"}"#)
        .expect_err("缺少 category 应解析失败");
    assert!(error.to_string().contains("category"));
}

#[test]
fn accepts_any_context_json() {
    for context in [json!("text"), json!([1, 2]), json!(42), json!(true)] {
        let body = json!({ "category": "chat_plugin", "context": context });
        let req = parse(&body.to_string());
        assert_eq!(req.context, context);
    }
}

#[test]
fn ignores_unknown_fields() {
    let req = parse(r#"{"category":"chat_plugin","unknown":1}"#);
    assert_eq!(req.category, "chat_plugin");
}

#[test]
fn run_rejects_empty_category() {
    let req = parse(r#"{"category":"   "}"#);
    let error = call_flow::run(&req).expect_err("空分类应失败");
    assert_eq!(error.code(), 400304);
    assert!(error.message().contains("category"));
}
