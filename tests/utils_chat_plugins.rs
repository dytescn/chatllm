// tests/utils_chat_plugins.rs —— utils::chat_plugins 的阶段流程与响应处理测试
use chatllm::utils::chat_plugins::{
    content_of, input_at, join_results, plugin_of, plugin_request, plugin_uuid_of, run,
    stage_status, symbol_of, ChatPluginCall, ChatPluginsOutcome, COLLECTION, TOP_K,
};
use chatllm::utils::plugins_callback::PluginCallGuard;
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
fn joins_multiple_results_and_skips_empty() {
    let results = vec!["first".to_string(), "   ".to_string(), "second".to_string()];
    assert_eq!(join_results(&results).as_deref(), Some("first\n\nsecond"));
    assert_eq!(join_results(&[]), None);
    assert_eq!(join_results(&["  ".to_string()]), None);
}

#[test]
fn symbol_of_falls_back_to_chat() {
    assert_eq!(symbol_of(&json!({})), "chat");
    assert_eq!(symbol_of(&json!({ "ffi_symbol": "do_test" })), "do_test");
}

#[test]
fn plugin_collection_is_chat_plugins() {
    assert_eq!(COLLECTION, "chat_plugins");
}

#[test]
fn reads_plugin_uuid_from_meta() {
    let hit = json!({
        "meta": { "plugin_uuid": "p1", "plugin": "do_test_chat" },
        "value": "{}"
    });
    assert_eq!(plugin_uuid_of(&hit).as_deref(), Some("p1"));
}

#[test]
fn reads_plugin_uuid_from_value_json() {
    let hit = json!({
        "value": "{\"plugin_uuid\":\"p1\",\"lib_name\":\"do_test_chat\"}"
    });
    assert_eq!(plugin_uuid_of(&hit).as_deref(), Some("p1"));

    let hit = json!({
        "value": "{\"uuid\":\"p2\",\"lib_name\":\"do_test_chat\"}"
    });
    assert_eq!(plugin_uuid_of(&hit).as_deref(), Some("p2"));
}

#[test]
fn ignores_blank_plugin_uuid() {
    assert_eq!(plugin_uuid_of(&json!({ "plugin_uuid": "  " })), None);
    assert_eq!(plugin_uuid_of(&json!({ "value": "not-json" })), None);
    assert_eq!(plugin_uuid_of(&json!({})), None);
}

#[test]
fn input_at_reuses_last_and_handles_empty() {
    let inputs = vec!["a".to_string(), "b".to_string()];
    assert_eq!(input_at(&inputs, 0), "a");
    assert_eq!(input_at(&inputs, 1), "b");
    assert_eq!(input_at(&inputs, 5), "b");
    assert_eq!(input_at(&[], 0), "");
}

#[test]
fn plugin_request_carries_prompt_input_and_step() {
    let prompts = vec!["关键词".to_string()];
    let inputs = vec!["文本".to_string()];
    let request = plugin_request("关键词", "文本", &prompts, &inputs, 0);
    assert_eq!(request["prompt"], "文本");
    assert_eq!(request["keyword"], "关键词");
    assert_eq!(request["prompts"], json!(["关键词"]));
    assert_eq!(request["inputs"], json!(["文本"]));
    assert_eq!(request["step"], json!(1));
}

#[test]
fn stage_status_aggregates_calls() {
    let ok = ChatPluginCall {
        status: "ok",
        ..ChatPluginCall::default()
    };
    let skipped = ChatPluginCall {
        status: "skipped",
        ..ChatPluginCall::default()
    };
    let error = ChatPluginCall {
        status: "error",
        ..ChatPluginCall::default()
    };

    assert_eq!(stage_status(&[]), "skipped");
    assert_eq!(stage_status(&[skipped.clone()]), "skipped");
    assert_eq!(stage_status(&[ok.clone(), skipped.clone()]), "ok");
    assert_eq!(stage_status(&[ok, error]), "error");
}

#[test]
fn outcome_json_contains_calls_and_arrays() {
    let outcome = ChatPluginsOutcome {
        start_time: 10,
        end_time: 20,
        status: "ok",
        inputs: vec!["文本".to_string()],
        outputs: vec!["结果".to_string()],
        calls: vec![ChatPluginCall {
            step: 1,
            prompt: "关键词".to_string(),
            input: "文本".to_string(),
            start_time: 10,
            end_time: 20,
            status: "ok",
            response: "结果".to_string(),
            ..ChatPluginCall::default()
        }],
    };
    let value = outcome.to_json();
    assert_eq!(value["start_time"], 10);
    assert!(value.get("prompts").is_none(), "返回不应再包含 prompts");
    assert_eq!(value["outputs"], json!(["结果"]));
    assert_eq!(value["calls"][0]["step"], 1);
    assert_eq!(value["calls"][0]["params"]["keyword"], "关键词");
    assert_eq!(value["status"], "ok");
}

#[test]
fn rejects_reentry_before_kv_lookup() {
    let _outer = PluginCallGuard::enter().expect("首层调用应成功");
    let outcome = run(&["测试".to_string()], &["输入".to_string()], 0);
    assert_eq!(outcome.status, "error");
    assert!(outcome.calls.is_empty(), "重入时不应发起检索");
}

#[test]
fn res_maps_outputs_to_contents() {
    let outcome = ChatPluginsOutcome {
        outputs: vec!["a".to_string()],
        ..Default::default()
    };
    assert_eq!(outcome.res().contents, vec!["a"]);
}
