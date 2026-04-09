use rmcp::model::{
    GetPromptRequestParams, GetPromptResult, Implementation, ListPromptsResult,
    ListResourcesResult, PaginatedRequestParams, Prompt, PromptArgument, PromptMessage,
    PromptMessageRole, RawResource, ReadResourceRequestParams, ReadResourceResult,
    ResourceContents, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{tool_handler, ErrorData, RoleServer, ServerHandler};

use crate::{config, prompts, resources, CodeSceneServer};

#[tool_handler]
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
        ))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        use rmcp::model::AnnotateAble;
        let resources = vec![
            RawResource::new(
                resources::HOW_IT_WORKS_URI,
                extract_md_title(resources::HOW_IT_WORKS),
            )
            .with_description(
                "Explains CodeScene's Code Health metric for assessing code quality and maintainability for both human devs and AI.",
            )
            .with_mime_type("text/markdown")
            .no_annotation(),
            RawResource::new(
                resources::BUSINESS_CASE_URI,
                extract_md_title(resources::BUSINESS_CASE),
            )
            .with_description("Describes how to build a business case for Code Health improvements.")
            .with_mime_type("text/markdown")
            .no_annotation(),
        ];
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        let uri = request.uri.as_str();
        let content = resolve_resource_content(uri)?;
        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            content, uri,
        )]))
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        let prompts_list = vec![
            Prompt::new(
                "review_code_health",
                Some(
                    "Review Code Health and assess code quality for the current open file. The file path needs to be sent to the code_health_review MCP tool when using this prompt.",
                ),
                Some(vec![PromptArgument::new("context")
                    .with_description("Optional context string.")
                    .with_required(false)]),
            ),
            Prompt::new(
                "plan_code_health_refactoring",
                Some("Plan a prioritized, low-risk refactoring to remediate detected Code Health issues."),
                Some(vec![PromptArgument::new("context")
                    .with_description("Optional context string.")
                    .with_required(false)]),
            ),
        ];
        Ok(ListPromptsResult::with_all_items(prompts_list))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, ErrorData> {
        let text = prompts::resolve_prompt_text(&request.name).ok_or_else(|| {
            ErrorData::new(
                rmcp::model::ErrorCode::INVALID_REQUEST,
                format!("Unknown prompt: {}", request.name),
                None,
            )
        })?;
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            text,
        )]))
    }
}

fn protocol_version_2025_11_25() -> rmcp::model::ProtocolVersion {
    serde_json::from_str("\"2025-11-25\"").expect("valid MCP protocol version literal")
}

pub(crate) fn resolve_resource_content(uri: &str) -> Result<&'static str, ErrorData> {
    if uri == resources::HOW_IT_WORKS_URI {
        Ok(resources::HOW_IT_WORKS)
    } else if uri == resources::BUSINESS_CASE_URI {
        Ok(resources::BUSINESS_CASE)
    } else {
        Err(ErrorData::new(
            rmcp::model::ErrorCode::INVALID_REQUEST,
            format!("Unknown resource: {uri}"),
            None,
        ))
    }
}

pub(crate) fn build_instructions(is_standalone: bool, tools_filtered: bool) -> String {
    let mut text = String::from(
        "CodeScene MCP Server - Code Health analysis tools for AI-assisted development.\n\n\
         TOOLS (always available):\n\
         - explain_code_health: Learn about the Code Health metric.\n\
         - explain_code_health_productivity: Business case for Code Health.\n\
         - code_health_review: Detailed review of a single file.\n\
         - code_health_score: Quick numeric score for a file.\n\
         - pre_commit_code_health_safeguard: Check staged changes before commit.\n\
         - analyze_change_set: Branch-level review before PR.\n\
         - code_health_refactoring_business_case: ROI for refactoring.\n\
         - code_health_auto_refactor: ACE-powered function refactoring.\n\
         - get_config / set_config: Manage server configuration.\n",
    );

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

    if tools_filtered {
        text.push_str(
            "\nNote: Tool availability is restricted by the 'enabled_tools' configuration. \
             Use get_config with key 'enabled_tools' to see the current setting.\n",
        );
    }

    text
}

pub(crate) fn extract_md_title(content: &str) -> &str {
    for line in content.lines() {
        if let Some(title) = line.strip_prefix("# ") {
            return title.trim();
        }
    }
    "Untitled"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_is_2025_11_25() {
        assert_eq!(protocol_version_2025_11_25().as_str(), "2025-11-25");
    }
}
