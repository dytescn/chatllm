// tests/base_message.rs —— post_message 的数据调用测试
mod common;

use chatllm::ffi::chat::{patch_dialogue, post_message};

/// 未选择 AI 供应商时，check 直接返回设置提示，不创建对话。
#[test]
fn post_message_prompts_to_choose_ai_supplier() {
    if !common::db_ready() {
        return common::skip("post_message 未选择 AI 供应商");
    }
    if chatllm::utils::check::missing_supplier_reply().is_none() {
        eprintln!("跳过 post_message 配置缺失：testenv 已配置供应商与 AppKey");
        return;
    }

    let created = common::call(post_message, r#"{"prompt":"你好","title":"未选择供应商"}"#);
    assert_eq!(created["code"], 200);
    assert_eq!(created["data"]["route"], "/setting");
    assert!(created["data"]["uuid"].is_null(), "check 阶段不创建对话");
    assert_eq!(created["data"]["created"], false);
    let content = created["data"]["content"].as_str().unwrap_or_default();
    assert!(content.contains("请选择设置AI供应商"));
    assert!(content.contains("/setting"));
}

#[test]
fn post_message_creates_then_continues() {
    if !common::db_ready() {
        return common::skip("post_message 创建/续聊");
    }
    if chatllm::utils::check::missing_supplier_reply().is_some() {
        eprintln!("跳过 post_message 创建/续聊：testenv 未配置供应商与 AppKey");
        return;
    }

    let created = common::call(
        post_message,
        r#"{"prompt":"你好，介绍一下自己","title":"测试新建"}"#,
    );
    assert_eq!(created["code"], 200);
    assert_eq!(created["data"]["created"], true);

    let uuid = created["data"]["uuid"]
        .as_str()
        .expect("应返回 uuid")
        .to_string();
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
    if chatllm::utils::check::missing_supplier_reply().is_some() {
        eprintln!("跳过 post_message 对话不存在：testenv 未配置供应商与 AppKey");
        return;
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
