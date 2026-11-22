// tests/base_message.rs —— post_message 的数据调用测试
mod common;

use chatllm::ffi::chat::{get_dialogue, patch_dialogue, post_message};

#[test]
fn post_message_creates_then_continues() {
    if !common::db_ready() {
        return common::skip("post_message 创建/续聊");
    }

    let created = common::call(
        post_message,
        r#"{"prompt":"你好，介绍一下自己","title":"测试新建"}"#,
    );
    assert_eq!(created["code"], 200);
    assert_eq!(created["data"]["created"], true);
    assert!(
        created["data"]["content"].is_string(),
        "应返回 reply 处理后的 content"
    );

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

    // 每次对话写入轮次记录、think / plugins / reply 与 assistant 共 5 条。
    let res = common::call(patch_dialogue, r#"{"keyword":"测试新建"}"#);
    let row = res["data"]["list"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == uuid.as_str())
        .expect("列表应能查到新建对话")
        .clone();
    assert_eq!(row["message_count"], 10);
    assert_eq!(row["last_seq"], 10);

    // 清理测试数据。
    chatllm::utils::dialogue::delete_dialogue(&uuid).expect("删除测试对话失败");
}

#[test]
fn post_message_creates_hidden_dialogue_for_unknown_uuid() {
    if !common::db_ready() {
        return common::skip("post_message 未知 uuid");
    }

    let uuid = "99999999-9999-4999-8999-999999999999";
    // 清掉上次运行可能留下的数据，保证 created 为 true。
    chatllm::utils::dialogue::delete_dialogue(uuid).expect("清理测试对话失败");

    let res = common::call(
        post_message,
        &format!(r#"{{"prompt":"回调内容","uuid":"{}"}}"#, uuid),
    );
    assert_eq!(res["code"], 200);
    assert_eq!(res["data"]["created"], true);
    assert_eq!(res["data"]["uuid"], uuid);

    // uuid 不存在时创建的对话不显示。
    let detail = common::call(
        get_dialogue,
        &format!(r#"{{"uuid":"{}","limit":20}}"#, uuid),
    );
    assert_eq!(detail["code"], 200);
    assert_eq!(detail["data"]["dialogue"]["show_status"], 2);

    chatllm::utils::dialogue::delete_dialogue(uuid).expect("删除测试对话失败");
}

#[test]
fn missing_prompt_returns_400102() {
    let res = common::call(post_message, r#"{"uuid":"x"}"#);
    assert_eq!(res["code"], 400102);
}

#[test]
fn post_message_round_contains_all_phases() {
    if !common::db_ready() {
        return common::skip("post_message 轮次阶段");
    }

    let created = common::call(
        post_message,
        r#"{"prompt":"round 阶段测试","title":"round 阶段测试"}"#,
    );
    assert_eq!(created["code"], 200);
    let uuid = created["data"]["uuid"]
        .as_str()
        .expect("应返回 uuid")
        .to_string();

    let detail = common::call(get_dialogue, &format!(r#"{{"uuid":"{}","limit":20}}"#, uuid));
    assert_eq!(detail["code"], 200);
    let list = detail["data"]["messages"]["list"]
        .as_array()
        .expect("messages.list 应为数组");
    let round = list
        .iter()
        .find(|row| row.get("user").is_some())
        .expect("应写入轮次记录");
    assert_eq!(round["user"]["content"], "round 阶段测试");
    assert!(
        round["sys"]["content"].is_string(),
        "轮次记录应包含 sys 内容"
    );
    // 列表接口不返回可能很大的 content。
    assert!(round.get("content").is_none(), "列表应隐藏 content");

    // 阶段内容同时以独立消息写入。
    for phase in ["think", "plugins", "reply"] {
        assert!(
            list.iter().any(|row| row["role"] == phase),
            "应写入 {phase} 消息"
        );
    }

    chatllm::utils::dialogue::delete_dialogue(&uuid).expect("删除测试对话失败");
}
