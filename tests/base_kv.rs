// tests/base_kv.rs —— kv_lib 的 FFI 调用测试（使用 .dytes/env）
mod common;

use chatllm::utils::kv::{collection_list, collection_put, stats, CollectionSpec};

/// `kv_collection` 的 list 在 .dytes 环境中可调用。
#[test]
fn kv_collection_list_works() {
    if !common::kv_ready() {
        return;
    }

    let data = collection_list(None, None).expect("kv_collection list 失败");
    println!("collections: {}", data);
    assert!(!data.is_null(), "list 应返回 data");
}

/// 创建测试集合并读取统计信息。
#[test]
fn kv_collection_put_and_stats() {
    if !common::kv_ready() {
        return;
    }

    let spec = CollectionSpec {
        name: Some("chatllm test".to_string()),
        description: Some("chatllm 自动化测试集合".to_string()),
        ..Default::default()
    };
    let put = collection_put("chatllm_test", &spec).expect("kv_collection put 失败");
    println!("put: {}", put);
    assert!(!put.is_null(), "put 应返回 data");

    let stats_value = stats(Some("chatllm_test")).expect("kv_collection stats 失败");
    println!("stats: {}", stats_value);
    assert!(!stats_value.is_null(), "stats 应返回 data");
}
