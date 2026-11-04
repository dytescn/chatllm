// tests/utils_think.rs —— utils::think 的分隔符与分段规则测试
use chatllm::utils::think::{last_segment, segments, COLLECTION, SEPARATOR, TOP_K};

#[test]
fn uses_pipe_as_separator() {
    assert_eq!(SEPARATOR, "|");
    assert_eq!(COLLECTION, "chat_think");
    assert_eq!(TOP_K, 5);
}

#[test]
fn splits_and_drops_empty_segments() {
    assert_eq!(
        segments("问候|系统信息|默认"),
        vec!["问候", "系统信息", "默认"]
    );
    assert_eq!(segments(" a | | b | "), vec!["a", "b"]);
    assert_eq!(segments("没有分隔符"), vec!["没有分隔符"]);
    assert!(segments("").is_empty());
    assert!(segments(" | | ").is_empty());
}

#[test]
fn picks_last_non_empty_segment() {
    assert_eq!(last_segment("问候|系统信息|默认"), Some("默认"));
    assert_eq!(last_segment("问候|系统信息|  "), Some("系统信息"));
    assert_eq!(last_segment("没有分隔符"), Some("没有分隔符"));
    assert_eq!(last_segment(" | | "), None);
    assert_eq!(last_segment(""), None);
}
