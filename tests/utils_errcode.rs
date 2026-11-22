// tests/utils_errcode.rs —— utils::errcode 的错误码与文案映射
use chatllm::utils::errcode::ChatError;

#[test]
fn maps_invalid_uuid() {
    let error = ChatError::InvalidUuid("bad".to_string());
    assert_eq!(error.code(), 400103);
    assert!(error.message().contains("invalid uuid: bad"));
}

#[test]
fn maps_create_error() {
    let error = ChatError::Create("boom".to_string());
    assert_eq!(error.code(), 400300);
    assert!(error.message().contains("create dialogue failed: boom"));
}

#[test]
fn maps_load_error() {
    let error = ChatError::Load("missing".to_string());
    assert_eq!(error.code(), 400303);
    assert!(error.message().contains("load dialogue failed: missing"));
}

#[test]
fn maps_append_error() {
    let error = ChatError::Append("disk".to_string());
    assert_eq!(error.code(), 400301);
    assert!(error.message().contains("append user message failed: disk"));
}

#[test]
fn maps_plugin_error() {
    let error = ChatError::Plugin("callback".to_string());
    assert_eq!(error.code(), 400304);
    assert!(error.message().contains("plugin call failed: callback"));
}
