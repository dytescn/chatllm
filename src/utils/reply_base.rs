// src/utils/reply_base.rs
//! reply_base 资源包：读取 skills / prompts，并处理 plugins 结果。
//!
//! 当前测试规则：plugins 结果形如 `<文本>do_chat_test<时间>` 时，
//! 去掉 `do_chat_test` 与其后的时间，保留文本；多段用空行连接。

use std::fs;

use serde_json::Value;

/// reply_base skills 文件（安装到 testenv/plugins/reply_base/）。
pub const SKILLS_PATH: &str = "./plugins/reply_base/reply_base.md";
/// reply_base prompts 文件。
pub const PROMPTS_PATH: &str = "./plugins/reply_base/reply_base.json";
/// 测试插件结果中的标记。
pub const TAG: &str = "do_chat_test";

/// reply_base 安装清单。
pub const MANIFEST_PATH: &str = "./plugins/reply_base/dytes.json";

/// 读取 reply_base 的 uuid。
pub fn uuid() -> Option<String> {
    let text = match fs::read_to_string(MANIFEST_PATH) {
        Ok(text) => text,
        Err(error) => {
            println!(
                "[reply_base] manifest 读取失败: {} ({})",
                MANIFEST_PATH, error
            );
            return None;
        }
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => {
            println!("[reply_base] manifest 解析失败: {}", error);
            return None;
        }
    };
    let uuid = value
        .get("uuid")
        .and_then(Value::as_str)
        .map(str::to_string);
    println!("[reply_base] uuid: {:?}", uuid);
    uuid
}

/// 读取 reply_base skills 内容。
pub fn skills() -> Option<String> {
    match fs::read_to_string(SKILLS_PATH) {
        Ok(text) => {
            let text = text.trim().to_string();
            if text.is_empty() {
                println!("[reply_base] skills 为空: {}", SKILLS_PATH);
                None
            } else {
                println!(
                    "[reply_base] skills 读取成功: {} ({} chars)",
                    SKILLS_PATH,
                    text.len()
                );
                Some(text)
            }
        }
        Err(error) => {
            println!("[reply_base] skills 读取失败: {} ({})", SKILLS_PATH, error);
            None
        }
    }
}

/// 读取 reply_base prompts；仅用于日志/后续扩展。
pub fn prompts() -> Option<Value> {
    match fs::read_to_string(PROMPTS_PATH) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(value) => {
                let count = value.as_array().map(|items| items.len()).unwrap_or(0);
                println!(
                    "[reply_base] prompts 读取成功: {} ({} 条)",
                    PROMPTS_PATH, count
                );
                Some(value)
            }
            Err(error) => {
                println!(
                    "[reply_base] prompts 解析失败: {} ({})",
                    PROMPTS_PATH, error
                );
                None
            }
        },
        Err(error) => {
            println!(
                "[reply_base] prompts 读取失败: {} ({})",
                PROMPTS_PATH, error
            );
            None
        }
    }
}

/// 按 reply_base skills 规则处理 plugins 结果。
///
/// reply_base 不可用时返回 `None`，调用方回退到原始结果。
pub fn process(result: &str) -> Option<String> {
    skills()?;
    let _ = prompts();
    let cleaned = clean(result);
    if cleaned.is_empty() {
        println!("[reply_base] 清洗后为空");
        None
    } else {
        println!("[reply_base] 清洗结果: {:?}", preview(&cleaned, 500));
        Some(cleaned)
    }
}

/// 去除每段的 `do_chat_test<时间>` 后缀，并用空行连接多段。
pub fn clean(result: &str) -> String {
    result
        .split("\n\n")
        .filter_map(clean_segment)
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn clean_segment(segment: &str) -> Option<String> {
    let text = segment.trim();
    if text.is_empty() {
        return None;
    }
    let cleaned = match text.split_once(TAG) {
        Some((before, _after)) => before.trim(),
        None => text,
    };
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
}

fn preview(value: &str, max: usize) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_do_chat_test_and_time() {
        assert_eq!(
            clean("问候do_chat_test1793785051554\n\n系统信息do_chat_test1793785051621"),
            "问候\n\n系统信息"
        );
    }

    #[test]
    fn keeps_plain_segments() {
        assert_eq!(clean("普通结果"), "普通结果");
        assert_eq!(clean("第一段\n\n第二段"), "第一段\n\n第二段");
    }

    #[test]
    fn drops_empty_segments() {
        assert_eq!(clean("\n\n问候do_chat_test1\n\n"), "问候");
        assert_eq!(clean("do_chat_test123"), "");
    }
}
