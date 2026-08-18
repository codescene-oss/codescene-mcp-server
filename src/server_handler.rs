use rmcp::model::{
    AnnotateAble, GetPromptRequestParams, GetPromptResult, Implementation, ListPromptsResult,
    ListResourceTemplatesResult, ListResourcesResult, PaginatedRequestParams, Prompt,
    PromptArgument, PromptMessage, PromptMessageRole, RawResource, RawResourceTemplate,
    ReadResourceRequestParams, ReadResourceResult, ResourceContents, ServerCapabilities,
    ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{tool_handler, ErrorData, RoleServer, ServerHandler};

use crate::{config, environment, prompts, skills, CodeSceneServer};

#[tool_handler(router = "self.tool_router")]
impl ServerHandler for CodeSceneServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .build(),
        )
        .with_protocol_version(protocol_version_2025_11_25())
        .with_server_info(Implementation::new(
            "codescene-mcp-server",
            env!("CS_MCP_VERSION"),
        ))
        .with_instructions(build_instructions(
            self.is_standalone,
            config::enabled_tools(&self.config_data).is_some(),
            environment::is_docker(),
        ))
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        Ok(build_prompts_list(environment::is_docker()))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, ErrorData> {
        resolve_prompt(&request.name, environment::is_docker())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(build_resources_list())
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        resolve_resource(&request.uri)
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        Ok(build_resource_templates())
    }
}

fn protocol_version_2025_11_25() -> rmcp::model::ProtocolVersion {
    serde_json::from_str("\"2025-11-25\"").expect("valid MCP protocol version literal")
}

fn build_prompts_list(is_docker: bool) -> ListPromptsResult {
    let mut prompts_list = Vec::new();
    if !is_docker {
        prompts_list.push(Prompt::new(
            "login",
            Some(
                "Sign in to CodeScene with OAuth. Invokes the login tool to open a browser and complete authentication.",
            ),
            Some(vec![PromptArgument::new("context")
                .with_description("Optional context string.")
                .with_required(false)]),
        ));
        prompts_list.push(Prompt::new(
            "switch_account",
            Some(
                "Switch CodeScene Cloud OAuth account. Lists accounts when called without arguments, then switches by name or account ID.",
            ),
            Some(vec![PromptArgument::new("context")
                .with_description("Optional context string (include the account name or ID when known).")
                .with_required(false)]),
        ));
    }
    prompts_list.push(Prompt::new(
        "logout",
        Some("Sign out of CodeScene OAuth. Invokes the logout tool to clear the stored session."),
        Some(vec![PromptArgument::new("context")
            .with_description("Optional context string.")
            .with_required(false)]),
    ));
    prompts_list.push(Prompt::new(
        "review_code_health",
        Some(
            "Review Code Health and assess code quality for the current open file. The file path needs to be sent to the code_health_review MCP tool when using this prompt.",
        ),
        Some(vec![PromptArgument::new("context")
            .with_description("Optional context string.")
            .with_required(false)]),
    ));
    prompts_list.push(Prompt::new(
        "plan_code_health_refactoring",
        Some("Plan a prioritized, low-risk refactoring to remediate detected Code Health issues."),
        Some(vec![PromptArgument::new("context")
            .with_description("Optional context string.")
            .with_required(false)]),
    ));
    ListPromptsResult::with_all_items(prompts_list)
}

fn docker_hides_prompt(name: &str) -> bool {
    matches!(name, "login" | "switch_account")
}

fn resolve_prompt(name: &str, is_docker: bool) -> Result<GetPromptResult, ErrorData> {
    if is_docker && docker_hides_prompt(name) {
        return Err(ErrorData::new(
            rmcp::model::ErrorCode::INVALID_REQUEST,
            format!("Unknown prompt: {name}"),
            None,
        ));
    }
    let text = prompts::resolve_prompt_text(name).ok_or_else(|| {
        ErrorData::new(
            rmcp::model::ErrorCode::INVALID_REQUEST,
            format!("Unknown prompt: {name}"),
            None,
        )
    })?;
    Ok(GetPromptResult::new(vec![PromptMessage::new_text(
        PromptMessageRole::User,
        text,
    )]))
}

fn build_resources_list() -> ListResourcesResult {
    let skill_list = skills::load_skills();
    let resources = skill_list
        .iter()
        .flat_map(|skill| {
            let main_uri = skills::skill_uri(&skill.name, "SKILL.md");
            let manifest_uri_str = skills::manifest_uri(&skill.name);
            let manifest_name = format!("{} manifest", skill.name);
            let manifest_desc = format!("File manifest for the {} skill", skill.name);
            vec![
                RawResource::new(main_uri, &skill.name)
                    .with_description(&skill.description)
                    .with_mime_type("text/markdown")
                    .with_size(skill.content.len() as u32)
                    .no_annotation(),
                RawResource::new(manifest_uri_str, manifest_name)
                    .with_description(manifest_desc)
                    .with_mime_type("application/json")
                    .no_annotation(),
            ]
        })
        .collect();
    ListResourcesResult {
        resources,
        next_cursor: None,
        meta: None,
    }
}

