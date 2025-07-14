use anyhow::Result;
use llama_cpp_2::{
    llama::{ContextParams, ModelParams},
    InferenceSession, LlamaContext, LlamaModel,
};

fn main() -> Result<()> {
    // 1. 加载模型
    let model_path = "models/tinychat.gguf";
    let model = LlamaModel::load_from_file(
        model_path,
        ModelParams::default()
            .n_gpu_layers(0)        // 全 CPU，GPU 请按需调整
            .use_mmap(true),        // 省内存
    )?;

    // 2. 创建推理上下文
    let ctx = LlamaContext::new(&model, ContextParams::default().ctx_len(2048))?;

    // 3. 启动对话 Session
    let mut session = InferenceSession::new(&ctx);

    // 4. 系统 & 用户提示词（采用 llama.cpp 默认 chat 模板）
    let system_prompt = "You are a helpful assistant.";
    let user_prompt   = "你好，简单介绍一下你自己。";

    session.feed_prompt(system_prompt)?;
    session.feed_prompt(user_prompt)?;

    // 5. 流式生成
    print!("Assistant: ");
    while let Some(token) = session.infer_next_token(Default::default())? {
        // token.text() 自动转换 UTF‑8
        print!("{}", token.text());
        // 可在此处添加 break 条件 / 采样参数
    }
    println!();
    Ok(())
}