pub mod login;
pub mod logout;
pub mod plan_code_health_refactoring;
pub mod review_code_health;
pub mod switch_account;

pub fn resolve_prompt_text(name: &str) -> Option<&'static str> {
    match name {
        "login" => Some(login::TEXT),
        "logout" => Some(logout::TEXT),
        "switch_account" => Some(switch_account::TEXT),
        "review_code_health" => Some(review_code_health::TEXT),
        "plan_code_health_refactoring" => Some(plan_code_health_refactoring::TEXT),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_prompt_text;

    #[test]
    fn resolve_login_prompt() {
        assert!(resolve_prompt_text("login").is_some());
    }

    #[test]
    fn resolve_logout_prompt() {
        assert!(resolve_prompt_text("logout").is_some());
    }

    #[test]
    fn resolve_switch_account_prompt() {
        let text = resolve_prompt_text("switch_account").expect("switch_account prompt");
        assert!(text.contains("switch_account tool"));
        assert!(text.contains("account_id"));
        assert!(text.contains("set_config"));
    }

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
