// src/utils/skills.rs
//! designplugin_lib 的技能集合：按 `plugin_uuid` 或技能名称读取正文。

use crate::utils::designdb;
use serde_json::{json, Value};

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
            Err(error) => println!("[skills] 查询失败: {:#}", error),
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
                    Err(error) => println!("[skills] 查询失败: {:#}", error),
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
        println!("[skills] 未找到: uuid={:?}, name={:?}", uuid, name);
        None
    } else {
        println!("[skills] 读取成功: {} chars", content.len());
        Some(content)
    }
}
