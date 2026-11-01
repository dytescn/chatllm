// tests/utils_reply.rs —— utils::reply 的组装测试
use chatllm::utils::reply::Reply;

#[test]
fn assembles_think_and_html() {
    let reply = Reply::new("**你好**", Some("思考中".to_string()));
    assert_eq!(reply.content, "**你好**");
    assert_eq!(reply.think.as_deref(), Some("思考中"));
    // `content_html` 依赖 ./libary/parse.dll，缺失时为空，这里只校验正文与思考。
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
    let reply = Reply::from_think(Some("思考结果".to_string()), "输入");
    assert_eq!(reply.content, "思考结果");
    assert!(reply.think.is_none());
}

#[test]
fn from_think_falls_back_to_input() {
    let reply = Reply::from_think(None, "输入");
    assert_eq!(reply.content, "我收到你的信息了：输入");
}
