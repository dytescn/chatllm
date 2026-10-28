use anyhow::{Context, Result};
use llama_cpp_2::{
    context::params::LlamaContextParams, llama_backend::LlamaBackend, llama_batch::LlamaBatch,
    model::params::LlamaModelParams, model::LlamaModel,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::time::Instant;

const API_URL: &str = "http://127.0.0.1:44944/database";
const DB_PATH: &str = "./design_plugins";
const PAGE_SIZE: usize = 6; // 每条 hex 约 8KB，6 条约 49KB，低于 64KB 上限
const DIM: usize = 1024; // Qwen3-Embedding-0.6B
const BIT_WIDTH: usize = 4;
const K: usize = 5;

fn gguf_path() -> String {
    format!(
        "{}/models/qwen3-embed/qwen3-embedding-0.6.gguf",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    code: i32,
    data: serde_json::Value,
    #[allow(dead_code)]
    msg: String,
}

#[derive(Debug, Deserialize)]
struct PromptRow {
    uuid: String,
    embedding: String, // hex
}

#[derive(Debug, Deserialize)]
struct PluginInfo {
    id: String,
    name: Option<String>,
    soft: Option<String>,
    description: Option<String>,
}

async fn db_query(sql: &str) -> Result<ApiResponse> {
    let client = reqwest::Client::builder()
        .http1_only()
        .build()
        .context("创建 HTTP client 失败")?;

    let body = serde_json::to_string(&json!({ "path": DB_PATH, "sql": sql }))?;
    let body_len = body.len();

    let resp = client
        .post(API_URL)
        .header("Content-Type", "application/libary")
        .header("FFI-Symbol", "db_query")
        .header("Content-Length", body_len)
        .header("Accept", "*/*")
        .header("User-Agent", "curl/8.0")
        .body(body)
        .send()
        .await
        .context("db_query 请求失败")?;

    let status = resp.status();
    let text = resp.text().await.context("读取响应文本失败")?;

    if !status.is_success() {
        anyhow::bail!(
            "db_query HTTP {}: {}",
            status,
            text.chars().take(300).collect::<String>()
        );
    }

    serde_json::from_str::<ApiResponse>(&text).with_context(|| {
        let preview: String = text.chars().take(500).collect();
        format!("解析响应失败，原始响应前 500 字符: {}", preview)
    })
}

fn decode_hex_to_f32(hex: &str) -> Vec<f32> {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn normalise(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

async fn fetch_plugin_info(ids: &[String]) -> Result<HashMap<String, (String, String, String)>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let in_clause = ids
        .iter()
        .map(|id| format!("'{}'", id.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT id, name, soft, description FROM design_plugins WHERE id IN ({})",
        in_clause
    );
    let api = db_query(&sql).await?;
    let rows: Vec<PluginInfo> = serde_json::from_value(api.data)?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.id,
                (
                    r.name.unwrap_or_default(),
                    r.soft.unwrap_or_default(),
                    r.description.unwrap_or_default(),
                ),
            )
        })
        .collect())
}

fn truncate_chars(s: &str, n: usize) -> String {
    let taken: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{}...", taken)
    } else {
        taken
    }
}

