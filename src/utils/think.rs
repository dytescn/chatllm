// src/utils/think.rs
//! 思考阶段：用固定关键词 `init` 查询 kv_lib 的 `chat_think` 分类，
//! 命中后执行插件回调，回调结果作为 think 输出交给 `plugins` 阶段。

use crate::utils::chat_plugins;
use crate::utils::designdb;
use crate::utils::kv;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

/// `chat_think` 分类：思考阶段查询的 kv 集合。
pub const COLLECTION: &str = "chat_think";

/// think 阶段的默认分类检索关键词。
pub const INIT_KEYWORD: &str = "base";
/// think 检索阈值：100% 匹配。
pub const INIT_MIN_SCORE: f64 = 1.0;

/// 思考内容的分段分隔符。
pub const SEPARATOR: &str = "|";
/// 思考内容的第二个分段分隔符。
pub const SEPARATOR_COMMA: &str = ",";

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
    /// 可执行插件名（`meta.plugin` / `lib_name`）。
    pub plugin: Option<String>,
    /// 插件回调方法，默认 `chat`。
    pub ffi_symbol: Option<String>,
    /// think 返回的提示词数组。
    pub prompts: Vec<String>,
    /// think 返回的输出数组。
    pub outputs: Vec<String>,
}

/// think 阶段的对外结果：`prompts` 与 `outputs` 两个数组。
///
/// 同时保留 `start_time` / `end_time` 供轮次记录落库；`hit` 用于读取
/// `plugin_uuid` / `plugin_name`（如加载技能正文）。
#[derive(Debug, Clone, Default)]
pub struct ThinkOutcome {
    pub start_time: i64,
    pub end_time: i64,
    /// `ok` / `skipped` / `error`。
    pub status: &'static str,
    /// 本阶段输入的用户文本。
    pub input: String,
    /// 插件返回或命中记录携带的提示词数组。
    pub prompts: Vec<String>,
    /// 插件返回的输出数组。
    pub outputs: Vec<String>,
    /// 首条命中记录，未命中时为 `None`。
    pub hit: Option<ThinkHit>,
}

/// 执行 think 阶段：计时 → 检索 `chat_think` → 调用插件 → 汇总 prompts / outputs。
///
/// 检索失败不会中断对话，结果以 `status = "error"` 返回；调用方可直接把
/// `start_time` / `end_time` / `prompts` / `outputs` 写入轮次记录。
pub fn run(text: &str, top_k: i64) -> ThinkOutcome {
    let start_time = now_ms();
    println!(
        "[think] run 开始: text={:?}, top_k={}, start_time={}",
        preview(text, 200),
        top_k,
        start_time
    );

    let (status, hit) = match find(text, top_k) {
        Ok(Some(hit)) => ("ok", Some(hit)),
        Ok(None) => ("skipped", None),
        Err(error) => {
            println!("[think] run 检索失败: {:#}", error);
            ("error", None)
        }
    };

    let prompts = hit
        .as_ref()
        .map(|hit| hit.prompts.clone())
        .unwrap_or_default();
    let outputs = hit
        .as_ref()
        .map(|hit| hit.outputs.clone())
        .unwrap_or_default();
    let end_time = now_ms();
    println!(
        "[think] run 结束: status={}, prompts={}, outputs={}, 耗时 {} ms",
        status,
        prompts.len(),
        outputs.len(),
        end_time - start_time
    );

    ThinkOutcome {
        start_time,
        end_time,
        status,
        input: text.to_string(),
        prompts,
        outputs,
        hit,
    }
}

/// 用固定关键词 `init` 在 `chat_think` 中检索，命中后把当前文本与空的
/// `prompts` / `outputs` 传给插件。
pub fn find(text: &str, top_k: i64) -> anyhow::Result<Option<ThinkHit>> {
    println!(
        "[think] find: query={:?}, text={:?}, top_k={}, min_score={}",
        INIT_KEYWORD,
        preview(text, 200),
        top_k,
        INIT_MIN_SCORE
    );
    let data = match kv::search(COLLECTION, INIT_KEYWORD, top_k, INIT_MIN_SCORE, None, true) {
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
            hit.get("value").and_then(Value::as_str),
            hit.get("meta")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string())
        );
    }

    let Some(hit) = list.first() else {
        println!("[think] 未命中 chat_think");
        return Ok(None);
    };
    let mut parsed = parse_hit(hit);
    println!(
        "[think] 选中: key={:?}, value={:?}, plugin_uuid={:?}, plugin_name={:?}, plugin={:?}",
        parsed.key,
        parsed.value.as_deref(),
        parsed.plugin_uuid,
        parsed.plugin_name,
        parsed.plugin
    );

    if let Some(plugin) = parsed.plugin.as_deref() {
        let symbol = parsed.ffi_symbol.as_deref().unwrap_or("chat");
        let request = plugin_request(text);
        match chat_plugins::call_plugin_response(plugin, symbol, &request) {
            Ok(response) => {
                println!("[think] 回调插件 {} 成功", plugin);
                apply_plugin_response(&mut parsed, &response);
            }
            Err(error) => println!(
                "[think] 回调插件 {} 失败，保留命中内容: {:#}",
                plugin, error
            ),
        }
    }

    Ok(Some(parsed))
}

/// think 阶段传给插件的请求：当前文本 + 空的 `prompts` / `outputs` 数组。
pub fn plugin_request(text: &str) -> Value {
    json!({
        "prompt": text,
        "prompts": [],
        "outputs": []
    })
}

