use anyhow::{Context, Result};

/// 在独立线程里执行重计算，避免占用宿主请求线程的小栈。
pub fn run_with_large_stack<T, F>(f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    let handle = std::thread::Builder::new()
        .name("chatllm-worker".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .context("创建 worker 线程失败")?;

    handle
        .join()
        .map_err(|_| anyhow::anyhow!("worker 线程 panic"))?
}
