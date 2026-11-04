// src/utils/plugins.rs
//! 插件阶段：查询 kv_lib 的 `chat_plugin` 分类，并调用命中的插件。
//!
//! 输入是 think 阶段按 `|` 分割后选出的查询词；输出是插件返回的 `content`，
//! 交给 `reply` 阶段组装最终回复。
//!
//! 调用方式全部走本地动态库：
//! - kv：`utils::kv` → `./libary/kv_lib.dll`
//! - 插件：`designplugin_lib` 安装到 `./plugins/<name>/`，这里按该约定 dlopen。

use anyhow::{bail, Context, Result};
use libloading::Library;
use serde_json::{json, Value};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::PathBuf;

use crate::utils::cloud::Supplier;
use crate::utils::kv;

/// 插件检索命中数量。
pub const TOP_K: i64 = 5;
/// 插件未指定符号时的默认入口。
const DEFAULT_SYMBOL: &str = "chat";
/// 与插件 `write_json_out` 保持一致的输出缓冲。
const OUT_BUF_SIZE: usize = 65536;

#[cfg(windows)]
const DYLIB_EXT: &str = "dll";
#[cfg(target_os = "linux")]
const DYLIB_EXT: &str = "so";
#[cfg(target_os = "macos")]
const DYLIB_EXT: &str = "dylib";

type FfiFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

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

/// 处理一条查询，返回插件结果或命中内容的回退文本。
pub fn handle(input: &str) -> Result<String> {
    println!("[plugins] handle 输入: {:?}", preview(input, 300));
    let list = match kv::search_chat_plugins(input, TOP_K) {
        Ok(list) => list,
        Err(error) => {
            println!("[plugins] chat_plugin 查询失败: {:#}", error);
            return Err(error);
        }
    };
    println!("[plugins] chat_plugin 命中 {} 条", list.len());
    for (index, hit) in list.iter().enumerate() {
        println!(
            "[plugins] hit[{}] key={:?}, score={}, meta={}, value={:?}",
            index,
            hit.get("key").and_then(Value::as_str),
            hit.get("score").and_then(Value::as_f64).unwrap_or(0.0),
            hit.get("meta")
                .map(|v| preview(&v.to_string(), 300))
                .unwrap_or_else(|| "null".to_string()),
            hit.get("value")
                .and_then(Value::as_str)
                .map(|v| preview(v, 300))
        );
    }

    let Some(hit) = list.first() else {
        println!("[plugins] 未命中插件，返回回退文本");
        return Ok(format!("未找到相关对话内容：{input}"));
    };

    if let Some((plugin, symbol)) = plugin_of(hit) {
        println!(
            "[plugins] 选中插件: plugin={:?}, symbol={:?}",
            plugin, symbol
        );
        return call_plugin(&plugin, &symbol, input);
    }

    let fallback = content_of(hit).unwrap_or_else(|| format!("未找到相关对话内容：{input}"));
    println!(
        "[plugins] 命中记录无插件，返回内容: {:?}",
        preview(&fallback, 300)
    );
    Ok(fallback)
}

/// 逐段查询并执行插件；返回每段的结果文本。
///
/// think 阶段按 `|` 拆分出的数组直接传入这里；单段失败不会阻断其它段，
/// 错误文本也会作为该段结果保留，最终统一交给 reply。
pub fn handle_many(queries: &[&str]) -> Vec<String> {
    println!("[plugins] handle_many 输入数组: {:?}", queries);
    let results = queries
        .iter()
        .enumerate()
        .map(|(index, query)| {
            println!(
                "[plugins] 处理第 {} 段: {:?}",
                index + 1,
                preview(query, 200)
            );
            match handle(query) {
                Ok(content) => {
                    println!(
                        "[plugins] 第 {} 段结果: {:?}",
                        index + 1,
                        preview(&content, 300)
                    );
                    content
                }
                Err(error) => {
                    println!("[plugins] 第 {} 段失败 ({query}): {:#}", index + 1, error);
                    format!("插件处理失败：{:#}", error)
                }
            }
        })
        .collect::<Vec<_>>();
    println!("[plugins] handle_many 完成，共 {} 段结果", results.len());
    results
}

/// 合并多段插件结果；空结果返回 `None`。
pub fn join_results(results: &[String]) -> Option<String> {
    let text = results
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.is_empty() {
        println!("[plugins] join_results: 无有效结果");
        None
    } else {
        println!("[plugins] join_results: {:?}", preview(&text, 500));
        Some(text)
    }
}

