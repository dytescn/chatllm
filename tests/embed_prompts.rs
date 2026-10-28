use anyhow::{Context, Result};
use llama_cpp_2::{
    context::params::LlamaContextParams, llama_backend::LlamaBackend, llama_batch::LlamaBatch,
    model::params::LlamaModelParams, model::LlamaModel,
};
use serde::Deserialize;
use serde_json::json;
use std::time::Instant;

const API_URL: &str = "http://127.0.0.1:44944/database";
const DB_PATH: &str = "./design_plugins";
const PAGE_SIZE: usize = 10;
const DIM: usize = 1024; // Qwen3-Embedding-0.6B 输出 1024 维

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
struct Row {
    id: String, // design_plugins.id，作为 prompts.uuid
    description: String,
}

fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn normalise(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// 生成 prompts.id：sha1(uuid + 文本) 前 32 位，保证同 uuid 同文本幂等
fn stable_id(uuid: &str, text: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut h = Sha1::new();
    h.update(uuid.as_bytes());
    h.update(b":");
    h.update(text.as_bytes());
    format!("{:x}", h.finalize())[..32].to_string()
}

async fn post(symbol: &str, sql: &str) -> Result<ApiResponse> {
    let body = serde_json::to_string(&json!({ "path": DB_PATH, "sql": sql }))?;
    let resp = reqwest::Client::new()
        .post(API_URL)
        .header("Content-Type", "application/libary")
        .header("FFI-Symbol", symbol)
        .body(body)
        .send()
        .await?
        .text()
        .await?;
    serde_json::from_str(&resp)
        .with_context(|| format!("解析失败: {}", &resp[..resp.len().min(200)]))
}

#[tokio::test]
async fn embed_and_insert() -> Result<()> {
    println!("[步骤 1] 开始");
    // 表已由 ts/reinit_prompts.ts 重建为 (id, uuid, embedding)，
    // 这里不再 DELETE，直接开始向量化插入。

    // ===== 步骤 2：初始化 llama-cpp-2 =====
    println!("[步骤 2] 初始化 llama-cpp-2 并加载模型 ...");
    println!("  GGUF 路径: {}", gguf_path());
    let t = Instant::now();

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

    println!("[步骤 2] 模型加载完成，耗时={:?}", t.elapsed());

    // ===== 步骤 3：分页读全量，逐批 embed + 插入 =====
    println!("\n[步骤 3] 开始分页处理，每页 {} 条 ...", PAGE_SIZE);
    let total_start = Instant::now();
    let mut offset = 0;
    let mut total_inserted = 0;
    let mut page = 0;

    loop {
        let sql = format!(
            "SELECT id, description FROM design_plugins WHERE description IS NOT NULL AND description != '' LIMIT {} OFFSET {}",
            PAGE_SIZE, offset
        );
        let r = post("db_query", &sql).await?;
        if r.code != 200 {
            eprintln!("查询失败: {} - {}", r.code, r.msg);
            break;
        }

        let rows: Vec<Row> = serde_json::from_value(r.data)?;
        if rows.is_empty() {
            break;
        }

        page += 1;
        println!("\n[页 {}] offset={} 读到 {} 条", page, offset, rows.len());

        for (i, row) in rows.iter().enumerate() {
            let t = Instant::now();

            let tokens = model
                .vocab()
                .tokenize(row.description.as_bytes(), true, false);

            ctx.clear_kv_cache();
            let mut batch = LlamaBatch::new(tokens.len().max(1), 1);
            batch
                .add_sequence(&tokens, 0, true)
                .context("添加 batch 失败")?;

            ctx.decode(&mut batch).context("llama_decode 失败")?;

            let emb = ctx.embeddings_seq_ith(0).context("获取 embeddings 失败")?;
            let mut vector = emb.to_vec();
            normalise(&mut vector);

            if vector.len() != DIM {
                eprintln!(
                    "  !! 维度异常，跳过 uuid={}（期望 {}，实际 {}）",
                    row.id,
                    DIM,
                    vector.len()
                );
                continue;
            }

            let vec_id = stable_id(&row.id, &row.description);
            let hex: String = to_blob(&vector)
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect();

            // 新表结构：(id, uuid, embedding)
            let sql = format!(
                "INSERT OR REPLACE INTO prompts (id, uuid, embedding) VALUES ('{}', '{}', X'{}')",
                vec_id.replace('\'', "''"),
                row.id.replace('\'', "''"),
                hex
            );
            let insert_start = Instant::now();
            let r = post("db_update", &sql).await?;

            if r.code == 200 {
                total_inserted += 1;
                println!(
                    "  [{}/{}] uuid={} vec_id={} 向量={:?} 插入={:?}",
                    i + 1,
                    rows.len(),
                    row.id,
                    &vec_id[..8],
                    t.elapsed(),
                    insert_start.elapsed()
                );
            } else {
                eprintln!(
                    "  [{}/{}] uuid={} 插入失败: {} - {}",
                    i + 1,
                    rows.len(),
                    row.id,
                    r.code,
                    r.msg
                );
            }
        }

        offset += PAGE_SIZE;
    }

    println!("\n========== 统计 ==========");
    println!("总页数: {}", page);
    println!("总插入条数: {}", total_inserted);
    println!("总耗时: {:?}", total_start.elapsed());

    Ok(())
}
