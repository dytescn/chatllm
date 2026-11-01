// src/utils/reply.rs
//! 助手回复组装：think 结果交给 `reply_sys_info` 插件，转成面向用户的语义提示。
//!
//! 插件调用走 cyctron 的设计插件回调（`POST /reply_sys_info`），不在 chatllm 内 dlopen；
//! `reply` 自身也不访问 cloud、不查询 kv_lib。

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::utils::markparse;

/// cyctron 地址；可用 `CYCTRON_ADDR` 覆盖。
const DEFAULT_ADDR: &str = "127.0.0.1:44944";
/// 设计插件端点：cyctron 按 `./plugins/<name>/` 解析。
const PLUGIN_ENDPOINT: &str = "/reply_sys_info";
/// 单次调用超时。
const TIMEOUT: Duration = Duration::from_secs(15);

/// 一条组装好的助手回复。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// 回复正文。
    pub content: String,
    /// 思考内容；没有时为 `None`。
    pub think: Option<String>,
    /// 正文的 HTML 版本。
    pub html: String,
}

impl Reply {
    /// 从正文与可选思考内容构造。
    pub fn new(content: &str, think: Option<String>) -> Self {
        let content = content.trim().to_string();
        let html = markparse::to_html(&content).unwrap_or_default();
        Self {
            content,
            think,
            html,
        }
    }

    /// 调用 `reply_sys_info` 插件，把 think 结果转换成语义提示。
    ///
    /// 插件不可用时回退到 think 结果或输入回显，不阻断对话。
    pub fn from_think(think: Option<String>, input: &str) -> Self {
        let content = match call_plugin(input, think.as_deref()) {
            Ok(content) if !content.trim().is_empty() => content,
            Ok(_) => fallback(think.as_deref(), input),
            Err(error) => {
                eprintln!("[reply] 调用 reply_sys_info 失败: {:#}", error);
                fallback(think.as_deref(), input)
            }
        };
        Self::new(&content, None)
    }
}

fn fallback(think: Option<&str>, input: &str) -> String {
    think
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("我收到你的信息了：{input}"))
}

/// 通过 cyctron 的设计插件回调调用 `reply_sys_info`，返回 `data.content`。
fn call_plugin(input: &str, think: Option<&str>) -> Result<String> {
    let addr = std::env::var("CYCTRON_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let body = json!({ "prompt": input, "think": think }).to_string();

    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/libary\r\nFFI-Symbol: chat\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        PLUGIN_ENDPOINT,
        addr,
        body.len(),
        body
    );

    let mut stream =
        TcpStream::connect(&addr).with_context(|| format!("连接 cyctron {} 失败", addr))?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .context("设置读取超时失败")?;
    stream
        .set_write_timeout(Some(TIMEOUT))
        .context("设置写入超时失败")?;
    stream
        .write_all(request.as_bytes())
        .context("写入插件请求失败")?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .context("读取插件响应失败")?;
    let response = String::from_utf8_lossy(&response);

    let (head, body) = response
        .split_once("\r\n\r\n")
        .context("插件响应缺少 header 分隔")?;
    let status = head.lines().next().unwrap_or("");
    if !status.contains(" 200 ") {
        bail!("插件 HTTP 状态异常: {}", status);
    }

    let value: Value = serde_json::from_str(body.trim())
        .with_context(|| format!("插件响应解析失败: {}", body.trim()))?;
    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 && code != 200 {
        let msg = value
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        bail!("reply_sys_info 错误 (code={}): {}", code, msg);
    }

    Ok(value
        .get("data")
        .and_then(|data| data.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string())
}
