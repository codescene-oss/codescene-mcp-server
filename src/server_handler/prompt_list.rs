use rmcp::model::{
    GetPromptResult, ListPromptsResult, Prompt, PromptArgument, PromptMessage, Role,
};
use rmcp::ErrorData;

use crate::prompts;

const DEFAULT_CONTEXT_DESCRIPTION: &str = "Optional context string.";

fn context_prompt(name: &str, description: &str, context_description: &str) -> Prompt {
    Prompt::new(
        name,
        Some(description),
        Some(vec![PromptArgument::new("context")
            .with_description(context_description)
            .with_required(false)]),
    )
}

pub(super) fn build_prompts_list(is_docker: bool) -> ListPromptsResult {
    let mut prompts_list = Vec::new();
    if !is_docker {
        prompts_list.push(context_prompt(
            "login",
            "Sign in to CodeScene with OAuth. Invokes the login tool to open a browser and complete authentication.",
            DEFAULT_CONTEXT_DESCRIPTION,
        ));
        prompts_list.push(context_prompt(
            "switch_account",
            "Switch CodeScene Cloud OAuth account. Lists accounts when called without arguments, then switches by name or account ID.",
            "Optional context string (include the account name or ID when known).",
        ));
    }
    prompts_list.push(context_prompt(
        "logout",
        "Sign out of CodeScene OAuth. Invokes the logout tool to clear the stored session.",
        DEFAULT_CONTEXT_DESCRIPTION,
    ));
    prompts_list.push(context_prompt(
        "review_code_health",
        "Review Code Health and assess code quality for the current open file. The file path needs to be sent to the code_health_review MCP tool when using this prompt.",
        DEFAULT_CONTEXT_DESCRIPTION,
    ));
    prompts_list.push(context_prompt(
        "plan_code_health_refactoring",
        "Plan a prioritized, low-risk refactoring to remediate detected Code Health issues.",
        DEFAULT_CONTEXT_DESCRIPTION,
    ));
    ListPromptsResult::with_all_items(prompts_list)
}

fn docker_hides_prompt(name: &str) -> bool {
    matches!(name, "login" | "switch_account")
}

fn unknown_prompt(name: &str) -> ErrorData {
    ErrorData::new(
        rmcp::model::ErrorCode::INVALID_REQUEST,
        format!("Unknown prompt: {name}"),
        None,
    )
}

pub(super) fn resolve_prompt(name: &str, is_docker: bool) -> Result<GetPromptResult, ErrorData> {
    if is_docker && docker_hides_prompt(name) {
        return Err(unknown_prompt(name));
    }
    let text = prompts::resolve_prompt_text(name).ok_or_else(|| unknown_prompt(name))?;
    Ok(GetPromptResult::new(vec![PromptMessage::new_text(
        Role::User,
        text,
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt_names(is_docker: bool) -> Vec<String> {
        build_prompts_list(is_docker)
            .prompts
            .iter()
            .map(|p| p.name.to_string())
            .collect()
    }

    fn assert_prompt_names(is_docker: bool, expected: &[&str]) {
        let names = prompt_names(is_docker);
        assert_eq!(names.len(), expected.len());
        for name in expected {
            assert!(names.iter().any(|n| n == name), "missing {name}");
        }
    }

    #[test]
    fn prompts_list_contains_expected_prompts() {
        assert_prompt_names(
            false,
            &[
                "login",
                "switch_account",
                "logout",
                "review_code_health",
                "plan_code_health_refactoring",
            ],
        );
    }

    #[test]
    fn prompts_list_omits_login_and_switch_account_in_docker() {
        assert_prompt_names(
            true,
            &[
                "logout",
                "review_code_health",
                "plan_code_health_refactoring",
            ],
        );
    }

    #[test]
    fn resolve_known_prompt_succeeds() {
        let result = resolve_prompt("review_code_health", false);
        assert!(result.is_ok());
        let prompt = result.unwrap();
        assert!(!prompt.messages.is_empty());
    }

    fn assert_prompt_text_contains(name: &str, expected: &str) {
        let result = resolve_prompt(name, false);
        assert!(result.is_ok());
        let prompt = result.unwrap();
        let text = match &prompt.messages[0].content {
            rmcp::model::ContentBlock::Text(content) => content.text.as_str(),
            _ => panic!("expected text content"),
        };
        assert!(
            text.contains(expected),
            "prompt {name} missing {expected:?}, got: {text}"
        );
    }

    #[test]
    fn resolve_login_prompt_succeeds() {
        assert_prompt_text_contains("login", "login tool");
    }

    #[test]
    fn resolve_login_prompt_fails_in_docker() {
        let result = resolve_prompt("login", true);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Unknown prompt"));
    }

    #[test]
    fn resolve_logout_prompt_succeeds() {
        assert_prompt_text_contains("logout", "logout tool");
    }

    #[test]
    fn resolve_unknown_prompt_returns_error() {
        let result = resolve_prompt("nonexistent_prompt", false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Unknown prompt"));
    }

    #[test]
    fn resolve_switch_account_prompt_succeeds() {
        assert_prompt_text_contains("switch_account", "switch_account tool");
    }

    #[test]
    fn resolve_switch_account_prompt_fails_in_docker() {
        let result = resolve_prompt("switch_account", true);
        assert!(result.is_err());
    }
}
