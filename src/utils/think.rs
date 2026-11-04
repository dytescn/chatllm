// src/utils/think.rs
//! 思考阶段：查询 kv_lib 的 `chat_think` 分类。
//!
//! 命中内容可用 `|` 分成多段；对话流程会把所有非空段作为数组，
//! 分别查询 `chat_plugin` 并执行插件，最后把结果交给 `reply`。

use crate::utils::kv;
use serde_json::Value;

/// `chat_think` 分类：思考阶段查询的 kv 集合。
pub const COLLECTION: &str = "chat_think";

/// 思考内容的分段分隔符。
pub const SEPARATOR: &str = "|";

/// 思考检索默认返回条数。
pub const TOP_K: i64 = 5;

/// 一条命中的 think 记录。
#[derive(Debug, Clone, Default)]
pub struct ThinkHit {
    pub key: Option<String>,
    pub value: Option<String>,
    /// think_base 等资源包的 uuid，用作 dialogue id / plugin_uuid。
    pub plugin_uuid: Option<String>,
    /// 资源包名，用于定位 `./plugins/<name>/<name>.md`。
    pub plugin_name: Option<String>,
}

/// 截断长文本，避免日志刷屏。
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

/// 在 `chat_think` 分类中检索，返回首条命中的完整记录。
pub fn find(input: &str, top_k: i64) -> anyhow::Result<Option<ThinkHit>> {
    println!(
        "[think] find 输入: query={:?}, top_k={}",
        preview(input, 200),
        top_k
    );
    let data = match kv::search(COLLECTION, input, top_k, 0.0, None, true) {
        Ok(data) => data,
        Err(error) => {
            println!("[think] kv 查询失败: {:#}", error);
            return Err(error);
        }
    };
    let list = data
        .get("list")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    println!("[think] kv 返回 {} 条", list.len());
    for (index, hit) in list.iter().enumerate() {
        println!(
            "[think] hit[{}] key={:?}, score={}, value={:?}, meta={}",
            index,
            hit.get("key").and_then(Value::as_str),
            hit.get("score").and_then(Value::as_f64).unwrap_or(0.0),
            hit.get("value")
                .and_then(Value::as_str)
                .map(|v| preview(v, 300)),
            hit.get("meta")
                .map(|v| preview(&v.to_string(), 400))
                .unwrap_or_else(|| "null".to_string())
        );
    }

    let Some(hit) = list.first() else {
        println!("[think] 未命中 chat_think");
        return Ok(None);
    };
    let parsed = parse_hit(hit);
    println!(
        "[think] 选中: key={:?}, value={:?}, plugin_uuid={:?}, plugin_name={:?}",
        parsed.key,
        parsed.value.as_deref().map(|v| preview(v, 200)),
        parsed.plugin_uuid,
        parsed.plugin_name
    );
    Ok(Some(parsed))
}

/// 解析命中记录：`meta.plugin_uuid` / `meta.plugin_name`，兼容 value JSON。
pub fn parse_hit(hit: &Value) -> ThinkHit {
    let meta = hit.get("meta");
    let value = hit.get("value").and_then(Value::as_str).map(str::to_string);
    let value_json = value
        .as_deref()
        .and_then(|text| serde_json::from_str::<Value>(text).ok());

    let plugin_uuid = meta
        .and_then(|m| m.get("plugin_uuid"))
        .and_then(Value::as_str)
        .or_else(|| hit.get("plugin_uuid").and_then(Value::as_str))
        .or_else(|| {
            value_json
                .as_ref()
                .and_then(|v| v.get("plugin_uuid"))
                .and_then(Value::as_str)
        })
        .map(str::to_string);

    let plugin_name = meta
        .and_then(|m| m.get("plugin_name"))
        .and_then(Value::as_str)
        .or_else(|| meta.and_then(|m| m.get("plugin")).and_then(Value::as_str))
        .or_else(|| {
            value_json
                .as_ref()
                .and_then(|v| v.get("plugin_name"))
                .and_then(Value::as_str)
        })
        .map(str::to_string);

    ThinkHit {
        key: hit.get("key").and_then(Value::as_str).map(str::to_string),
        value,
        plugin_uuid,
        plugin_name,
    }
}

