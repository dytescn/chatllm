// tests/base_get_dialogue.rs —— get_dialogue 的数据调用测试
mod common;

use chatllm::ffi::chat::{get_dialogue, post_message};

/// seed.ts 插入的基础对话。
const SEED_ID: &str = "11111111-1111-4111-8111-111111111111";

#[test]
fn get_dialogue_returns_dialogue_and_messages() {
    if !common::db_ready() {
        return common::skip("get_dialogue 读取");
    }

    let body = format!(r#"{{"uuid":"{}","limit":10}}"#, SEED_ID);
    let res = common::call(get_dialogue, &body);
    assert_eq!(res["code"], 200);
    assert_eq!(res["data"]["dialogue"]["id"], SEED_ID);
    assert_eq!(res["data"]["messages"]["limit"], 10);
    assert!(res["data"]["messages"]["list"].is_array());
}

#[test]
fn get_dialogue_after_post_message_returns_conversation() {
    if !common::db_ready() {
        return common::skip("get_dialogue 消息");
    }

    let prompt = "get_dialogue 测试";
    let created = common::call(
        post_message,
        &format!(r#"{{"prompt":"{}","title":"get_dialogue 测试"}}"#, prompt),
    );
    assert_eq!(created["code"], 200);
    let uuid = created["data"]["uuid"].as_str().expect("应返回 uuid").to_string();

    let body = format!(r#"{{"uuid":"{}","limit":20}}"#, uuid);
    let res = common::call(get_dialogue, &body);
    assert_eq!(res["code"], 200);

    let list = res["data"]["messages"]["list"].as_array().unwrap();
    assert!(list.len() >= 2, "应包含 user 与 assistant 消息");
    assert_eq!(list[0]["role"], "user");
    assert_eq!(list[0]["content"], prompt);
    assert_eq!(list[1]["role"], "assistant");

    chatllm::utils::dialogue::delete_dialogue(&uuid).expect("删除测试对话失败");
}

#[test]
fn get_dialogue_rejects_bad_uuid() {
    if !common::db_ready() {
        return common::skip("get_dialogue 非法 uuid");
    }

    let res = common::call(get_dialogue, r#"{"uuid":"not-a-uuid"}"#);
    assert_eq!(res["code"], 400103);
}

#[test]
fn get_dialogue_requires_uuid() {
    let res = common::call(get_dialogue, r#"{"limit":10}"#);
    assert_eq!(res["code"], 400102);
}
