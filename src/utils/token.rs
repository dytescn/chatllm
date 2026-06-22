use super::config::LlmConfig;
use anyhow::{Context, Result};
use encoding_rs::UTF_8;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::{AddBos, LlamaModel};
use std::io::Write;

pub fn tokenize_prompt(
    model: &LlamaModel,
    prompt: &str,
) -> Result<Vec<llama_cpp_2::token::LlamaToken>> {
    model
        .str_to_token(prompt, AddBos::Always)
        .context("Prompt分词失败")
}

pub fn print_prompt_tokens(
    model: &LlamaModel,
    tokens: &[llama_cpp_2::token::LlamaToken],
) -> Result<()> {
    let mut decoder = UTF_8.new_decoder();
    for token in tokens {
        eprint!(
            "{}",
            model.token_to_piece(*token, &mut decoder, true, None)?
        );
    }
    std::io::stderr().flush()?;
    eprintln!("\n=== 模型输出 ===");
    Ok(())
}

pub fn build_prompt_batch(tokens: &[llama_cpp_2::token::LlamaToken]) -> Result<LlamaBatch<'_>> {
    let mut batch = LlamaBatch::new(512, 1);
    let last_idx = (tokens.len() - 1) as i32;
    for (i, token) in (0i32..).zip(tokens) {
        let is_last = i == last_idx;
        batch.add(*token, i, &[0], is_last)?;
    }
    Ok(batch)
}

pub fn check_kv_capacity(tokens_len: usize, cfg: &LlmConfig) -> Result<()> {
    let n_cxt = cfg.ctx_size as i32;
    let n_kv_req = tokens_len as i32 + cfg.max_gen_tokens;
    eprintln!("\n上下文容量: {n_cxt}, 需要KV缓存: {n_kv_req}");

    if n_kv_req > n_cxt {
        anyhow::bail!("KV缓存不足，调大ctx_size或减少生成token");
    }
    if tokens_len >= cfg.max_gen_tokens as usize {
        anyhow::bail!("Prompt过长，超过最大生成限制");
    }
    Ok(())
}
