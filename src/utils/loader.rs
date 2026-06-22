use super::config::LlmConfig;
use anyhow::{Context, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use std::num::NonZeroU32;
use std::path::PathBuf;

pub fn init_backend() -> Result<LlamaBackend> {
    LlamaBackend::init().context("初始化llama后端失败")
}

pub fn load_model(backend: &LlamaBackend, cfg: &LlmConfig) -> Result<LlamaModel> {
    let model_path = PathBuf::from(cfg.model_path);
    if !model_path.exists() {
        anyhow::bail!("模型文件不存在: {}", cfg.model_path);
    }
    let model_params = LlamaModelParams::default();
    let model = LlamaModel::load_from_file(backend, model_path, &model_params)
        .context("加载GGUF模型失败")?;
    Ok(model)
}

pub fn create_ctx<'a>(
    model: &'a LlamaModel,
    backend: &'a LlamaBackend,
    cfg: &LlmConfig,
) -> Result<llama_cpp_2::context::LlamaContext<'a>> {
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(Some(NonZeroU32::new(cfg.ctx_size).unwrap()))
        .with_n_threads(cfg.cpu_threads)
        .with_n_threads_batch(cfg.cpu_threads);
    model
        .new_context(backend, ctx_params)
        .context("创建推理上下文失败")
}
