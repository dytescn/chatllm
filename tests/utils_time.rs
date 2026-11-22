// tests/utils_time.rs —— utils::time 的单元测试
use chatllm::utils::time::now_ms;

#[test]
fn now_ms_is_unix_milliseconds() {
    let now = now_ms();
    // 2023-11-14 之后，且远小于 2100 年
    assert!(now > 1_700_000_000_000, "now_ms 应为毫秒时间戳: {now}");
    assert!(now < 4_000_000_000_000, "now_ms 异常偏大: {now}");
}

#[test]
fn now_ms_does_not_go_backwards() {
    let first = now_ms();
    let second = now_ms();
    assert!(
        second >= first,
        "时间戳不应倒退: {first} -> {second}"
    );
}
