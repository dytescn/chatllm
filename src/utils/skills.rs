// src/utils/skills.rs
//! skills 索引（SQLite `dialogue.db`）+ 完整内容（`./designdb/skills`）读取与 system prompt 组装。
//!
//! 目标结构：
//! - SQLite：id / name / description / category / is_system / is_enabled / sort / version
//! - designdb：`./designdb/skills` 的 `skills` 集合，存 content / trigger / params_schema

use crate::utils::{db, designdb};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

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
/// - `skill_ids` 非空：只取这些技能；
/// - `skill_categories` 非空：只取这些分类；
/// - 两者都为空：全部启用技能；
/// - `explicit`：显式基础 prompt，优先于内置基础 prompt。
pub fn build_system_prompt(
    use_skills: bool,
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

    // 1. SQLite 索引
    let rows = db::db_query(
        "dialogue",
        "SELECT id, name, category, sort, version
         FROM skills WHERE is_enabled = 1 ORDER BY category, sort",
    )
    .context("读取 skills 索引失败")?;

    let id_filter: HashSet<&str> = skill_ids.iter().map(String::as_str).collect();
    let category_filter: HashSet<&str> = skill_categories.iter().map(String::as_str).collect();

    let mut index_rows: Vec<(String, String, String, i64, i64)> = Vec::new();
    for row in &rows {
        let id = as_string(row.get("id"));
        let category = as_string(row.get("category"));
        if !id_filter.is_empty() && !id_filter.contains(id.as_str()) {
            continue;
        }
        if id_filter.is_empty()
            && !category_filter.is_empty()
            && !category_filter.contains(category.as_str())
        {
            continue;
        }
        index_rows.push((
            id,
            as_string(row.get("name")),
            category,
            as_i64(row.get("sort"), 0),
            as_i64(row.get("version"), 1),
        ));
    }

    // 2. designdb 完整内容
    let content_rows = designdb::query_all("skills", "skills", Some(json!({})), None)
        .context("读取 skills 完整内容失败")?;
    let content_by_id: HashMap<String, Value> = content_rows
        .into_iter()
        .filter_map(|doc| {
            let id = doc
                .get("_id")
                .or_else(|| doc.get("id"))
                .and_then(Value::as_str)?
                .to_string();
            Some((id, doc))
        })
        .collect();

    let mut missing_content_ids = Vec::new();
    let mut version_mismatch_ids = Vec::new();
    let mut selected: Vec<SkillRow> = Vec::new();

    for (id, name, category, sort, index_version) in index_rows {
        let Some(doc) = content_by_id.get(&id) else {
            missing_content_ids.push(id);
            continue;
        };
        let content = as_string(doc.get("content"));
        if content.trim().is_empty() {
            missing_content_ids.push(id);
            continue;
        }
        let content_version = as_i64(doc.get("version"), index_version);
        if content_version != index_version {
            version_mismatch_ids.push(id.clone());
        }
        let content_name = doc
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(&name)
            .to_string();

        selected.push(SkillRow {
            id,
            name: content_name,
            category,
            content,
            sort,
        });
    }

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
            missing_content_ids,
            version_mismatch_ids,
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
        missing_content_ids,
        version_mismatch_ids,
    })
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
}
