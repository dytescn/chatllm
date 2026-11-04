// tests/utils_check.rs —— utils::check 的供应商解析与提示文案测试
use chatllm::utils::check::{
    parse_supplier, AI_SUPPLIER_KEY, MISSING_SUPPLIER_MESSAGE, SETTING_ROUTE,
};
use chatllm::utils::cloud::Supplier;

#[test]
fn parses_supported_suppliers() {
    assert_eq!(parse_supplier(Some("deepseek")), Some(Supplier::Deepseek));
    assert_eq!(
        parse_supplier(Some("siliconflow_t2t")),
        Some(Supplier::SiliconFlow)
    );
    assert_eq!(
        parse_supplier(Some("  siliconflow_t2t  ")),
        Some(Supplier::SiliconFlow)
    );
}

#[test]
fn treats_unset_and_unknown_as_missing() {
    assert_eq!(parse_supplier(None), None);
    assert_eq!(parse_supplier(Some("")), None);
    assert_eq!(parse_supplier(Some("   ")), None);
    assert_eq!(parse_supplier(Some("siliconflow")), None);
    assert_eq!(parse_supplier(Some("openai")), None);
}

#[test]
fn missing_reply_contains_setting_route() {
    assert!(MISSING_SUPPLIER_MESSAGE.contains("请选择设置AI供应商"));
    assert!(MISSING_SUPPLIER_MESSAGE.contains("AppKey"));
    assert!(MISSING_SUPPLIER_MESSAGE.contains(SETTING_ROUTE));
    assert_eq!(SETTING_ROUTE, "/setting");
    assert_eq!(AI_SUPPLIER_KEY, "ai_supplier");
}
