// tests/base_call_chat.rs —— post_call_chat 的冒烟测试
//
// 依赖 testenv 下的 kv_lib 与 do_test_chat 插件；缺失时自动跳过。
mod common;

use chatllm::ffi::call_back::post_call_chat;
use chatllm::utils::plugins;
use serde_json::json;

#[test]
fn invalid_json_returns_400102() {
    let res = common::call(post_call_chat, "{not-json");
    assert_eq!(res["code"], 400102);
}

#[test]
fn missing_category_returns_400102() {
    let res = common::call(post_call_chat, r#"{"prompts":"测试"}"#);
    assert_eq!(res["code"], 400102);
}

#[test]
fn empty_category_returns_400304() {
    let res = common::call(post_call_chat, r#"{"category":"   "}"#);
    assert_eq!(res["code"], 400304);
    assert!(res["msg"]
        .as_str()
        .unwrap_or_default()
        .contains("category"));
}

#[test]
fn unknown_collection_returns_400304() {
    if !common::kv_ready() {
        return common::skip_kv("post_call_chat 未知分类");
    }
    let res = common::call(
        post_call_chat,
        r#"{"category":"no_such_collection_chatllm_test","prompts":"测试"}"#,
    );
    assert_eq!(res["code"], 400304, "响应: {}", res);
}

#[test]
fn plugin_call_is_forwarded_and_errors_mapped() {
    if !common::kv_ready() {
        return common::skip_kv("post_call_chat 插件调用");
    }
    // chat_plugin 中只有 do_test_chat；它只接受 prompt/messages，
    // 四字段请求会被插件拒绝，chatllm 应映射为 400304。
    let res = common::call(
        post_call_chat,
        r#"{"category":"chat_plugin","prompts":"do_test_chat","context":{"a":1}}"#,
    );
    assert_eq!(res["code"], 400304, "响应: {}", res);
}

#[test]
fn plugin_data_is_returned_verbatim() {
    if !common::kv_ready() {
        return common::skip_kv("post_call_chat 插件 data");
    }
    let data = plugins::call_plugin_data("do_test_chat", "chat", &json!({ "prompt": "你好" }))
        .expect("调用 do_test_chat 失败");
    assert_eq!(data["tag"], "do_chat_test");
    assert_eq!(data["input"], "你好");
    assert!(data["content"]
        .as_str()
        .unwrap_or_default()
        .contains("do_chat_test"));
    assert!(data["time"].is_number());
}

#[test]
fn plugin_library_is_cached_across_calls() {
    if !common::kv_ready() {
        return common::skip_kv("plugins 缓存");
    }
    for index in 0..3 {
        let data = plugins::call_plugin_data(
            "do_test_chat",
            "chat",
            &json!({ "prompt": format!("第{}次", index) }),
        )
        .expect("重复调用应成功");
        assert_eq!(data["tag"], "do_chat_test");
    }
}

#[test]
fn missing_plugin_returns_error() {
    if !common::kv_ready() {
        return common::skip_kv("plugins 缺失");
    }
    let error = plugins::call_plugin_data(
        "no_such_plugin_chatllm",
        "chat",
        &json!({ "prompt": "x" }),
    )
    .expect_err("不存在的插件应失败");
    assert!(
        error.to_string().contains("no_such_plugin_chatllm"),
        "错误信息应包含插件名: {}",
        error
    );
}

#[test]
fn concurrent_plugin_calls_are_safe() {
    if !common::kv_ready() {
        return common::skip_kv("plugins 并发");
    }
    let handles = (0..4)
        .map(|index| {
            std::thread::spawn(move || {
                plugins::call_plugin_data(
                    "do_test_chat",
                    "chat",
                    &json!({ "prompt": format!("并发{}", index) }),
                )
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        let data = handle
            .join()
            .expect("并发线程 panic")
            .expect("并发调用失败");
        assert_eq!(data["tag"], "do_chat_test");
    }
}
