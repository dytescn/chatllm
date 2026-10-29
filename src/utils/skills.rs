// src/utils/skills.rs
//! 技能相关的两类能力：
//!
//! 1. 组装 system prompt：通过 `plugin_lib.get_skills` 读取技能索引与正文，
//!    不直接读取 `skills` 表或 `designdb`；
//! 2. `skills` 表 CRUD：供 FFI 层维护技能内容（`db_query` 直接访问数据库）。

use crate::utils::plugin_lib;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashSet;

const BASE_SYSTEM_PROMPT: &str = "你是一个有用的助手。";
const MAX_SKILLS: usize = 20;
const MAX_SYSTEM_CHARS: usize = 6000;

const TEMPLATE: &str = "{{base_system_prompt}}\n\n当前可用的本地能力分类：\n{{categories}}\n\n可使用的技能：\n{{skills}}\n\n使用规则：\n{{rules}}";

const RULES: &[&str] = &[
    "只使用与当前任务相关的技能；不确定时先追问。",
    "调用本地插件或修改文件前，先确认参数与路径。",
    "不编造执行结果；失败时返回可读错误。",
    "不输出 API Key、密码等敏感信息。",
];

#[derive(Debug, Clone)]
pub struct SkillBuild {
    pub system_prompt: String,
    pub skill_ids: Vec<String>,
    pub categories: Vec<String>,
    pub truncated: bool,
    pub missing_content_ids: Vec<String>,
    pub version_mismatch_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct SkillRow {
    id: String,
    name: String,
    category: String,
    content: String,
    sort: i64,
}

fn category_meta(id: &str) -> (&'static str, &'static str, i64) {
    match id {
        "general" => ("通用", "所有对话都可用的基础回答规范", 10),
        "design" => ("设计软件", "CorelDRAW 等设计软件的操作规范", 20),
        "material" => ("素材库", "素材检索、分类与引用", 30),
        "plugin" => ("本地插件", "插件能力检索、参数确认与执行", 40),
        "safety" => ("安全与权限", "路径、密钥、破坏性操作约束", 90),
        _ => ("其他", "", 100),
    }
}

fn category_rank(id: &str) -> i64 {
    category_meta(id).2
}

fn as_i64(value: Option<&Value>, default: i64) -> i64 {
    match value {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(default),
        Some(Value::String(s)) => s.parse::<i64>().unwrap_or(default),
        _ => default,
    }
}

fn as_string(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or("").to_string()
}

fn compose(base: &str, skills: &[SkillRow]) -> String {
    let mut used_categories: Vec<String> = Vec::new();
    for skill in skills {
        if !used_categories.contains(&skill.category) {
            used_categories.push(skill.category.clone());
        }
    }
    used_categories.sort_by_key(|id| category_rank(id));

    let category_lines = used_categories
        .iter()
        .map(|id| {
            let (name, description, _) = category_meta(id);
            format!("- {}（{}）：{}", name, id, description)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let skill_lines = skills
        .iter()
        .map(|skill| {
            let (name, _, _) = category_meta(&skill.category);
            format!("- [{}] {}：{}", name, skill.name, skill.content)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let rule_lines = RULES
        .iter()
        .map(|rule| format!("- {}", rule))
        .collect::<Vec<_>>()
        .join("\n");

    TEMPLATE
        .replace("{{base_system_prompt}}", base)
        .replace("{{categories}}", &category_lines)
        .replace("{{skills}}", &skill_lines)
        .replace("{{rules}}", &rule_lines)
}

/// 读取 skills 索引与完整内容，组装 system prompt。
///
/// - `use_skills=false`：只返回基础 prompt；
/// - `plugin_uuid` 非空：由 `plugin_lib.get_skills` 按插件过滤；
/// - `skill_ids` 非空：只取这些技能；
/// - `skill_categories` 非空：只取这些分类；
/// - 都为空：全部启用技能；
/// - `explicit`：显式基础 prompt，优先于内置基础 prompt。
pub fn build_system_prompt(
    use_skills: bool,
    plugin_uuid: Option<&str>,
    skill_categories: &[String],
    skill_ids: &[String],
    explicit: Option<&str>,
) -> Result<SkillBuild> {
    let base = explicit
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(BASE_SYSTEM_PROMPT);

    if !use_skills {
        return Ok(SkillBuild {
            system_prompt: base.to_string(),
            skill_ids: Vec::new(),
            categories: Vec::new(),
            truncated: false,
            missing_content_ids: Vec::new(),
            version_mismatch_ids: Vec::new(),
        });
    }

    let request = build_skills_request(plugin_uuid, skill_categories, skill_ids);
    let data = plugin_lib::get_skills(request).context("读取技能失败（plugin_lib.get_skills）")?;

    let mut selected = parse_skills_response(&data, skill_categories, skill_ids);
    selected.sort_by(|a, b| {
        category_rank(&a.category)
            .cmp(&category_rank(&b.category))
            .then(a.sort.cmp(&b.sort))
            .then(a.name.cmp(&b.name))
    });

    let mut truncated = false;
    if selected.len() > MAX_SKILLS {
        selected.truncate(MAX_SKILLS);
        truncated = true;
    }

    if selected.is_empty() {
        return Ok(SkillBuild {
            system_prompt: base.to_string(),
            skill_ids: Vec::new(),
            categories: Vec::new(),
            truncated,
            missing_content_ids: Vec::new(),
            version_mismatch_ids: Vec::new(),
        });
    }

    let mut system_prompt = compose(base, &selected);
    while system_prompt.chars().count() > MAX_SYSTEM_CHARS && !selected.is_empty() {
        selected.pop();
        truncated = true;
        system_prompt = if selected.is_empty() {
            base.to_string()
        } else {
            compose(base, &selected)
        };
    }

    let mut categories: Vec<String> = selected.iter().map(|s| s.category.clone()).collect();
    categories.sort_by_key(|id| category_rank(id));
    categories.dedup();

    Ok(SkillBuild {
        system_prompt,
        skill_ids: selected.iter().map(|s| s.id.clone()).collect(),
        categories,
        truncated,
        missing_content_ids: Vec::new(),
        version_mismatch_ids: Vec::new(),
    })
}

/// 构造 `plugin_lib.get_skills` 请求。
///
/// `plugin_uuid` 用于取某个插件随包技能；`ids` / `categories` 用于进一步筛选。
fn build_skills_request(
    plugin_uuid: Option<&str>,
    skill_categories: &[String],
    skill_ids: &[String],
) -> Value {
    let mut request = json!({
        "enabled": 1,
        "with_content": true,
        "page": 1,
        "page_size": 500,
    });
    if let Some(plugin) = plugin_uuid.map(str::trim).filter(|s| !s.is_empty()) {
        request["plugin_uuid"] = json!(plugin);
    }
    if !skill_categories.is_empty() {
        request["categories"] = json!(skill_categories);
    }
    if !skill_ids.is_empty() {
        request["ids"] = json!(skill_ids);
    }
    request
}

/// 解析 `plugin_lib.get_skills` 的 `data.list`，并按 ids / categories 做二次筛选。
fn parse_skills_response(
    data: &Value,
    skill_categories: &[String],
    skill_ids: &[String],
) -> Vec<SkillRow> {
    let id_filter: HashSet<&str> = skill_ids.iter().map(String::as_str).collect();
    let category_filter: HashSet<&str> = skill_categories.iter().map(String::as_str).collect();

    let Some(list) = data.get("list").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut rows = Vec::new();
    for item in list {
        let id = as_string(item.get("id"));
        if id.is_empty() {
            continue;
        }
        let category = as_string(item.get("category"));
        if !id_filter.is_empty() && !id_filter.contains(id.as_str()) {
            continue;
        }
        if id_filter.is_empty()
            && !category_filter.is_empty()
            && !category_filter.contains(category.as_str())
        {
            continue;
        }
        let content = as_string(item.get("content"));
        if content.trim().is_empty() {
            continue;
        }
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(&id)
            .to_string();
        rows.push(SkillRow {
            id,
            name,
            category,
            content,
            sort: as_i64(item.get("sort"), 0),
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_ranks_are_stable() {
        assert!(category_rank("general") < category_rank("design"));
        assert!(category_rank("design") < category_rank("material"));
        assert!(category_rank("plugin") < category_rank("safety"));
    }

    #[test]
    fn compose_contains_sections() {
        let skills = vec![SkillRow {
            id: "s1".into(),
            name: "技能一".into(),
            category: "general".into(),
            content: "内容一".into(),
            sort: 1,
        }];
        let prompt = compose("你是一个有用的助手。", &skills);
        assert!(prompt.contains("当前可用的本地能力分类"));
        assert!(prompt.contains("通用（general）"));
        assert!(prompt.contains("[通用] 技能一：内容一"));
        assert!(prompt.contains("使用规则"));
    }

    #[test]
    fn skills_request_carries_plugin_and_filters() {
        let req = build_skills_request(
            Some("plugin-1"),
            &["plugin".to_string()],
            &["s1".to_string()],
        );
        assert_eq!(req["plugin_uuid"], "plugin-1");
        assert_eq!(req["enabled"], 1);
        assert_eq!(req["with_content"], true);
        assert_eq!(req["categories"][0], "plugin");
        assert_eq!(req["ids"][0], "s1");

        let plain = build_skills_request(None, &[], &[]);
        assert!(plain.get("plugin_uuid").is_none());
        assert!(plain.get("ids").is_none());
        assert!(plain.get("categories").is_none());
    }

    #[test]
    fn parse_response_filters_by_ids_and_drops_empty_content() {
        let data = json!({
            "list": [
                {"id": "s1", "name": "技能一", "category": "plugin", "content": "内容一", "sort": 2},
                {"id": "s2", "name": "技能二", "category": "plugin", "content": "内容二", "sort": 1},
                {"id": "s3", "name": "空技能", "category": "plugin", "content": " ", "sort": 3}
            ]
        });

        let all = parse_skills_response(&data, &[], &[]);
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|row| row.id != "s3"));

        let only = parse_skills_response(&data, &[], &["s2".to_string()]);
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].id, "s2");
        assert_eq!(only[0].content, "内容二");
    }

    #[test]
    fn parse_response_uses_id_when_name_missing() {
        let data = json!({
            "list": [
                {"id": "s1", "category": "plugin", "content": "内容"}
            ]
        });
        let rows = parse_skills_response(&data, &[], &[]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "s1");
        assert_eq!(rows[0].sort, 0);
    }
}
