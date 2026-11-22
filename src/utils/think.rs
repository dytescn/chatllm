// src/utils/think.rs
//! 思考阶段：用固定关键词 `base` 查询 kv_lib 的 `chat_think` 分类，
//! 命中后执行插件回调，结果作为 think 输出交给 `plugins` 阶段。

use crate::utils::kv;
use crate::utils::plugins_callback;
use crate::utils::plugins_lib;
use crate::utils::time::now_ms;
use serde_json::{json, Value};

/// `chat_think` 分类：思考阶段查询的 kv 集合。
pub const COLLECTION: &str = "chat_think";

/// think 阶段的默认分类检索关键词。
pub const INIT_KEYWORD: &str = "base";
/// think 检索阈值：100% 匹配。
pub const INIT_MIN_SCORE: f64 = 1.0;

/// 思考检索默认返回条数。
pub const TOP_K: i64 = 5;

/// 一条命中的 think 记录。
#[derive(Debug, Clone, Default)]
pub struct ThinkHit {
    pub value: Option<String>,
    /// 插件 uuid；用于读取 skills 与查询插件调用参数。
    pub plugins_uid: Option<String>,
    /// think 返回的提示词数组。
    pub prompts: Vec<String>,
    /// think 返回的输出数组。
    pub outputs: Vec<String>,
}

/// think 阶段结果。
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
    /// 命中记录的插件 uuid。
    pub plugins_uid: Option<String>,
}

impl ThinkOutcome {
    /// 生成轮次记录里的 `think` 段。
    pub fn to_round_json(&self) -> Value {
        json!({
            "start_time": self.start_time,
            "input": { "content": self.input },
            "prompts": self.prompts.clone(),
            "calls": [],
            "outputs": self.outputs.clone(),
            "end_time": self.end_time,
            "status": self.status,
        })
    }
}

/// 执行 think 阶段：计时 → 检索 `chat_think` → 调用插件 → 汇总结果。
pub fn think_run(text: &str, top_k: i64) -> ThinkOutcome {
    let start_time = now_ms();
    println!("[think] run: text={:?}, top_k={}", text, top_k);

    let (status, hit) = match find(text, top_k) {
        Ok(Some(hit)) => ("ok", Some(hit)),
        Ok(None) => ("skipped", None),
        Err(error) => {
            println!("[think] 检索失败: {:#}", error);
            ("error", None)
        }
    };

    let (prompts, outputs, plugins_uid) = hit
        .map(|hit| (hit.prompts, hit.outputs, hit.plugins_uid))
        .unwrap_or_default();
    let end_time = now_ms();
    println!(
        "[think] 结束: status={}, prompts={}, outputs={}, 耗时 {} ms",
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
        plugins_uid,
    }
}

/// 检索 `chat_think` 首条命中，命中后执行插件并合并结果。
pub fn find(text: &str, top_k: i64) -> anyhow::Result<Option<ThinkHit>> {
    let data = kv::search(COLLECTION, INIT_KEYWORD, top_k, INIT_MIN_SCORE, None, true)?;
    let Some(hit) = kv::take_list(data).into_iter().next() else {
        println!("[think] 未命中 chat_think");
        return Ok(None);
    };

    let mut parsed = parse_hit(&hit);
    println!("[think] 选中: {:?}", parsed);

    if let Some(uid) = parsed.plugins_uid.as_deref() {
        match plugins_lib::plugin_params(uid) {
            Some(params) => {
                let request = plugin_request(text);
                match plugins_callback::call_plugin_response(
                    &params.plugin,
                    &params.ffi_symbol,
                    &request,
                ) {
                    Ok(response) => {
                        println!("[think] 回调插件 {} 成功", params.plugin);
                        apply_plugin_response(&mut parsed, &response);
                    }
                    Err(error) => println!("[think] 回调插件 {} 失败: {:#}", params.plugin, error),
                }
            }
            None => println!("[think] 未找到 plugins_uid={} 对应的插件参数", uid),
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

/// 解析命中记录：只取 `value` 与统一的 `plugins_uid`。
pub fn parse_hit(hit: &Value) -> ThinkHit {
    let meta = hit.get("meta");
    let value = hit.get("value").and_then(Value::as_str).map(str::to_string);
    let value_json = value
        .as_deref()
        .and_then(|text| serde_json::from_str::<Value>(text).ok());

    let plugins_uid = meta
        .and_then(|m| m.get("plugins_uid"))
        .or_else(|| value_json.as_ref().and_then(|v| v.get("plugins_uid")))
        .and_then(Value::as_str)
        .map(str::to_string);

    ThinkHit {
        value: value.clone(),
        plugins_uid,
        prompts: strings_at(meta, "prompts")
            .or_else(|| strings_at(value_json.as_ref(), "prompts"))
            .or_else(|| strings_at(value_json.as_ref(), "prompt"))
            .unwrap_or_else(|| value.clone().into_iter().collect()),
        outputs: value.clone().into_iter().collect(),
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
