use super::config::LlmConfig;
use anyhow::{Context, Result};
use encoding_rs::UTF_8;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::{ggml_time_us, llama_batch::LlamaBatch, sampling::LlamaSampler};
use std::io::Write;
use std::time::Duration;

/// 构建采样器
pub fn build_sampler(cfg: &LlmConfig) -> LlamaSampler {
    LlamaSampler::chain_simple([
        LlamaSampler::dist(cfg.rng_seed),
        LlamaSampler::penalties(
            cfg.penalty_last_n,
            cfg.repeat_penalty,
            cfg.penalty_alpha,
            cfg.dry_base,
        ),
        LlamaSampler::temp(cfg.temp),
        LlamaSampler::top_p(cfg.top_p, cfg.top_k),
        LlamaSampler::greedy(),
    ])
}

pub fn generate_completion(
    model: &LlamaModel,
    ctx: &mut LlamaContext,
    mut batch: LlamaBatch,
    cfg: &LlmConfig,
) -> Result<()> {
    ctx.decode(&mut batch).context("Prompt decode失败")?;
    let mut sampler = build_sampler(cfg);

    let mut decoder = UTF_8.new_decoder();
    let mut n_cur = batch.n_tokens();
    let mut n_decode = 0;
    let mut char_count = 0; // 新增：统计输出汉字数量
    let t_start = ggml_time_us();

    while n_cur <= cfg.max_gen_tokens {
        let token = sampler.sample(ctx, batch.n_tokens() - 1);
        sampler.accept(token);

        if model.is_eog_token(token) {
            break;
        }

        let text = model.token_to_piece(token, &mut decoder, true, None)?;

        // 新增1：检测分隔符|||，一旦出现立刻停止（防止输出提示词规则）
        if text.contains("|||") {
            break;
        }

        print!("{}", text);
        std::io::stdout().flush()?;

        // 新增2：累计字符，超过10字强制截断，保证极短回答
        char_count += text.chars().count();
        if char_count >= 10 {
            break;
        }

        batch.clear();
        batch.add(token, n_cur, &[0], true)?;
        n_cur += 1;
        ctx.decode(&mut batch).context("decode推理失败")?;
        n_decode += 1;
    }

    // 输出性能
    let t_end = ggml_time_us();
    let cost = Duration::from_micros((t_end - t_start) as u64);
    let sec = cost.as_secs_f32();
    let speed = if sec > 0.001 {
        n_decode as f32 / sec
    } else {
        0.0
    };
    eprintln!(
        "\n\n生成完成：{} tokens | 耗时 {:.2}s | 速度 {:.2} t/s",
        n_decode, sec, speed
    );
    println!("{}", ctx.timings());
    Ok(())
}

/// 捕获输出文本到字符串，不打印控制台，返回生成token数量
pub fn generate_completion_capture(
    model: &LlamaModel,
    ctx: &mut LlamaContext,
    mut batch: LlamaBatch<'_>,
    cfg: &LlmConfig,
    out_buf: &mut String,
) -> Result<usize> {
    ctx.decode(&mut batch).context("Prompt decode失败")?;
    let mut sampler = build_sampler(cfg);

    let mut decoder = UTF_8.new_decoder();
    let mut n_cur = batch.n_tokens();
    let mut n_decode = 0;
    let mut char_count = 0;
    let _t_start = ggml_time_us();

    while n_cur <= cfg.max_gen_tokens {
        let token = sampler.sample(ctx, batch.n_tokens() - 1);
        sampler.accept(token);

        if model.is_eog_token(token) {
            break;
        }

        let text = model.token_to_piece(token, &mut decoder, true, None)?;

        if text.contains("|||") {
            break;
        }

        // 存入缓冲区，不打印stdout
        out_buf.push_str(&text);

        char_count += text.chars().count();
        if char_count >= 10 {
            break;
        }

        batch.clear();
        batch.add(token, n_cur, &[0], true)?;
        n_cur += 1;
        ctx.decode(&mut batch).context("decode推理失败")?;
        n_decode += 1;
    }

    Ok(n_decode)
}
