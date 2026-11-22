// tests/utils_think.rs —— utils::think 的解析规则测试
use chatllm::utils::think::{
    apply_plugin_response, parse_hit, plugin_request, ThinkHit, COLLECTION, INIT_KEYWORD,
    INIT_MIN_SCORE, TOP_K,
};

#[test]
fn think_constants_are_stable() {
    assert_eq!(INIT_KEYWORD, "base");
    assert_eq!(INIT_MIN_SCORE, 1.0);
    assert_eq!(COLLECTION, "chat_think");
    assert_eq!(TOP_K, 5);
}

#[test]
fn parses_plugins_uid_from_meta() {
    let hit = serde_json::json!({
        "key": "think-base",
        "value": "系统信息助手",
        "meta": {
            "plugins_uid": "9a7f3c2e-5b1d-4e8a-9c6f-2d4b8a1e7c30"
        }
    });
    let parsed = parse_hit(&hit);
    assert_eq!(parsed.value.as_deref(), Some("系统信息助手"));
    assert_eq!(
        parsed.plugins_uid.as_deref(),
        Some("9a7f3c2e-5b1d-4e8a-9c6f-2d4b8a1e7c30")
    );
}

#[test]
fn parses_plugins_uid_from_value_json() {
    let hit = serde_json::json!({
        "key": "think-base",
        "value": "{\"plugins_uid\":\"uuid\"}"
    });
    let parsed = parse_hit(&hit);
    assert_eq!(parsed.plugins_uid.as_deref(), Some("uuid"));
}

#[test]
fn parses_prompts_and_outputs() {
    let hit = serde_json::json!({
        "value": "问候",
        "meta": { "prompts": ["问候", "系统信息"] }
    });
    let parsed = parse_hit(&hit);
    assert_eq!(parsed.prompts, vec!["问候", "系统信息"]);
    assert_eq!(parsed.outputs, vec!["问候"]);
}

#[test]
fn plugin_request_carries_text_and_empty_arrays() {
    let request = plugin_request("用户输入");
    assert_eq!(request["prompt"], "用户输入");
    assert_eq!(request["prompts"], serde_json::json!([]));
    assert_eq!(request["outputs"], serde_json::json!([]));
}

#[test]
fn apply_plugin_response_prefers_prompts_and_outputs() {
    let mut hit = ThinkHit {
        value: Some("命中值".to_string()),
        prompts: vec!["旧提示".to_string()],
        outputs: vec!["旧输出".to_string()],
        ..ThinkHit::default()
    };
    let response = serde_json::json!({
        "code": 200,
        "data": {
            "prompts": ["问候", "系统信息"],
            "outputs": ["结果一", "结果二"]
        }
    });

    apply_plugin_response(&mut hit, &response);
    assert_eq!(hit.prompts, vec!["问候", "系统信息"]);
    assert_eq!(hit.outputs, vec!["结果一", "结果二"]);
    assert_eq!(hit.value.as_deref(), Some("结果一"));
}

#[test]
fn apply_plugin_response_falls_back_to_content() {
    let mut hit = ThinkHit::default();
    let response = serde_json::json!({
        "code": 200,
        "data": { "content": "  单条输出  " }
    });

    apply_plugin_response(&mut hit, &response);
    assert!(hit.prompts.is_empty());
    assert_eq!(hit.outputs, vec!["单条输出"]);
    assert_eq!(hit.value.as_deref(), Some("单条输出"));
}

#[test]
fn apply_plugin_response_keeps_existing_on_empty() {
    let mut hit = ThinkHit {
        prompts: vec!["提示".to_string()],
        outputs: vec!["输出".to_string()],
        value: Some("输出".to_string()),
        ..ThinkHit::default()
    };
    apply_plugin_response(&mut hit, &serde_json::json!({ "code": 200, "data": {} }));
    assert_eq!(hit.prompts, vec!["提示"]);
    assert_eq!(hit.outputs, vec!["输出"]);
}
