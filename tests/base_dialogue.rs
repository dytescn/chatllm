// tests/base_dialogue.rs —— patch_dialogue / update_dialogue 的数据调用测试
mod common;

use chatllm::ffi::chat::{patch_dialogue, update_dialogue};

/// seed.ts 插入的基础对话。
const SEED_ID: &str = "11111111-1111-4111-8111-111111111111";
/// seed.ts 中 show_status = 2 的对话。
const HIDDEN_ID: &str = "33333333-3333-4333-8333-333333333333";
/// seed.ts 中 is_archived = 1 的对话。
const ARCHIVED_ID: &str = "44444444-4444-4444-8444-444444444444";

fn list_ids(res: &serde_json::Value) -> Vec<String> {
    res["data"]["list"]
        .as_array()
        .expect("data.list 应为数组")
        .iter()
        .filter_map(|row| row["id"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn patch_dialogue_returns_visible_list() {
    if !common::db_ready() {
        return common::skip("patch_dialogue 列表");
    }

    let res = common::call(patch_dialogue, "");
    assert_eq!(res["code"], 200);
    assert_eq!(res["data"]["page"], 1);

    let ids = list_ids(&res);
    assert!(ids.contains(&SEED_ID.to_string()), "应包含基础对话");
    assert!(!ids.contains(&HIDDEN_ID.to_string()), "show_status = 2 不应出现");
    assert!(!ids.contains(&ARCHIVED_ID.to_string()), "归档对话不应出现");
}

#[test]
fn patch_dialogue_searches_by_title() {
    if !common::db_ready() {
        return common::skip("patch_dialogue 搜索");
    }

    let res = common::call(patch_dialogue, r#"{"keyword":"接口","page":1,"page_size":10}"#);
    assert_eq!(res["code"], 200);

    let list = res["data"]["list"].as_array().unwrap();
    assert!(!list.is_empty(), "应能搜到「接口联调记录」");
    assert!(list.iter().all(|row| row["title"]
        .as_str()
        .unwrap_or("")
        .contains("接口")));
}

#[test]
fn update_dialogue_updates_title() {
    if !common::db_ready() {
        return common::skip("update_dialogue 更新");
    }

    let updated = format!(r#"{{"uuid":"{}","title":"产品需求讨论（已更新）"}}"#, SEED_ID);
    let res = common::call(update_dialogue, &updated);
    assert_eq!(res["code"], 200);
    assert_eq!(res["data"]["id"], SEED_ID);

    // 通过列表接口确认已写入。
    let res = common::call(patch_dialogue, r#"{"keyword":"已更新"}"#);
    assert!(list_ids(&res).contains(&SEED_ID.to_string()));

    // 恢复基础数据，保证测试可重复。
    let restored = format!(r#"{{"uuid":"{}","title":"产品需求讨论"}}"#, SEED_ID);
    let res = common::call(update_dialogue, &restored);
    assert_eq!(res["code"], 200);
}

#[test]
fn update_dialogue_updates_pin_and_archive() {
    if !common::db_ready() {
        return common::skip("update_dialogue 置顶/归档");
    }

    const OTHER_ID: &str = "22222222-2222-4222-8222-222222222222";

    let pinned = format!(r#"{{"uuid":"{}","is_pinned":1}}"#, OTHER_ID);
    let res = common::call(update_dialogue, &pinned);
    assert_eq!(res["code"], 200);

    let res = common::call(patch_dialogue, r#"{"keyword":"接口"}"#);
    let row = res["data"]["list"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == OTHER_ID)
        .cloned()
        .expect("应能查到该对话");
    assert_eq!(row["is_pinned"], 1);

    // 恢复基础数据。
    let restored = format!(r#"{{"uuid":"{}","is_pinned":0}}"#, OTHER_ID);
    let res = common::call(update_dialogue, &restored);
    assert_eq!(res["code"], 200);
}

#[test]
fn update_dialogue_rejects_bad_uuid() {
    if !common::db_ready() {
        return common::skip("update_dialogue 非法 uuid");
    }

    let res = common::call(update_dialogue, r#"{"uuid":"not-a-uuid","title":"x"}"#);
    assert_eq!(res["code"], 400103);
}

#[test]
fn invalid_json_returns_400102() {
    let res = common::call(patch_dialogue, r#"{"page":"bad"}"#);
    assert_eq!(res["code"], 400102);

    let res = common::call(update_dialogue, r#"{"show_status":"bad"}"#);
    assert_eq!(res["code"], 400102);
}