/// 从命中记录里解析插件：`meta.plugin`，或 `value` 里的 `lib_name`。
pub fn plugin_of(hit: &Value) -> Option<(String, String)> {
    if let Some(meta) = hit.get("meta") {
        if let Some(plugin) = meta.get("plugin").and_then(Value::as_str) {
            let symbol = meta
                .get("ffi_symbol")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_SYMBOL);
            return Some((plugin.to_string(), symbol.to_string()));
        }
    }

    let value = hit.get("value").and_then(Value::as_str)?;
    let meta: Value = serde_json::from_str(value).ok()?;
    let plugin = meta.get("lib_name").and_then(Value::as_str)?;
    let symbol = meta
        .get("ffi_symbol")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_SYMBOL);
    Some((plugin.to_string(), symbol.to_string()))
}

/// 取命中记录里的非空 `value`，用于没有插件时的回退文本。
pub fn content_of(hit: &Value) -> Option<String> {
    hit.get("value")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// 按 `designplugin_lib` 的安装约定探测插件动态库。
fn candidates(plugin: &str) -> Vec<PathBuf> {
    vec![
        PathBuf::from(format!("./plugins/{0}/{0}.{1}", plugin, DYLIB_EXT)),
        PathBuf::from(format!("./plugins/{0}/lib{0}.{1}", plugin, DYLIB_EXT)),
        PathBuf::from(format!("./libary/{0}.{1}", plugin, DYLIB_EXT)),
        PathBuf::from(format!("./libary/lib{0}.{1}", plugin, DYLIB_EXT)),
    ]
}

/// 执行插件：dlopen 后调用 `post_<symbol>`，返回响应 `data.content`。
pub fn call_plugin(plugin: &str, symbol: &str, input: &str) -> Result<String> {
    println!(
        "[plugins] call_plugin: plugin={:?}, symbol={:?}, input={:?}",
        plugin,
        symbol,
        preview(input, 200)
    );
    // 供应商插件统一走 cloud，不再由插件各自 dlopen 供应商库。
    if let Some(supplier) = Supplier::from_exact_code(plugin) {
        println!("[plugins] 供应商插件走 cloud: {:?}", supplier);
        let reply = match crate::utils::cloud::chat_with(supplier, input, &[]) {
            Ok(reply) => reply,
            Err(error) => {
                println!("[plugins] cloud 调用失败: {:#}", error);
                return Err(error);
            }
        };
        println!(
            "[plugins] cloud 返回: content={:?}, model={:?}",
            preview(&reply.content, 300),
            reply.model
        );
        return Ok(reply.content);
    }

    let path = candidates(plugin)
        .into_iter()
        .find(|path| path.exists())
        .with_context(|| format!("找不到插件 {plugin}（./plugins/{plugin}/ 或 ./libary/）"))?;
    println!("[plugins] 插件路径: {}", path.display());

    let lib = unsafe { Library::new(&path) }
        .with_context(|| format!("加载插件 {} 失败", path.display()))?;
    let entry = format!("post_{}", symbol);
    let function: libloading::Symbol<'_, FfiFn> = unsafe {
        lib.get(entry.as_bytes())
            .with_context(|| format!("dlsym {} 失败（{}）", entry, path.display()))?
    };

    let request = json!({ "prompt": input }).to_string();
    println!("[plugins] 插件请求: {:?}", preview(&request, 400));
    let req = CString::new(request).context("插件请求 JSON 含 \\0")?;
    let mut out = vec![0u8; OUT_BUF_SIZE];
    let ret = unsafe { function(req.as_ptr(), out.as_mut_ptr() as *mut c_char) };
    println!("[plugins] 插件 FFI 返回: {}", ret);
    if ret == -2 {
        bail!("插件 {} 输出超过 64 KiB", plugin);
    }
    if ret != 0 {
        bail!("插件 {} FFI 返回 {}", plugin, ret);
    }

    let out = unsafe { CStr::from_ptr(out.as_ptr() as *const c_char) }
        .to_str()
        .context("插件响应非 UTF-8")?;
    println!("[plugins] 插件原始响应: {:?}", preview(out, 500));
    let value: Value = serde_json::from_str(out)
        .with_context(|| format!("插件 {} 响应解析失败: {}", plugin, out))?;

    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 && code != 200 {
        let msg = value
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        println!("[plugins] 插件错误: code={}, msg={}", code, msg);
        bail!("插件 {} 错误 (code={}): {}", plugin, code, msg);
    }

    let content = value
        .get("data")
        .and_then(|data| data.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    println!("[plugins] 插件 content: {:?}", preview(&content, 300));
    Ok(content)
}
