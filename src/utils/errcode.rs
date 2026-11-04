// src/utils/errcode.rs
//! FFI 业务错误码。

/// 对话接口的业务错误，`code()` 直接作为 FFI 响应码。
#[derive(Debug)]
pub enum ChatError {
    InvalidUuid(String),
    Create(String),
    Load(String),
    Append(String),
}

impl ChatError {
    pub fn code(&self) -> i32 {
        let code = match self {
            ChatError::InvalidUuid(_) => 400103,
            ChatError::Create(_) => 400300,
            ChatError::Load(_) => 400303,
            ChatError::Append(_) => 400301,
        };
        println!("[errcode] ChatError::code = {}", code);
        code
    }

    pub fn message(&self) -> String {
        let message = match self {
            ChatError::InvalidUuid(e) => format!("invalid uuid: {}", e),
            ChatError::Create(e) => format!("create dialogue failed: {}", e),
            ChatError::Load(e) => format!("load dialogue failed: {}", e),
            ChatError::Append(e) => format!("append user message failed: {}", e),
        };
        println!("[errcode] ChatError::message = {}", message);
        message
    }
}
