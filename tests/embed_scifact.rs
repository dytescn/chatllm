use anyhow::{Context, Result};
use llama_cpp_2::{
    context::params::LlamaContextParams, llama_backend::LlamaBackend, llama_batch::LlamaBatch,
    model::params::LlamaModelParams, model::LlamaModel,
};
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const API_URL: &str = "http://127.0.0.1:44944/database";
const DB_PATH: &str = "./design_plugins"; // 你原来的 db 路径，保持不动
const DIM: usize = 1024;

/// 0 = 全量；调试时可设 100/200
const MAX_SAMPLES: usize = 0;

fn gguf_path() -> String {
    format!(
        "{}/models/qwen3-embed/qwen3-embedding-0.6.gguf",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn scifact_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("datasets/scifact")
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    code: i32,
    data: serde_json::Value,
    #[allow(dead_code)]
    msg: String,
}

#[derive(Debug, Deserialize)]
struct ScifactCorpusDoc {
    #[serde(rename = "_id")]
    id: serde_json::Value,
    title: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct ScifactQuery {
    #[serde(rename = "_id")]
    id: serde_json::Value,
    text: String,
}

fn id_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn normalise(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

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

// ===== 数据加载 =====

fn load_corpus(max: usize) -> Result<Vec<(String, String)>> {
    let path = scifact_dir().join("corpus.jsonl");
    let f = File::open(&path).with_context(|| format!("打开 {} 失败", path.display()))?;
    let reader = BufReader::new(f);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() { continue; }
        let doc: ScifactCorpusDoc = serde_json::from_str(&line)
            .with_context(|| format!("解析 corpus 行失败: {}", &line[..line.len().min(80)]))?;
        let id = id_to_string(&doc.id);
        let text = format!("{} {}", doc.title.trim(), doc.text.trim());
        out.push((id, text));
        if max > 0 && out.len() >= max { break; }
    }
    Ok(out)
}

fn load_queries(max: usize) -> Result<Vec<(String, String)>> {
    let path = scifact_dir().join("queries.jsonl");
    let f = File::open(&path).with_context(|| format!("打开 {} 失败", path.display()))?;
    let reader = BufReader::new(f);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() { continue; }
        let q: ScifactQuery = serde_json::from_str(&line)
            .with_context(|| format!("解析 queries 行失败: {}", &line[..line.len().min(80)]))?;
        let id = id_to_string(&q.id);
        out.push((id, q.text.trim().to_string()));
        if max > 0 && out.len() >= max { break; }
    }
    Ok(out)
}

// ===== embed 单条 =====

fn embed_one(
    model: &LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext<'_>,
    text: &str,
) -> Result<(Vec<f32>, usize, Duration)> {
    let tokens = model.vocab().tokenize(text.as_bytes(), true, false);
    let n_tokens = tokens.len();
    ctx.clear_kv_cache();
    let mut batch = LlamaBatch::new(tokens.len().max(1), 1);
    batch.add_sequence(&tokens, 0, true)?;
    let t = Instant::now();
    ctx.decode(&mut batch)?;
    let decode_time = t.elapsed();
    let emb = ctx.embeddings_seq_ith(0).context("获取 embeddings 失败")?;
    let mut vector = emb.to_vec();
    normalise(&mut vector);
    Ok((vector, n_tokens, decode_time))
}

// ===== 延迟统计 =====

struct LatencyStats { samples: Vec<Duration> }
impl LatencyStats {
    fn new() -> Self { Self { samples: Vec::new() } }
    fn push(&mut self, d: Duration) { self.samples.push(d); }
    fn report(&self, label: &str) {
        if self.samples.is_empty() { println!("{}: 无样本", label); return; }
        let mut v: Vec<f64> = self.samples.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = v.len();
        let mean = v.iter().sum::<f64>() / n as f64;
        let p50 = v[n / 2];
        let p90 = v[(n * 9 / 10).min(n - 1)];
        let p99 = v[(n * 99 / 100).min(n - 1)];
        println!(
            "{}: n={} mean={:.2}ms p50={:.2}ms p90={:.2}ms p99={:.2}ms min={:.2}ms max={:.2}ms",
            label, n, mean, p50, p90, p99, v[0], v[n - 1]
        );
    }
}

// ===== 通用：embed 一批数据并写入指定表 =====

async fn embed_and_store(
    model: &LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext<'_>,
    docs: &[(String, String)],
    table: &str,
) -> Result<(usize, usize, LatencyStats, Duration)> {
    // 重建表
    let _ = post("db_update", &format!("DROP TABLE IF EXISTS {}", table)).await?;
    let _ = post(
        "db_update",
        &format!(
            "CREATE TABLE {} (id TEXT PRIMARY KEY, uuid TEXT NOT NULL, embedding BLOB NOT NULL)",
            table
        ),
    ).await?;

    let mut insert_lat = LatencyStats::new();
    let mut ok = 0usize;
    let mut fail = 0usize;
    let start = Instant::now();

    for (i, (uuid, text)) in docs.iter().enumerate() {
        let t0 = Instant::now();
        let (vector, _ntok, _) = embed_one(model, ctx, text)?;
        if vector.len() != DIM {
            eprintln!("  !! 维度异常 uuid={} 实际={}", uuid, vector.len());
            fail += 1;
            continue;
        }
        let vec_id = stable_id(uuid, text);
        let hex: String = to_blob(&vector).iter().map(|b| format!("{:02x}", b)).collect();
        let sql = format!(
            "INSERT OR REPLACE INTO {} (id, uuid, embedding) VALUES ('{}', '{}', X'{}')",
            table,
            vec_id.replace('\'', "''"),
            uuid.replace('\'', "''"),
            hex
        );
        let t1 = Instant::now();
        let r = post("db_update", &sql).await?;
        insert_lat.push(t1.elapsed());
        if r.code == 200 { ok += 1; } else {
            fail += 1;
            eprintln!("  插入失败 uuid={} code={} msg={}", uuid, r.code, r.msg);
        }
        if (i + 1) % 50 == 0 {
            println!("  [{}/{}] 已处理", i + 1, docs.len());
        }
        let _ = t0;
    }
    Ok((ok, fail, insert_lat, start.elapsed()))
}

#[tokio::test]
async fn embed_scifact() -> Result<()> {
    println!("========== 环境 ==========");
    println!("GGUF       : {}", gguf_path());
    println!("scifact    : {}", scifact_dir().display());
    println!("DB_PATH    : {}", DB_PATH);
    println!("DIM        : {}", DIM);
    println!("MAX_SAMPLES: {}", if MAX_SAMPLES == 0 { "全量".to_string() } else { MAX_SAMPLES.to_string() });

    // 1. 加载模型
    println!("\n[1] 加载模型 ...");
    let t = Instant::now();
    let backend = LlamaBackend::init().context("初始化 LlamaBackend 失败")?;
    let model = LlamaModel::load_from_file(&backend, gguf_path(), &LlamaModelParams::default())
        .context("加载 GGUF 失败")?;
    let ctx_params = LlamaContextParams::default()
        .with_embeddings(true)
        .with_n_ctx(std::num::NonZeroU32::new(2048));
    let mut ctx = model.new_context(&backend, ctx_params).context("创建 context 失败")?;
    println!("模型加载耗时: {:?}", t.elapsed());

    // 2. 加载 corpus
    println!("\n[2] 加载 scifact corpus ...");
    let t = Instant::now();
    let corpus = load_corpus(MAX_SAMPLES)?;
    let total_chars: usize = corpus.iter().map(|(_, s)| s.chars().count()).sum();
    println!(
        "corpus: {} 条, 平均字符 {:.1}, 加载耗时 {:?}",
        corpus.len(), total_chars as f64 / corpus.len() as f64, t.elapsed()
    );

    // 3. embed + 写入 scifact_corpus
    println!("\n[3] embed corpus → scifact_corpus ...");
    let (ok, fail, insert_lat, elapsed) =
        embed_and_store(&model, &mut ctx, &corpus, "scifact_corpus").await?;
    println!(
        "\ncorpus 完成: 成功={} 失败={} 墙钟={:?} 吞吐={:.2} docs/s",
        ok, fail, elapsed, ok as f64 / elapsed.as_secs_f64()
    );
    insert_lat.report("单条插入延迟");

    // 4. 加载 queries
    println!("\n[4] 加载 scifact queries ...");
    let t = Instant::now();
    let queries = load_queries(MAX_SAMPLES)?;
    println!("queries: {} 条, 加载耗时 {:?}", queries.len(), t.elapsed());

    // 5. embed + 写入 scifact_queries
    println!("\n[5] embed queries → scifact_queries ...");
    let (ok_q, fail_q, insert_lat_q, elapsed_q) =
        embed_and_store(&model, &mut ctx, &queries, "scifact_queries").await?;
    println!(
        "\nqueries 完成: 成功={} 失败={} 墙钟={:?} 吞吐={:.2} docs/s",
        ok_q, fail_q, elapsed_q, ok_q as f64 / elapsed_q.as_secs_f64()
    );
    insert_lat_q.report("单条插入延迟");

    // 6. 汇总
    println!("\n========== 汇总 ==========");
    println!("corpus : {} 条, {:.2} docs/s, {:?}", ok, ok as f64 / elapsed.as_secs_f64(), elapsed);
    println!("queries: {} 条, {:.2} docs/s, {:?}", ok_q, ok_q as f64 / elapsed_q.as_secs_f64(), elapsed_q);

    Ok(())
}
