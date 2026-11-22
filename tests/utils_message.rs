// tests/utils_message.rs —— utils::message 的轮次 / 阶段消息完整流程
mod common;

use chatllm::utils::{dialogue, message};
use serde_json::json;

#[test]
fn round_and_phase_messages_complete_flow() {
    if !common::db_ready() {
        return common::skip("message 轮次 / 阶段消息");
    }

    let uuid = dialogue::create_dialogue_with(None, "message 流程测试", 1, None)
        .expect("创建测试对话失败");

    let user = "用户输入";
    let sys = "系统回答";

    // 轮次记录按阶段增量写入：think → plugins → reply + sys。
    let round = message::start_round(&uuid, user, json!([{ "think": { "status": "ok" } }]))
        .expect("开始轮次失败");
    message::update_round(
        &uuid,
        &round,
        &json!([{ "think": { "status": "ok" }, "plugins": { "status": "skipped" } }]),
        None,
    )
    .expect("更新 plugins 阶段失败");
    message::update_round(
        &uuid,
        &round,
        &json!([{
            "think": { "status": "ok" },
            "plugins": { "status": "skipped" },
            "reply": { "status": "ok" }
        }]),
        Some(sys),
    )
    .expect("更新 reply 阶段失败");

    // 阶段消息单独插入。
    message::append_phase(&uuid, "think", json!({ "status": "ok" })).expect("写 think 失败");
    message::append_phase(&uuid, "plugins", json!({ "status": "skipped" }))
        .expect("写 plugins 失败");
    message::append_phase(&uuid, "reply", json!({ "status": "ok" })).expect("写 reply 失败");

    // 列表：轮次记录 + think / plugins / reply，共 4 条。
    let list = message::list_messages(&uuid, None, None, Some(20)).expect("读取消息失败");
    let rows = list["list"].as_array().expect("list 应为数组");
    assert_eq!(rows.len(), 4);
    assert_eq!(list["last_id"], 4);

    // 列表隐藏 content。
    assert!(
        rows.iter().all(|row| row.get("content").is_none()),
        "列表应隐藏 content"
    );

    // 轮次记录包含 user / sys。
    let round_row = rows
        .iter()
        .find(|row| row.get("user").is_some())
        .expect("应有轮次记录");
    assert_eq!(round_row["user"]["content"], user);
    assert_eq!(round_row["sys"]["content"], sys);

    // 三个阶段的独立消息。
    for phase in ["think", "plugins", "reply"] {
        assert!(
            rows.iter().any(|row| row["role"] == phase),
            "应有 {phase} 阶段消息"
        );
    }

    // 上下文只还原 user / assistant。
    let history = message::message_history(&uuid).expect("读取上下文失败");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0]["role"], "user");
    assert_eq!(history[0]["content"], user);
    assert_eq!(history[1]["role"], "assistant");
    assert_eq!(history[1]["content"], sys);

    dialogue::delete_dialogue(&uuid).expect("删除测试对话失败");
}
