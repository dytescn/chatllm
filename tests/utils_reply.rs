// tests/utils_reply.rs —— utils::reply 的组装测试
use chatllm::utils::reply::{
    from_contents, from_result, from_think, is_resource_hit, parse_hit, Reply, BASE_KEYWORD,
    REPLY_COLLECTION,
};
use serde_json::json;

#[test]
fn assembles_think_and_html() {
    let reply = Reply::new("**你好**", Some("思考中".to_string()));
    assert_eq!(reply.content, "**你好**");
    assert_eq!(reply.think.as_deref(), Some("思考中"));
    // `content_html` 依赖 ./libary/parse.dll，缺失时为空，这里只校验正文与思考。
}

#[test]
fn from_contents_joins_and_falls_back() {
    let reply = from_contents(&["a".to_string(), " b ".to_string()], "input");
    assert_eq!(reply.content, "a\n\nb");

    let empty = from_contents(&[], "input");
    assert_eq!(empty.content, "我收到你的信息了：input");
}

#[test]
fn trims_content() {
    let reply = Reply::new("  正文  ", None);
    assert_eq!(reply.content, "正文");
    assert!(reply.think.is_none());
}

#[test]
fn from_think_uses_result() {
    // 插件不可用时回退到 think 结果。
    let reply = from_think(Some("思考结果".to_string()), "输入");
    assert_eq!(reply.content, "思考结果");
    assert!(reply.think.is_none());
}

#[test]
fn from_think_falls_back_to_input() {
    let reply = from_think(None, "输入");
    assert_eq!(reply.content, "我收到你的信息了：输入");
}

#[test]
fn from_result_uses_plugin_output() {
    // plugins 阶段成功后，把插件输出作为上游结果传给 reply。
    let reply = from_result(Some("插件输出".to_string()), "输入");
    assert_eq!(reply.content, "插件输出");
    assert!(reply.think.is_none());
}

#[test]
fn from_result_falls_back_to_input() {
    let reply = from_result(None, "输入");
    assert_eq!(reply.content, "我收到你的信息了：输入");
}

#[test]
fn parses_chat_reply_from_meta() {
    let hit = json!({
        "key": "reply-sys-info",
        "meta": {
            "plugins_uid": "6f2b8d41-9c3a-4e7b-8d5f-1a2c3e4b5d60",
            "plugin_name": "reply_sys_info"
        }
    });
    let parsed = parse_hit(&hit).expect("应解析成功");
    assert_eq!(
        parsed.plugins_uid.as_deref(),
        Some("6f2b8d41-9c3a-4e7b-8d5f-1a2c3e4b5d60")
    );
}

#[test]
fn parses_chat_reply_from_value() {
    let hit = json!({
        "key": "reply-value",
        "value": "{\"plugins_uid\":\"6f2b8d41-9c3a-4e7b-8d5f-1a2c3e4b5d60\"}"
    });
    let parsed = parse_hit(&hit).expect("应解析成功");
    assert_eq!(
        parsed.plugins_uid.as_deref(),
        Some("6f2b8d41-9c3a-4e7b-8d5f-1a2c3e4b5d60")
    );
}

#[test]
fn rejects_chat_reply_without_plugin() {
    assert!(parse_hit(&json!({ "key": "empty" })).is_none());
}

#[test]
fn reply_collection_is_chat_reply() {
    assert_eq!(REPLY_COLLECTION, "chat_reply");
}

#[test]
fn detects_resource_hit_without_plugin() {
    let hit = json!({
        "key": "reply-sys-info:skill:main",
        "meta": {
            "plugin_name": "reply_sys_info",
            "kind": "skill"
        }
    });
    assert!(is_resource_hit(&hit));
    assert!(parse_hit(&hit).is_none());
}

#[test]
fn reply_uses_base_as_default_category_keyword() {
    assert_eq!(BASE_KEYWORD, "base");
}
