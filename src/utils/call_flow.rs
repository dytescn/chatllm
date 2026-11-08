// src/utils/call_flow.rs
//! 直接调用插件流程（`post_call_chat`）。
//!
//! 与 [`run_flow`](crate::utils::run_flow) 不同，这里不执行 think_base 与
//! reply：
//!
//! 1. 用 `category` 作为 kv 集合名、`prompts` 作为检索词，取 Top1 插件；
//! 2. 把 `{plugins_uid, category, prompts, context}` 原样交给插件；
//! 3. 返回插件响应中的 `data`。
//!
//! 不向宿主推送阶段事件。

use crate::ffi::types::CallChatReq;
use crate::utils::errcode::ChatError;
use crate::utils::{kv, plugins};
use serde_json::{json, Value};

/// 分类内检索的候选数量，与正常流程的 plugins 阶段一致。
pub const TOP_K: i64 = plugins::TOP_K;

/// 在分类中定位插件并直接调用，返回插件响应的 `data`。
pub fn run(req: &CallChatReq) -> Result<Value, ChatError> {
    let category = req.category.trim();
    if category.is_empty() {
        return Err(ChatError::Plugin("category is required".to_string()));
    }

    // 重入保护：插件回调 chatllm 时不再进入 kv 检索与插件调用，
    // 避免递归加载动态库、以及 kv / 插件引擎锁互相等待。
    if plugins::plugin_call_active() {
        return Err(ChatError::Plugin(
            "检测到插件重入调用（当前线程已在执行插件）".to_string(),
        ));
    }

    println!(
        "[call_flow] category={:?}, plugins_uid={:?}, prompts={:?}",
        category, req.plugins_uid, req.prompts
    );

    // 分类中检索插件：category 即 kv 集合名。
    let data = kv::search(
        category,
        &req.prompts,
        TOP_K,
        kv::match_min_score(),
        None,
        true,
    )
    .map_err(|error| ChatError::Plugin(format!("kv 检索失败: {:#}", error)))?;
    let hit = data
        .get("list")
        .and_then(Value::as_array)
        .and_then(|list| list.first())
        .ok_or_else(|| {
            ChatError::Plugin(format!(
                "分类 {} 未找到插件（prompts={:?}）",
                category, req.prompts
            ))
        })?;

    let (plugin, symbol) = plugins::plugin_of(hit).ok_or_else(|| {
        ChatError::Plugin(format!("分类 {} 的命中记录没有插件信息", category))
    })?;
    println!(
        "[call_flow] 选中插件: plugin={:?}, symbol={:?}",
        plugin, symbol
    );

    // 插件入参：四个字段原样透传。
    let request = json!({
        "plugins_uid": req.plugins_uid,
        "category": category,
        "prompts": req.prompts,
        "context": req.context,
    });

    plugins::call_plugin_data(&plugin, &symbol, &request)
        .map_err(|error| ChatError::Plugin(format!("插件 {} 调用失败: {:#}", plugin, error)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(category: &str) -> CallChatReq {
        CallChatReq {
            plugins_uid: None,
            category: category.to_string(),
            prompts: "测试".to_string(),
            context: Value::Null,
        }
    }

    #[test]
    fn requires_non_empty_category() {
        let error = run(&request("   ")).unwrap_err();
        assert_eq!(error.code(), 400304);
        assert!(error.message().contains("category"));
    }

    #[test]
    fn rejects_reentry_before_kv_lookup() {
        let _outer = crate::utils::plugins::PluginCallGuard::enter().expect("首层调用应成功");
        let error = run(&request("no_such_collection")).unwrap_err();
        assert_eq!(error.code(), 400304);
        assert!(
            error.message().contains("重入"),
            "重入应先于 kv 检索被拒绝: {}",
            error.message()
        );
    }
}
