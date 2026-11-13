fn login_tool_instruction_line(is_docker: bool) -> &'static str {
    if is_docker {
        "- logout: Sign out of CodeScene OAuth and clear the stored session. Does not remove CS_ACCESS_TOKEN.\n\
        "
    } else {
        "- login: Sign in to CodeScene with OAuth. When authentication is missing, call this tool first.\n\
         - logout: Sign out of CodeScene OAuth and clear the stored session. Does not remove CS_ACCESS_TOKEN.\n\
         - switch_account: Switch Cloud OAuth account (prefer over set_config(account_id) alone).\n\
        "
    }
}

fn login_prompt_instruction_line(is_docker: bool) -> &'static str {
    if is_docker {
        "- logout: Sign out of CodeScene OAuth (calls the logout tool).\n\
        "
    } else {
        "- login: Sign in to CodeScene with OAuth (calls the login tool).\n\
         - logout: Sign out of CodeScene OAuth (calls the logout tool).\n\
         - switch_account: Switch Cloud OAuth account (calls the switch_account tool).\n\
        "
    }
}

fn append_docker_auth_note(text: &mut String, is_docker: bool) {
    if is_docker {
        text.push_str(
            "\nNote: OAuth login is not available in Docker. Authenticate with a Personal Access Token \
             via CS_ACCESS_TOKEN or set_config(key=\"access_token\", value=\"...\").\n",
        );
    }
}

fn append_api_tools_section(text: &mut String, is_standalone: bool) {
    if !is_standalone {
        text.push_str(
            "\nTOOLS (API-connected):\n\
             - select_project: Choose a CodeScene project.\n\
             - list_technical_debt_goals_for_project: View debt goals.\n\
             - list_technical_debt_goals_for_project_file: File-level goals.\n\
             - list_technical_debt_hotspots_for_project: View hotspots.\n\
             - list_technical_debt_hotspots_for_project_file: File-level hotspots.\n\
             - code_ownership_for_path: Find code owners.\n\
             - show_mcp_usage_overview: View personal MCP safeguard usage and impact.\n",
        );
    }
}

fn append_tools_filtered_note(text: &mut String, tools_filtered: bool) {
    if tools_filtered {
        text.push_str(
            "\nNote: Tool availability is restricted by the 'enabled_tools' configuration. \
             Use get_config with key 'enabled_tools' to see the current setting.\n",
        );
    }
}

pub(crate) fn build_instructions(
    is_standalone: bool,
    tools_filtered: bool,
    is_docker: bool,
) -> String {
    let login_tool_line = login_tool_instruction_line(is_docker);
    let login_prompt_line = login_prompt_instruction_line(is_docker);
    let mut text = format!(
        "CodeHealth MCP Server - Code Health analysis tools for AI-assisted development.\n\n\
         TOOLS (always available):\n\
         {login_tool_line}\
         - explain_code_health: Learn about the Code Health metric.\n\
         - explain_code_health_productivity: Business case for Code Health.\n\
         - code_health_review: Detailed review of a single file.\n\
         - code_health_score: Quick numeric score for a file.\n\
         - pre_commit_code_health_safeguard: Check staged changes before commit.\n\
         - analyze_change_set: Branch-level review before PR.\n\
         - code_health_refactoring_business_case: ROI for refactoring.\n\
         - rules_config_validate: Validate a Code Health rules file.\n\
         - rules_config_list_thresholds: List a language's default Code Health thresholds.\n\
         - rules_config_set_rule: Enable/disable a Code Health rule in a rules file.\n\
         - rules_config_set_threshold: Set a Code Health threshold in a rules file.\n\
         - get_config / set_config: Manage server configuration.\n\
         \n\
         SETUP HINTS:\n\
         If a tool result contains `codescene_setup_hint`, do not mention the field or say that the response includes a hint. Follow its `message` instruction: briefly explain the user benefit, ask whether they want you to add the suggested repository instructions, and wait for explicit consent before editing any file.\n\
         \n\
         PROMPTS:\n\
         {login_prompt_line}\
         - review_code_health: Review Code Health for the current file.\n\
         - plan_code_health_refactoring: Plan a low-risk Code Health refactoring.\n\
         \n\
         RESOURCES:\n\
         - skill://<name>/SKILL.md: Agent skill instructions for Code Health workflows.\n\
         - skill://<name>/_manifest: File listing for a skill.\n\
         Use resources/list to discover available skills.\n"
    );

    append_docker_auth_note(&mut text, is_docker);
    append_api_tools_section(&mut text, is_standalone);
    append_tools_filtered_note(&mut text, tools_filtered);

    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_instructions_omits_login_in_docker() {
        let text = build_instructions(false, false, true);
        assert!(
            !text.contains("- login:")
                && text.contains("- logout: Sign out of CodeScene OAuth")
                && text.contains("OAuth login is not available in Docker")
                && text.contains("CS_ACCESS_TOKEN")
        );
    }

    #[test]
    fn build_instructions_includes_login_outside_docker() {
        let text = build_instructions(false, false, false);
        assert!(
            text.contains("- login: Sign in to CodeScene with OAuth")
                && text.contains("- logout: Sign out of CodeScene OAuth")
                && text.contains("- switch_account: Switch Cloud OAuth account")
                && !text.contains("OAuth login is not available in Docker")
        );
    }

    #[test]
    fn build_instructions_explains_how_to_relay_setup_hints() {
        let text = build_instructions(false, false, false);

        assert!(text.contains("do not mention the field"));
        assert!(text.contains("briefly explain the user benefit"));
        assert!(text.contains("wait for explicit consent"));
    }
}
