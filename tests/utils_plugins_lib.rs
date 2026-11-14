// tests/utils_plugins_lib.rs —— utils::plugins_lib 的响应解析与符号清单测试
use chatllm::utils::plugins_lib::{
    check_ret, is_error_code, parse_response, preview, symbols, PluginLibError, CODE_CONFLICT,
    CODE_NOT_FOUND,
};

#[test]
fn symbols_match_export_table() {
    let all = [
        symbols::GET_PLUGINS,
        symbols::POST_PLUGINS,
        symbols::PATCH_PLUGINS,
        symbols::UPDATE_PLUGINS,
        symbols::DELETE_PLUGINS,
        symbols::GET_FUNCTIONS,
        symbols::POST_FUNCTION,
        symbols::UPDATE_FUNCTION,
        symbols::PATCH_FUNCTION,
        symbols::DELETE_FUNCTION,
        symbols::GET_SKILLS,
        symbols::GET_SKILL,
        symbols::POST_SKILLS,
        symbols::UPDATE_SKILLS,
        symbols::PATCH_SKILLS,
        symbols::DELETE_SKILLS,
        symbols::GET_PROMPTS,
        symbols::UPDATE_PROMPTS,
        symbols::GET_TASKFLOW,
        symbols::UPDATE_TASKFLOW,
        symbols::INSTALL_PLUGIN,
        symbols::UPDATE_PLUGIN,
    ];
    assert_eq!(all.len(), 22);
    for symbol in all {
        assert!(!symbol.contains("post_get_"), "符号不应二次加前缀");
    }
}

#[test]
fn parse_response_success_returns_data() {
    let out = r#"{"code":200,"data":{"list":[{"uuid":"p1"}]},"msg":"success"}"#;
    let data = parse_response(symbols::GET_PLUGINS, out).unwrap();
    assert_eq!(data["list"][0]["uuid"], "p1");
}

#[test]
fn parse_response_error_carries_code() {
    let out = r#"{"code":400404,"data":{},"msg":"plugin not found"}"#;
    let err = parse_response(symbols::GET_PLUGINS, out).unwrap_err();

    let detail = err
        .downcast_ref::<PluginLibError>()
        .expect("应保留结构化错误");
    assert_eq!(detail.code, CODE_NOT_FOUND);
    assert_eq!(detail.msg, "plugin not found");
    assert!(is_error_code(&err, CODE_NOT_FOUND));
    assert!(!is_error_code(&err, CODE_CONFLICT));
}

#[test]
fn parse_response_missing_code_is_error() {
    let err = parse_response(symbols::GET_PLUGINS, r#"{"data":{}}"#).unwrap_err();
    assert!(err.to_string().contains("code=-1"));
}

#[test]
fn parse_response_rejects_bad_json() {
    assert!(parse_response(symbols::GET_PLUGINS, "not json").is_err());
}

#[test]
fn check_ret_maps_codes() {
    assert!(check_ret(symbols::GET_PLUGINS, 0, 65536).is_ok());

    let err = check_ret(symbols::GET_PLUGINS, -1, 65536)
        .unwrap_err()
        .to_string();
    assert!(err.contains("为空指针"));

    let err = check_ret(symbols::GET_PLUGINS, -2, 65536)
        .unwrap_err()
        .to_string();
    assert!(err.contains("64 KiB"));

    let err = check_ret(symbols::GET_PLUGINS, 9, 64)
        .unwrap_err()
        .to_string();
    assert!(err.contains("FFI 返回 9"));
}

#[test]
fn preview_truncates_by_char() {
    assert_eq!(preview("abc", 5), "abc");
    assert_eq!(preview("abcdef", 3), "abc...");
    assert_eq!(preview("中文内容", 2), "中文...");
}
