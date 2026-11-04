// src/utils/check.rs
//! 对话前置检查：确认 `info.db.setting` 中已选择 AI 供应商并配置 AppKey。
//!
//! 配置由 `setting_ui` 通过 `info_lib.info_*` 写入 `setting` 表，
//! chatllm 直接读取同一张表。供应商或对应 AppKey 缺失（缺行、空值、未知值）时，
//! 所有对话都返回统一提示，并附带指向 `/setting` 的链接。

use std::fs;
use std::sync::Mutex;
use std::time::SystemTime;

use anyhow::Result;

use crate::utils::cloud::{self, Supplier};

/// 配置检查成功缓存：记录 info.db 的修改时间与大小。
///
/// 文件未变化时直接复用成功结果，避免每次对话重复读取 setting；
/// setting 写入后 info.db 的 mtime/length 变化，缓存自动失效。
static READY_CACHE: Mutex<Option<(SystemTime, u64)>> = Mutex::new(None);

/// `info.db` 的轻量签名。
fn info_signature() -> Option<(SystemTime, u64)> {
    let meta = fs::metadata("./database/info.db").ok()?;
    let modified = meta.modified().ok()?;
    Some((modified, meta.len()))
}

fn cache_matches(signature: &Option<(SystemTime, u64)>) -> bool {
    let cache = match READY_CACHE.lock() {
        Ok(cache) => cache,
        Err(_) => return false,
    };
    match (&*cache, signature) {
        (Some(cached), Some(current)) => cached == current,
        _ => false,
    }
}

fn set_cache(signature: &Option<(SystemTime, u64)>) {
    if let Some(signature) = signature {
        if let Ok(mut cache) = READY_CACHE.lock() {
            *cache = Some(signature.clone());
        }
    }
}

/// `info.db.setting` 中保存 AI 供应商的键。
pub const AI_SUPPLIER_KEY: &str = "ai_supplier";
/// 设置页面路由。
pub const SETTING_ROUTE: &str = "/setting";
/// 未配置供应商或 AppKey 时的统一回复。
///
/// 消息里的 Markdown 链接由 markparse 渲染成 `<a href="/setting">`。
pub const MISSING_SUPPLIER_MESSAGE: &str = "请选择设置AI供应商并配置 AppKey：[前往设置](/setting)";

/// 解析 `ai_supplier` 的值；空值或未知供应商返回 `None`。
///
/// 这里不复用 `Supplier::from_code`：该函数对未知值回退 `deepseek`，
/// 会让“未配置”被误判为已配置。
pub fn parse_supplier(value: Option<&str>) -> Option<Supplier> {
    let parsed = value.and_then(|value| Supplier::from_exact_code(value.trim()));
    println!("[check] parse_supplier: {:?} -> {:?}", value, parsed);
    parsed
}

/// 当前配置的有效供应商；未选择或未知值返回 `None`。
pub fn configured_supplier() -> Result<Option<Supplier>> {
    let value = cloud::setting(AI_SUPPLIER_KEY)?;
    let parsed = parse_supplier(value.as_deref());
    println!("[check] configured_supplier: {:?}", parsed);
    Ok(parsed)
}

/// 当前是否已选择有效的 AI 供应商。
pub fn supplier_configured() -> Result<bool> {
    let configured = configured_supplier()?.is_some();
    println!("[check] supplier_configured: {}", configured);
    Ok(configured)
}

/// 指定供应商的 AppKey 是否已配置。
pub fn token_configured(supplier: Supplier) -> bool {
    let configured = cloud::token(supplier).is_ok();
    println!(
        "[check] token_configured: supplier={:?}, configured={}",
        supplier, configured
    );
    configured
}

/// 对话前置检查：供应商与 AppKey 都已配置返回 `None`，否则返回统一提示。
///
/// 读取配置失败（如 `info.db` 尚未初始化）同样按未配置处理，
/// 保证对话仍能返回可操作的提示，而不是底层错误。
pub fn missing_supplier_reply() -> Option<String> {
    let signature = info_signature();
    if cache_matches(&signature) {
        println!("[check] 使用缓存，跳过 setting 读取");
        return None;
    }

    let reply = match configured_supplier() {
        Ok(Some(supplier)) => {
            if token_configured(supplier) {
                None
            } else {
                println!(
                    "[check] 未配置 {}：{}",
                    supplier.code(),
                    supplier.token_key()
                );
                Some(MISSING_SUPPLIER_MESSAGE.to_string())
            }
        }
        Ok(None) => Some(MISSING_SUPPLIER_MESSAGE.to_string()),
        Err(error) => {
            println!("[check] 读取 {} 失败: {:#}", AI_SUPPLIER_KEY, error);
            Some(MISSING_SUPPLIER_MESSAGE.to_string())
        }
    };
    if reply.is_none() {
        set_cache(&signature);
        println!("[check] 配置检查成功，已写入缓存");
    }
    println!("[check] missing_supplier_reply: {:?}", reply);
    reply
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_suppliers() {
        assert_eq!(parse_supplier(Some("deepseek")), Some(Supplier::Deepseek));
        assert_eq!(
            parse_supplier(Some(" siliconflow_t2t ")),
            Some(Supplier::SiliconFlow)
        );
    }

    #[test]
    fn unset_or_unknown_is_not_configured() {
        assert_eq!(parse_supplier(None), None);
        assert_eq!(parse_supplier(Some("")), None);
        assert_eq!(parse_supplier(Some("   ")), None);
        assert_eq!(parse_supplier(Some("siliconflow")), None);
        assert_eq!(parse_supplier(Some("openai")), None);
    }

    #[test]
    fn missing_reply_points_to_setting_route() {
        assert!(MISSING_SUPPLIER_MESSAGE.contains("请选择设置AI供应商"));
        assert!(MISSING_SUPPLIER_MESSAGE.contains(SETTING_ROUTE));
    }
}
