use crate::utils::config::LlmConfig;
use crate::utils::generate::generate_completion_capture;
use crate::utils::loader::{create_ctx, init_backend, load_model};
use crate::utils::prompt::build_short_answer_prompt;
use crate::utils::token::{build_prompt_batch, check_kv_capacity, tokenize_prompt};
use anyhow::Result;
use llama_cpp_2::{send_logs_to_tracing, LogOptions};

// 新增：供FFI调用，传入用户问题，返回(回答文本, prompt token数, 生成token数)
pub fn run_inference_capture(user_question: &str) -> Result<(String, usize, usize)> {
    send_logs_to_tracing(LogOptions::default().with_logs_enabled(false));
    let cfg = LlmConfig::default();
    let full_prompt = build_short_answer_prompt(user_question);

    let backend = init_backend()?;
    let model = load_model(&backend, &cfg)?;
    let mut ctx = create_ctx(&model, &backend, &cfg)?;

    let tokens = tokenize_prompt(&model, &full_prompt)?;
    let prompt_tok = tokens.len();
    check_kv_capacity(prompt_tok, &cfg)?;
    let batch = build_prompt_batch(&tokens)?;

    let mut output_buf = String::new();
    let gen_tok = generate_completion_capture(&model, &mut ctx, batch, &cfg, &mut output_buf)?;
    Ok((output_buf, prompt_tok, gen_tok))
}