fn resolve_resource(uri: &str) -> Result<ReadResourceResult, ErrorData> {
    let (skill_name, path) = skills::parse_skill_uri(uri)
        .ok_or_else(|| ErrorData::resource_not_found(format!("Invalid skill URI: {uri}"), None))?;

    let skill_list = skills::load_skills();
    let skill = skill_list
        .iter()
        .find(|s| s.name == skill_name)
        .ok_or_else(|| {
            ErrorData::resource_not_found(format!("Skill not found: {skill_name}"), None)
        })?;

    match path {
        "SKILL.md" => Ok(ReadResourceResult::new(vec![ResourceContents::text(
            skill.content,
            uri,
        )
        .with_mime_type("text/markdown")])),
        "_manifest" => {
            let manifest = skills::build_manifest(skill);
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                manifest, uri,
            )
            .with_mime_type("application/json")]))
        }
        _ => Err(ErrorData::resource_not_found(
            format!("File not found in skill {skill_name}: {path}"),
            None,
        )),
    }
}

fn build_resource_templates() -> ListResourceTemplatesResult {
    let template = RawResourceTemplate::new("skill://{skill_name}/{path}", "Skill file")
        .with_description(
            "Access a specific file within a CodeScene skill. \
         Use skill_name from the resource list and path from the manifest.",
        )
        .with_mime_type("text/markdown");
    ListResourceTemplatesResult {
        resource_templates: vec![template.no_annotation()],
        next_cursor: None,
        meta: None,
    }
}

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
             - code_ownership_for_path: Find code owners.\n",
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
        "CodeScene MCP Server - Code Health analysis tools for AI-assisted development.\n\n\
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
    fn protocol_version_is_2025_11_25() {
        assert_eq!(protocol_version_2025_11_25().as_str(), "2025-11-25");
    }

    fn prompt_names(is_docker: bool) -> Vec<String> {
        build_prompts_list(is_docker)
            .prompts
            .iter()
            .map(|p| p.name.to_string())
            .collect()
    }

    #[test]
    fn prompts_list_contains_expected_prompts() {
        let names = prompt_names(false);
        assert_eq!(names.len(), 5);
        for expected in [
            "login",
            "switch_account",
            "logout",
            "review_code_health",
            "plan_code_health_refactoring",
        ] {
            assert!(names.iter().any(|n| n == expected), "missing {expected}");
        }
    }

    #[test]
    fn prompts_list_omits_login_and_switch_account_in_docker() {
        let names = prompt_names(true);
        assert_eq!(names.len(), 3);
        assert!(!names.iter().any(|n| n == "login" || n == "switch_account"));
        for expected in ["logout", "review_code_health", "plan_code_health_refactoring"] {
            assert!(names.iter().any(|n| n == expected), "missing {expected}");
        }
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
            rmcp::model::PromptMessageContent::Text { text } => text.as_str(),
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
    fn resolve_switch_account_prompt_succeeds() {
        assert_prompt_text_contains("switch_account", "switch_account tool");
    }

    #[test]
    fn resolve_switch_account_prompt_fails_in_docker() {
        let result = resolve_prompt("switch_account", true);
        assert!(result.is_err());
    }

    #[test]
    fn resources_list_contains_all_skills() {
        let result = build_resources_list();
        let skills = skills::load_skills();
        assert_eq!(result.resources.len(), skills.len() * 2);
    }

    #[test]
    fn resources_have_correct_mime_types() {
        let result = build_resources_list();
        let md_resources: Vec<_> = result
            .resources
            .iter()
            .filter(|r| r.uri.ends_with("/SKILL.md"))
            .collect();
        assert!(!md_resources.is_empty());
        for r in &md_resources {
            assert_eq!(r.mime_type.as_deref(), Some("text/markdown"));
        }
        let manifest_resources: Vec<_> = result
            .resources
            .iter()
            .filter(|r| r.uri.ends_with("/_manifest"))
            .collect();
        for r in &manifest_resources {
            assert_eq!(r.mime_type.as_deref(), Some("application/json"));
        }
    }

    #[test]
    fn read_skill_md_resource() {
        let uri = "skill://safeguarding-ai-generated-code/SKILL.md";
        let result = resolve_resource(uri).unwrap();
        assert_eq!(result.contents.len(), 1);
    }

    #[test]
    fn read_manifest_resource() {
        let uri = "skill://safeguarding-ai-generated-code/_manifest";
        let result = resolve_resource(uri).unwrap();
        assert_eq!(result.contents.len(), 1);
    }

    #[test]
    fn read_unknown_skill_returns_error() {
        let result = resolve_resource("skill://nonexistent/SKILL.md");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Skill not found"));
    }

    #[test]
    fn read_invalid_uri_returns_error() {
        let result = resolve_resource("file:///etc/passwd");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Invalid skill URI"));
    }

    #[test]
    fn read_unknown_path_in_skill_returns_error() {
        let result = resolve_resource("skill://safeguarding-ai-generated-code/unknown.txt");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("File not found"));
    }

    #[test]
    fn resource_templates_contains_skill_template() {
        let result = build_resource_templates();
        assert_eq!(result.resource_templates.len(), 1);
    }
}
