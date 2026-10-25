use anyhow::{Context, Result};
use llama_cpp_2::{
    context::params::LlamaContextParams, llama_backend::LlamaBackend, llama_batch::LlamaBatch,
    model::params::LlamaModelParams, model::LlamaModel,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const DIM: usize = 1024;
const BIT_WIDTH: usize = 4;
const K: usize = 10;
/// 最多测多少条 query（0 = 全部）
const MAX_QUERIES: usize = 20;
/// 最多加载多少条 corpus 向量（0 = 全部）
const MAX_CORPUS: usize = 0;

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
struct CorpusEmbedding {
    uuid: String,
    embedding: Vec<f32>,
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

/// 从 corpus_embeddings.jsonl 加载向量
fn load_corpus_embeddings(max: usize) -> Result<(Vec<f32>, Vec<u64>, HashMap<u64, String>)> {
    let path = scifact_dir().join("corpus_embeddings.jsonl");
    let f = File::open(&path)
        .with_context(|| format!("打开 {} 失败（先跑 embed_scifact_file）", path.display()))?;
    let reader = BufReader::new(f);

    let mut flat: Vec<f32> = Vec::new();
    let mut ids: Vec<u64> = Vec::new();
    let mut id_to_uuid: HashMap<u64, String> = HashMap::new();
    let mut next: u64 = 0;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let row: CorpusEmbedding = serde_json::from_str(&line)?;
        if row.embedding.len() != DIM {
            continue;
        }
        flat.extend_from_slice(&row.embedding);
        ids.push(next);
        id_to_uuid.insert(next, row.uuid);
        next += 1;
        if max > 0 && ids.len() >= max {
            break;
        }
    }
    Ok((flat, ids, id_to_uuid))
}

/// 从 queries.jsonl 读取真实查询
fn load_queries(max: usize) -> Result<Vec<(String, String)>> {
    let path = scifact_dir().join("queries.jsonl");
    let f = File::open(&path).with_context(|| format!("打开 {} 失败", path.display()))?;
    let reader = BufReader::new(f);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let q: ScifactQuery = serde_json::from_str(&line)?;
        out.push((id_to_string(&q.id), q.text.trim().to_string()));
        if max > 0 && out.len() >= max {
            break;
        }
    }
    Ok(out)
}

struct LatencyStats {
    samples: Vec<Duration>,
}

