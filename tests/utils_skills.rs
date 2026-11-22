// tests/utils_skills.rs —— utils::skills 的测试
use chatllm::utils::skills::skills_for;

#[test]
fn skills_for_returns_none_without_keys() {
    assert!(skills_for(None, None).is_none());
    assert!(skills_for(Some("   "), Some(" ")).is_none());
}
