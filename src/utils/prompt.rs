pub fn build_short_answer_prompt(question: &str) -> String {
    format!(
        "规则：Piksel AI，回复≤10字，无客套、无拓展、不重复。|||问题：{}|||回答：",
        question
    )
}
