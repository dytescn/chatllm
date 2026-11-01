// tests/base_message.rs —— post_message 的数据调用测试
mod common;

use chatllm::ffi::chat::{patch_dialogue, post_message};

#[test]
fn post_message_creates_then_continues() {
    if !common::db_ready() {
        return common::skip("post_message 创建/续聊");
    }

    let created = common::call(post_message, r#"{"prompt":"你好，介绍一下自己","title":"测试新建"}"#);
    assert_eq!(created["code"], 200);
    assert_eq!(created["data"]["created"], true);

    let uuid = created["data"]["uuid"].as_str().expect("应返回 uuid").to_string();
    assert_eq!(uuid.len(), 36);

    // 续聊：同一个 uuid，不应重复创建。
    let continued = common::call(
        post_message,
        &format!(r#"{{"prompt":"继续","uuid":"{}"}}"#, uuid),
    );
    assert_eq!(continued["code"], 200);
    assert_eq!(continued["data"]["created"], false);
    assert_eq!(continued["data"]["uuid"], uuid.as_str());

    // 每条消息都会写入 user + assistant 两条（assistant 为云端回复或错误文本）。
    let res = common::call(patch_dialogue, r#"{"keyword":"测试新建"}"#);
    let row = res["data"]["list"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == uuid.as_str())
        .expect("列表应能查到新建对话")
        .clone();
    assert_eq!(row["message_count"], 4);
    assert_eq!(row["last_seq"], 4);

    // 清理测试数据。
    chatllm::utils::dialogue::delete_dialogue(&uuid).expect("删除测试对话失败");
}

#[test]
fn post_message_rejects_unknown_uuid() {
    if !common::db_ready() {
        return common::skip("post_message 对话不存在");
    }

    let res = common::call(
        post_message,
        r#"{"prompt":"x","uuid":"99999999-9999-4999-8999-999999999999"}"#,
    );
    assert_eq!(res["code"], 400303);
}

#[test]
fn missing_prompt_returns_400102() {
    let res = common::call(post_message, r#"{"uuid":"x"}"#);
    assert_eq!(res["code"], 400102);
}
