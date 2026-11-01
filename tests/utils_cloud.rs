// tests/utils_cloud.rs —— utils::cloud 的供应商选择与系统库调用测试
mod common;

use chatllm::utils::cloud::{self, Supplier};
use serde_json::json;

#[test]
fn supplier_from_code_defaults_to_deepseek() {
    assert_eq!(Supplier::from_code(None), Supplier::Deepseek);
    assert_eq!(Supplier::from_code(Some("")), Supplier::Deepseek);
    assert_eq!(Supplier::from_code(Some("   ")), Supplier::Deepseek);
    assert_eq!(Supplier::from_code(Some("unknown")), Supplier::Deepseek);
    assert_eq!(Supplier::from_code(Some("deepseek")), Supplier::Deepseek);
    assert_eq!(
        Supplier::from_code(Some(" siliconflow_t2t ")),
        Supplier::SiliconFlow
    );
}

#[test]
fn supplier_metadata() {
    assert_eq!(Supplier::Deepseek.code(), "deepseek");
    assert_eq!(Supplier::Deepseek.lib_name(), "libdeepseek");
    assert_eq!(Supplier::Deepseek.token_key(), "deepseek_app_key");
    assert_eq!(Supplier::SiliconFlow.code(), "siliconflow_t2t");
    assert_eq!(Supplier::SiliconFlow.lib_name(), "libsiliconflow_t2t");
    assert_eq!(Supplier::SiliconFlow.token_key(), "silicon_app_key");
    assert_eq!(
        Supplier::Deepseek.missing_token_message(),
        "AI 配置错误：未配置 deepseek token：deepseek_app_key"
    );
}

#[test]
fn library_candidates_only_use_libary() {
    let paths = cloud::library_candidates("libdeepseek");
    assert_eq!(paths.len(), 2);
    assert!(paths.iter().all(|p| p.starts_with("./libary/")));
    assert!(paths
        .iter()
        .all(|p| p.to_string_lossy().contains("libdeepseek")));
}

#[test]
fn messages_from_filters_empty_content() {
    let messages = cloud::messages_from(&[("system", "  "), ("user", "hi")]);
    assert_eq!(messages, vec![json!({ "role": "user", "content": "hi" })]);
}

#[test]
fn resolves_supplier_library_in_testenv() {
    if !common::db_ready() {
        return common::skip("cloud 系统库路径");
    }

    let path = cloud::resolve_library_path("libdeepseek").expect("应能找到 libdeepseek");
    assert!(path.exists());
}

#[test]
fn reads_current_supplier_from_info_db() {
    if !common::db_ready() {
        return common::skip("cloud 配置");
    }
    if !std::path::Path::new("./database/info.db").exists() {
        return common::skip("info.db 不存在");
    }

    let supplier = cloud::current_supplier().expect("读取 ai_supplier 失败");
    assert!(matches!(
        supplier,
        Supplier::Deepseek | Supplier::SiliconFlow
    ));
}

#[test]
#[ignore = "会真实请求云端 API，需配置对应 token"]
fn chat_returns_content() {
    if !common::db_ready() {
        return common::skip("cloud 调用");
    }

    let prompt = "只回复两个字：收到";
    let reply = cloud::chat(prompt, &[json!({ "role": "user", "content": prompt })])
        .expect("云端调用失败");
    assert!(!reply.content.trim().is_empty());
    assert!(reply.model.is_some());
}