impl LatencyStats {
    fn new() -> Self {
        Self { samples: Vec::new() }
    }
    fn push(&mut self, d: Duration) {
        self.samples.push(d);
    }
    fn report(&self, label: &str) {
        if self.samples.is_empty() {
            println!("{}: 无样本", label);
            return;
        }
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

fn embed_query(
    model: &LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext,
    text: &str,
) -> Result<Vec<f32>> {
    let tokens = model.vocab().tokenize(text.as_bytes(), true, false);
    ctx.clear_kv_cache();
    let mut batch = LlamaBatch::new(tokens.len().max(1), 1);
    batch.add_sequence(&tokens, 0, true)?;
    ctx.decode(&mut batch)?;
    let emb = ctx.embeddings_seq_ith(0).context("获取 embeddings 失败")?;
    let mut v = emb.to_vec();
    normalise(&mut v);
    Ok(v)
}

#[tokio::test]
async fn turbovec_sci() -> Result<()> {
    println!("========== 环境 ==========");
    println!("GGUF       : {}", gguf_path());
    println!("scifact    : {}", scifact_dir().display());
    println!("DIM        : {}", DIM);
    println!("BIT_WIDTH  : {}", BIT_WIDTH);
    println!("K          : {}", K);
    println!(
        "MAX_CORPUS : {}",
        if MAX_CORPUS == 0 { "全部".into() } else { MAX_CORPUS.to_string() }
    );
    println!(
        "MAX_QUERIES: {}",
        if MAX_QUERIES == 0 { "全部".into() } else { MAX_QUERIES.to_string() }
    );

    // ===== 1. 加载 corpus 向量 =====
    println!("\n[1] 加载 corpus 向量 ...");
    let t = Instant::now();
    let (flat, ids, id_to_uuid) = load_corpus_embeddings(MAX_CORPUS)?;
    let n = ids.len();
    println!("加载 {} 条向量，耗时 {:?}", n, t.elapsed());
    if n == 0 {
        anyhow::bail!("没有向量可加载，请先运行 embed_scifact_file 生成 corpus_embeddings.jsonl");
    }

    // ===== 2. 构建 turbovec 索引 =====
    println!("\n[2] 构建 turbovec 索引 ...");
    let t = Instant::now();
    let mut index = turbovec::IdMapIndex::new(DIM, BIT_WIDTH)?;
    index.add_with_ids(&flat, &ids)?;
    println!("索引构建完成（{} 条），耗时 {:?}", n, t.elapsed());

    // ===== 3. 加载模型 =====
    println!("\n[3] 加载 Qwen3 模型 ...");
    let t = Instant::now();
    let backend = LlamaBackend::init().context("初始化 LlamaBackend 失败")?;
    let model = LlamaModel::load_from_file(&backend, gguf_path(), &LlamaModelParams::default())
        .context("加载 GGUF 失败")?;

    // 关键：显式设置线程数，避免长序列 decode 被拖慢
    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
        .min(8);
    println!("使用线程数: {}", n_threads);

    let ctx_params = LlamaContextParams::default()
        .with_embeddings(true)
        .with_n_ctx(std::num::NonZeroU32::new(2048))
        .with_n_threads(n_threads as i32)
        .with_n_threads_batch(n_threads as i32);

    // 注意：这里必须保留 let mut ctx
    let mut ctx = model
        .new_context(&backend, ctx_params)
        .context("创建 context 失败")?;
    println!("模型加载耗时: {:?}", t.elapsed());

    // ===== 4. 加载 scifact queries =====
    println!("\n[4] 加载 scifact queries ...");
    let t = Instant::now();
    let queries = load_queries(MAX_QUERIES)?;
    println!("queries: {} 条，耗时 {:?}", queries.len(), t.elapsed());

    // ===== 5. 逐条查询测速 =====
    println!("\n[5] 逐条查询测速（k={}） ...", K);
    let mut enc_lat = LatencyStats::new();
    let mut search_lat = LatencyStats::new();
    let mut total_lat = LatencyStats::new();

    let mut printed = 0usize;
    for (qid, text) in &queries {
        let t0 = Instant::now();
        let q_vec = embed_query(&model, &mut ctx, text)?;
        let enc_elapsed = t0.elapsed();
        enc_lat.push(enc_elapsed);

        let t1 = Instant::now();
        let (scores, hit_ids) = index.search(&q_vec, K);
        let search_elapsed = t1.elapsed();
        search_lat.push(search_elapsed);
        total_lat.push(t0.elapsed());

        // 打印前 3 条 query 的 top-K，确认结果合理
        if printed < 3 {
            println!("\n查询 [{}] 「{}」", qid, text);
            println!("  编码 {:?}，搜索 {:?}", enc_elapsed, search_elapsed);
            for (rank, (score, id)) in scores.iter().zip(hit_ids.iter()).enumerate() {
                let uuid = id_to_uuid.get(id).cloned().unwrap_or_default();
                println!("  [{}] score={:.4} uuid={}", rank + 1, score, uuid);
            }
            printed += 1;
        }
    }

    // ===== 6. 汇总 =====
    println!("\n========== 汇总 ==========");
    println!("corpus 向量数 : {}", n);
    println!("query 条数    : {}", queries.len());
    println!(
        "索引大小估算  : {:.2} MiB（{} 条 × {} dim × {} bit）",
        (n * DIM * BIT_WIDTH / 8) as f64 / 1024.0 / 1024.0,
        n,
        DIM,
        BIT_WIDTH
    );
    enc_lat.report("编码延迟    ");
    search_lat.report("搜索延迟    ");
    total_lat.report("端到端延迟  ");
    println!(
        "\n吞吐: 编码 {:.1} q/s，搜索 {:.0} q/s",
        queries.len() as f64 / enc_lat.samples.iter().map(|d| d.as_secs_f64()).sum::<f64>(),
        queries.len() as f64 / search_lat.samples.iter().map(|d| d.as_secs_f64()).sum::<f64>()
    );

    // ===== 7. 批量搜索基准（排除编码干扰，纯测索引） =====
    println!("\n[7] 纯搜索批量基准（编码一次，搜 100 次） ...");
    if let Some((_, text)) = queries.first() {
        let q_vec = embed_query(&model, &mut ctx, text)?;
        let t = Instant::now();
        let reps = 100;
        for _ in 0..reps {
            let _ = index.search(&q_vec, K);
        }
        let elapsed = t.elapsed();
        println!(
            "{} 次搜索耗时 {:?}，平均 {:.3}ms/次，吞吐 {:.0} q/s",
            reps,
            elapsed,
            elapsed.as_secs_f64() * 1000.0 / reps as f64,
            reps as f64 / elapsed.as_secs_f64()
        );
    }

    Ok(())
}
