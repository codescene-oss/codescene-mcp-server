pub mod plan_code_health_refactoring;
pub mod review_code_health;

pub fn resolve_prompt_text(name: &str) -> Option<&'static str> {
    match name {
        "review_code_health" => Some(review_code_health::TEXT),
        "plan_code_health_refactoring" => Some(plan_code_health_refactoring::TEXT),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_prompt_text;

    #[test]
    fn resolve_review_prompt() {
        assert!(resolve_prompt_text("review_code_health").is_some());
    }

    #[test]
    fn resolve_refactoring_prompt() {
        assert!(resolve_prompt_text("plan_code_health_refactoring").is_some());
    }

    #[test]
    fn resolve_unknown_prompt() {
        assert!(resolve_prompt_text("unknown").is_none());
    }
}
