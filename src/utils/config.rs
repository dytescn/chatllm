#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub model_path: &'static str,
    pub prompt: &'static str,
    pub max_gen_tokens: i32,
    pub ctx_size: u32,
    pub cpu_threads: i32,
    pub rng_seed: u32,
    // 采样惩罚参数
    pub penalty_last_n: i32,
    pub repeat_penalty: f32,
    pub penalty_alpha: f32,
    pub dry_base: f32,
    pub temp: f32,
    pub top_p: f32,
    pub top_k: usize,
}
impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model_path: "./models/qwen3-embed/qwen3-embedding-0.6.gguf",
            prompt: "",
            max_gen_tokens: 128,
            ctx_size: 2048,
            cpu_threads: 8,
            rng_seed: 1234,

            penalty_last_n: 128,
            repeat_penalty: 1.3,
            penalty_alpha: 0.0,
            dry_base: 1.0,
            temp: 0.1,
            top_p: 0.25,
            top_k: 4,
        }
    }
}
