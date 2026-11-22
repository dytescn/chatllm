// src/utils/time.rs
//! 时间工具：统一 Unix 毫秒时间戳。

use std::time::{SystemTime, UNIX_EPOCH};

/// 当前 Unix 毫秒时间戳。
///
/// 系统时间早于 Unix epoch 时返回 `0`，避免在异常时钟下 panic。
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
