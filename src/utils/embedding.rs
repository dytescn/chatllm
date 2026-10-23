use anyhow::{Context, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use std::sync::{Mutex, OnceLock};

pub const MODEL_PATH: &str = "./models/qwen3-embed/qwen3-embedding-0.6.gguf";
pub const DIM: usize = 1024;

struct EmbeddingEngine {
    _backend: &'static LlamaBackend,
    model: &'static LlamaModel,
    ctx: llama_cpp_2::context::LlamaContext<'static>,
}

static EMBEDDING_ENGINE: OnceLock<Mutex<EmbeddingEngine>> = OnceLock::new();

fn get_embedding_engine() -> Result<&'static Mutex<EmbeddingEngine>> {
    if let Some(engine) = EMBEDDING_ENGINE.get() {
        return Ok(engine);
    }

    let backend: &'static mut LlamaBackend = Box::leak(Box::new(
        LlamaBackend::init().context("初始化 LlamaBackend 失败")?,
    ));
    let model_params = LlamaModelParams::default();
    let model: &'static mut LlamaModel = Box::leak(Box::new(
        LlamaModel::load_from_file(backend, MODEL_PATH, &model_params)
            .with_context(|| format!("加载 embedding 模型失败: {}", MODEL_PATH))?,
    ));

    let ctx_params = LlamaContextParams::default()
        .with_embeddings(true)
        .with_n_ctx(std::num::NonZeroU32::new(2048));
    let backend_ref: &'static LlamaBackend = backend;
    let model_ref: &'static LlamaModel = &*model;
    let ctx = model_ref
        .new_context(backend_ref, ctx_params)
        .context("创建 LlamaContext 失败")?;

    Ok(EMBEDDING_ENGINE.get_or_init(|| {
        Mutex::new(EmbeddingEngine {
            _backend: backend_ref,
            model: model_ref,
            ctx,
        })
    }))
}

/// 把单条文本编码成归一化后的 embedding 向量。
pub fn embed_text(text: &str) -> Result<Vec<f32>> {
    let engine = get_embedding_engine()?;
    let mut guard = engine.lock().unwrap_or_else(|e| e.into_inner());
    let model = guard.model;
    let ctx = &mut guard.ctx;

    let tokens = model.vocab().tokenize(text.as_bytes(), true, false);
    if tokens.is_empty() {
        anyhow::bail!("文本 token 为空");
    }

    ctx.clear_kv_cache();
    let mut batch = LlamaBatch::new(tokens.len().max(1), 1);
    batch
        .add_sequence(&tokens, 0, false)
        .context("添加 batch 失败")?;
    ctx.decode(&mut batch).context("llama_decode 失败")?;

    let emb = ctx.embeddings_seq_ith(0).context("获取 embeddings 失败")?;
    let mut vector = emb.to_vec();

    let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in vector.iter_mut() {
            *x /= norm;
        }
    }

    if vector.len() != DIM {
        anyhow::bail!("embedding 维度异常: 期望 {}，实际 {}", DIM, vector.len());
    }

    Ok(vector)
}