/// 用插件响应更新 `prompts` / `outputs`。
///
/// 优先读取 `data.prompts` / `data.outputs`；旧插件只返回 `data.content` 时，
/// 把该内容作为单条 output，并同步 `value` 供后续阶段使用。
pub fn apply_plugin_response(hit: &mut ThinkHit, response: &Value) {
    let data = response.get("data");
    if let Some(prompts) = strings_at(data, "prompts") {
        hit.prompts = prompts;
    }

    if let Some(outputs) = strings_at(data, "outputs") {
        hit.outputs = outputs;
    } else if let Some(content) = data
        .and_then(|data| data.get("content"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|content| !content.is_empty())
    {
        hit.outputs = vec![content.to_string()];
    }

    if let Some(first) = hit.outputs.first() {
        hit.value = Some(first.clone());
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// 截断长文本，避免日志刷屏。
pub fn preview(value: &str, max: usize) -> String {
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

    let plugin = meta
        .and_then(|m| m.get("plugin"))
        .and_then(Value::as_str)
        .or_else(|| meta.and_then(|m| m.get("lib_name")).and_then(Value::as_str))
        .or_else(|| {
            value_json
                .as_ref()
                .and_then(|v| v.get("lib_name"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            value_json
                .as_ref()
                .and_then(|v| v.get("plugin"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            value_json
                .as_ref()
                .and_then(|v| v.get("name"))
                .and_then(Value::as_str)
        })
        .map(str::to_string);

    let ffi_symbol = meta
        .and_then(|m| m.get("ffi_symbol"))
        .and_then(Value::as_str)
        .or_else(|| {
            value_json
                .as_ref()
                .and_then(|v| v.get("ffi_symbol"))
                .and_then(Value::as_str)
        })
        .map(str::to_string);

    let prompts = strings_at(meta, "prompts")
        .or_else(|| strings_at(value_json.as_ref(), "prompts"))
        .or_else(|| strings_at(value_json.as_ref(), "prompt"))
        .unwrap_or_else(|| value.clone().into_iter().collect());
    let outputs = value.clone().into_iter().collect();

    ThinkHit {
        key: hit.get("key").and_then(Value::as_str).map(str::to_string),
        value,
        plugin_uuid,
        plugin_name,
        plugin,
        ffi_symbol,
        prompts,
        outputs,
    }
}

/// 读取对象里的字符串数组字段。
fn strings_at(value: Option<&Value>, key: &str) -> Option<Vec<String>> {
    let list = value?.get(key)?.as_array()?;
    let items = list
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

/// designplugin_lib 的技能集合。
const SKILLS_PATH: &str = "skills";
const SKILLS_COLLECTION: &str = "skills";

/// 从 designplugin_lib 的 skills 集合读取技能正文。
///
/// 优先按 `plugin_uuid` 查询；没有 uuid 时按技能 `_id` / `name` 查询。
pub fn skills_for(plugin_uuid: Option<&str>, plugin_name: Option<&str>) -> Option<String> {
    let uuid = plugin_uuid.map(str::trim).filter(|value| !value.is_empty());
    let name = plugin_name.map(str::trim).filter(|value| !value.is_empty());

    let mut docs = Vec::new();
    if let Some(uuid) = uuid {
        match designdb::query(
            SKILLS_PATH,
            SKILLS_COLLECTION,
            Some(json!({ "plugin_uuid": uuid })),
            Some(json!({ "sort": 1 })),
            None,
            Some(100),
        ) {
            Ok(items) => docs = items,
            Err(error) => println!("[think] skills 查询失败: {:#}", error),
        }
    }

    if docs.is_empty() {
        if let Some(name) = name {
            for filter in [json!({ "_id": name }), json!({ "name": name })] {
                match designdb::query(
                    SKILLS_PATH,
                    SKILLS_COLLECTION,
                    Some(filter),
                    None,
                    None,
                    Some(1),
                ) {
                    Ok(items) if !items.is_empty() => {
                        docs = items;
                        break;
                    }
                    Ok(_) => {}
                    Err(error) => println!("[think] skills 查询失败: {:#}", error),
                }
            }
        }
    }

    let content = docs
        .iter()
        .filter_map(|doc| doc.get("content").and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    if content.is_empty() {
        println!("[think] skills 未找到: uuid={:?}, name={:?}", uuid, name);
        None
    } else {
        println!("[think] skills 读取成功: {} chars", content.len());
        Some(content)
    }
}

/// 在 `chat_think` 分类中检索，返回首条命中的 `value`。
pub fn query(text: &str, top_k: i64) -> anyhow::Result<Option<String>> {
    let value = find(text, top_k)?
        .and_then(|hit| hit.value)
        .filter(|text| !text.trim().is_empty());
    println!("[think] query 结果: {:?}", value.as_deref());
    Ok(value)
}

/// 按 [`SEPARATOR`] 或 [`SEPARATOR_COMMA`] 拆分思考内容，去掉首尾空白与空段。
pub fn segments(content: &str) -> Vec<&str> {
    let segments = content
        .split(['|', ','])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    println!("[think] 拆分: content={:?} -> {:?}", content, segments);
    segments
}

/// 取最后一段非空内容；没有有效段时返回 `None`。
pub fn last_segment(content: &str) -> Option<&str> {
    segments(content).last().copied()
}
