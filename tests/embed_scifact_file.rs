use anyhow::{Context, Result};
use llama_cpp_2::{
    context::params::LlamaContextParams, llama_backend::LlamaBackend, llama_batch::LlamaBatch,
    model::params::LlamaModelParams, model::LlamaModel,
};
use serde::Deserialize;
use serde_json::json;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const DIM: usize = 1024;
/// 0 = 全量；调试时设 50/100
const MAX_SAMPLES: usize = 10;

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
        for x in v.iter_mut() { *x /= norm; }
    }
}

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

fn embed_one(
    model: &LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext,
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

/// 把一批文档 embed 后写入单个 .jsonl 文件，每行：
/// {"uuid":"...","text_len":123,"n_tokens":45,"embedding":[...]}
fn embed_to_jsonl(
    model: &LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext,
    docs: &[(String, String)],
    out_path: &PathBuf,
) -> Result<(usize, usize, LatencyStats, Duration)> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(out_path)
        .with_context(|| format!("创建 {} 失败", out_path.display()))?;
    let mut writer = BufWriter::new(file);

    let mut insert_lat = LatencyStats::new();
    let mut ok = 0usize;
    let mut fail = 0usize;
    let start = Instant::now();

    for (i, (uuid, text)) in docs.iter().enumerate() {
        let t0 = Instant::now();
        let (vector, n_tokens, _decode) = embed_one(model, ctx, text)?;
        if vector.len() != DIM {
            eprintln!("  !! 维度异常 uuid={} 实际={}", uuid, vector.len());
            fail += 1;
            continue;
        }
        let line = json!({
            "uuid": uuid,
            "text_len": text.chars().count(),
            "n_tokens": n_tokens,
            "embedding": vector,
        });
        serde_json::to_writer(&mut writer, &line)?;
        writer.write_all(b"\n")?;
        insert_lat.push(t0.elapsed());
        ok += 1;
        if (i + 1) % 50 == 0 {
            println!("  [{}/{}] 已写入", i + 1, docs.len());
        }
    }
    writer.flush()?;
    Ok((ok, fail, insert_lat, start.elapsed()))
}

#[tokio::test]
async fn embed_scifact_file() -> Result<()> {
    println!("========== 环境 ==========");
    println!("GGUF       : {}", gguf_path());
    println!("scifact    : {}", scifact_dir().display());
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

    // 3. embed corpus → 单文件
    let corpus_out = scifact_dir().join("corpus_embeddings.jsonl");
    println!("\n[3] embed corpus → {} ...", corpus_out.display());
    let (ok, fail, lat, elapsed) =
        embed_to_jsonl(&model, &mut ctx, &corpus, &corpus_out)?;
    println!(
        "\ncorpus 完成: 成功={} 失败={} 墙钟={:?} 吞吐={:.2} docs/s",
        ok, fail, elapsed, ok as f64 / elapsed.as_secs_f64()
    );
    lat.report("单条耗时");

    // 4. 加载 queries
    println!("\n[4] 加载 scifact queries ...");
    let t = Instant::now();
    let queries = load_queries(MAX_SAMPLES)?;
    println!("queries: {} 条, 加载耗时 {:?}", queries.len(), t.elapsed());

    // 5. embed queries → 单文件
    let queries_out = scifact_dir().join("queries_embeddings.jsonl");
    println!("\n[5] embed queries → {} ...", queries_out.display());
    let (ok_q, fail_q, lat_q, elapsed_q) =
        embed_to_jsonl(&model, &mut ctx, &queries, &queries_out)?;
    println!(
        "\nqueries 完成: 成功={} 失败={} 墙钟={:?} 吞吐={:.2} docs/s",
        ok_q, fail_q, elapsed_q, ok_q as f64 / elapsed_q.as_secs_f64()
    );
    lat_q.report("单条耗时");

    // 6. 汇总 + 文件体积
    println!("\n========== 汇总 ==========");
    let corpus_size = fs::metadata(&corpus_out).map(|m| m.len()).unwrap_or(0);
    let queries_size = fs::metadata(&queries_out).map(|m| m.len()).unwrap_or(0);
    println!("corpus : {} 条, {:.2} docs/s, {:?}, {:.2} MiB",
        ok, ok as f64 / elapsed.as_secs_f64(), elapsed, corpus_size as f64 / 1024.0 / 1024.0);
    println!("queries: {} 条, {:.2} docs/s, {:?}, {:.2} MiB",
        ok_q, ok_q as f64 / elapsed_q.as_secs_f64(), elapsed_q, queries_size as f64 / 1024.0 / 1024.0);
    println!("输出目录: {}", scifact_dir().display());

    Ok(())
}


