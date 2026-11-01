// tests/utils_dialogue.rs —— utils::dialogue 的纯函数测试
use chatllm::utils::dialogue::{esc, norm_page, normalize_dialogue_row};
use serde_json::json;

#[test]
fn sql_escape_quotes() {
    assert_eq!(esc("a'b"), "'a''b'");
    assert_eq!(esc("abc"), "'abc'");
}

#[test]
fn page_normalization() {
    assert_eq!(norm_page(None, None), (1, 20, 0));
    assert_eq!(norm_page(Some(2), Some(15)), (2, 15, 15));
    assert_eq!(norm_page(Some(0), Some(500)), (1, 100, 0));
}

#[test]
fn normalize_dialogue_defaults() {
    let row = json!({ "id": "x" });
    let out = normalize_dialogue_row(row);
    assert_eq!(out["id"], "x");
    assert_eq!(out["show_status"], 1);
    assert_eq!(out["message_count"], 0);
    assert_eq!(out["last_seq"], 0);
    assert_eq!(out["is_pinned"], 0);
    assert_eq!(out["is_archived"], 0);
}