/// 读取资源包的 skills 文档：`./plugins/<name>/<name>.md`。
pub fn skills_for(plugin_name: &str) -> Option<String> {
    let name = plugin_name.trim();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        println!("[think] skills_for 非法 plugin_name: {:?}", plugin_name);
        return None;
    }
    let path = format!("./plugins/{0}/{0}.md", name);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let text = text.trim().to_string();
            if text.is_empty() {
                println!("[think] skills 文件为空: {}", path);
                None
            } else {
                println!("[think] skills 读取成功: {} ({} chars)", path, text.len());
                Some(text)
            }
        }
        Err(error) => {
            println!("[think] skills 读取失败: {} ({})", path, error);
            None
        }
    }
}

/// 在 `chat_think` 分类中检索，返回首条命中的 `value`。
pub fn query(input: &str, top_k: i64) -> anyhow::Result<Option<String>> {
    let value = find(input, top_k)?
        .and_then(|hit| hit.value)
        .filter(|text| !text.trim().is_empty());
    println!(
        "[think] query 结果: {:?}",
        value.as_deref().map(|v| preview(v, 300))
    );
    Ok(value)
}

/// 按 [`SEPARATOR`] 拆分思考内容，去掉首尾空白与空段。
pub fn segments(content: &str) -> Vec<&str> {
    let segments = content
        .split(SEPARATOR)
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    println!(
        "[think] 拆分: content={:?} -> {:?}",
        preview(content, 300),
        segments
    );
    segments
}

/// 取最后一段非空内容；没有有效段时返回 `None`。
pub fn last_segment(content: &str) -> Option<&str> {
    segments(content).last().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_and_keeps_non_empty_segments() {
        assert_eq!(
            segments("问候|系统信息|默认"),
            vec!["问候", "系统信息", "默认"]
        );
        assert_eq!(segments(" a | | b | "), vec!["a", "b"]);
        assert_eq!(segments("没有分隔符"), vec!["没有分隔符"]);
    }

    #[test]
    fn picks_last_non_empty_segment() {
        assert_eq!(last_segment("问候|系统信息|默认"), Some("默认"));
        assert_eq!(last_segment("问候|系统信息|  "), Some("系统信息"));
        assert_eq!(last_segment("没有分隔符"), Some("没有分隔符"));
        assert_eq!(last_segment(" | | "), None);
        assert_eq!(last_segment(""), None);
    }

    #[test]
    fn parses_plugin_info_from_meta() {
        let hit = serde_json::json!({
            "key": "think-base",
            "value": "系统信息助手",
            "meta": {
                "plugin_uuid": "9a7f3c2e-5b1d-4e8a-9c6f-2d4b8a1e7c30",
                "plugin_name": "think_base"
            }
        });
        let parsed = parse_hit(&hit);
        assert_eq!(parsed.value.as_deref(), Some("系统信息助手"));
        assert_eq!(
            parsed.plugin_uuid.as_deref(),
            Some("9a7f3c2e-5b1d-4e8a-9c6f-2d4b8a1e7c30")
        );
        assert_eq!(parsed.plugin_name.as_deref(), Some("think_base"));
    }

    #[test]
    fn parses_plugin_info_from_value_json() {
        let hit = serde_json::json!({
            "key": "think-base",
            "value": "{\"plugin_uuid\":\"uuid\",\"plugin_name\":\"think_base\"}"
        });
        let parsed = parse_hit(&hit);
        assert_eq!(parsed.plugin_uuid.as_deref(), Some("uuid"));
        assert_eq!(parsed.plugin_name.as_deref(), Some("think_base"));
    }
}