#[tokio::test]
async fn turbovec_bench() -> Result<()> {
    // ===== 1. 全量加载 prompts =====
    println!("=== 全量加载向量（每页 {} 条） ===", PAGE_SIZE);
    let load_start = Instant::now();

    let mut flat_vectors: Vec<f32> = Vec::new();
    let mut external_ids: Vec<u64> = Vec::new();
    let mut id_to_uuid: HashMap<u64, String> = HashMap::new();
    let mut next_id: u64 = 0;
    let mut offset = 0;
    let mut page = 0;

    loop {
        let sql = format!(
            "SELECT uuid, hex(embedding) AS embedding FROM prompts LIMIT {} OFFSET {}",
            PAGE_SIZE, offset
        );
        let api = db_query(&sql).await?;
        if api.code != 200 {
            eprintln!("查询失败: {}", api.msg);
            break;
        }
        let rows: Vec<PromptRow> = serde_json::from_value(api.data)?;
        if rows.is_empty() {
            break;
        }
        for row in &rows {
            let vec = decode_hex_to_f32(&row.embedding);
            if vec.len() != DIM {
                continue;
            }
            flat_vectors.extend_from_slice(&vec);
            external_ids.push(next_id);
            id_to_uuid.insert(next_id, row.uuid.clone());
            next_id += 1;
        }
        page += 1;
        if page % 50 == 0 {
            println!("  已加载 {} 条（第 {} 页）", external_ids.len(), page);
        }
        offset += PAGE_SIZE;
    }

    let n = external_ids.len();
    println!(
        "加载 {} 条向量，共 {} 页，耗时 {:?}",
        n,
        page,
        load_start.elapsed()
    );
    if n == 0 {
        return Ok(());
    }

    // ===== 2. 构建索引 =====
    let build_start = Instant::now();
    let mut index = turbovec::IdMapIndex::new(DIM, BIT_WIDTH)?;
    index.add_with_ids(&flat_vectors, &external_ids)?;
    println!("索引构建完成（{} 条），耗时 {:?}", n, build_start.elapsed());

    // ===== 3. 加载 Qwen3 GGUF 模型 =====
    println!("\n=== 加载 Qwen3 模型 ===");
    let model_start = Instant::now();
    let backend = LlamaBackend::init().context("初始化 LlamaBackend 失败")?;
    let model_params = LlamaModelParams::default();
    let model = LlamaModel::load_from_file(&backend, gguf_path(), &model_params)
        .context("加载 GGUF 模型失败")?;
    let ctx_params = LlamaContextParams::default()
        .with_embeddings(true)
        .with_n_ctx(std::num::NonZeroU32::new(2048));
    let mut ctx = model
        .new_context(&backend, ctx_params)
        .context("创建 LlamaContext 失败")?;
    println!("模型加载完成，耗时 {:?}", model_start.elapsed());

    // ===== 4. 中文查询测试 =====
    let queries = [
        "人工智能的发展历程",
        "健康生活方式和饮食建议",
        "如何减轻工作压力",
        "北京天气预报",
        "环保行动计划",
        "秋天主题的诗歌",
    ];

    println!("\n=== 中文查询测试（k={}） ===", K);
    for q in &queries {
        // 编码
        let enc_start = Instant::now();
        let tokens = model.vocab().tokenize(q.as_bytes(), true, false);
        ctx.clear_kv_cache();
        let mut batch = LlamaBatch::new(tokens.len().max(1), 1);
        batch
            .add_sequence(&tokens, 0, false)
            .context("添加 batch 失败")?;
        ctx.decode(&mut batch).context("llama_decode 失败")?;
        let emb = ctx.embeddings_seq_ith(0).context("获取 embeddings 失败")?;
        let mut q_vec = emb.to_vec();
        normalise(&mut q_vec);
        let enc_elapsed = enc_start.elapsed();

        // 搜索
        let search_start = Instant::now();
        let (scores, ids) = index.search(&q_vec, K);
        let search_elapsed = search_start.elapsed();

        // 按 uuid 去重，保留最高分
        let mut best: HashMap<String, f32> = HashMap::new();
        for (score, id) in scores.iter().zip(ids.iter()) {
            let uuid = id_to_uuid.get(id).cloned().unwrap_or_default();
            let entry = best.entry(uuid).or_insert(f32::MIN);
            if *score > *entry {
                *entry = *score;
            }
        }
        let mut ranked: Vec<(String, f32)> = best.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let uuids: Vec<String> = ranked.iter().map(|(u, _)| u.clone()).collect();
        let info = fetch_plugin_info(&uuids).await?;

        println!("\n查询: 「{}」", q);
        println!(
            "  编码耗时: {:?}，搜索耗时: {:?}",
            enc_elapsed, search_elapsed
        );
        for (rank, (uuid, score)) in ranked.iter().enumerate() {
            let (_, _, desc) = info.get(uuid).cloned().unwrap_or_default();
            println!(
                "  [{}] score={:.4} uuid={} desc={}",
                rank + 1,
                score,
                uuid,
                truncate_chars(&desc, 100)
            );
        }
    }

    Ok(())
}
