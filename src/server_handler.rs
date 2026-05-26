use rmcp::model::{
    AnnotateAble, GetPromptRequestParams, GetPromptResult, Implementation, ListPromptsResult,
    ListResourceTemplatesResult, ListResourcesResult, PaginatedRequestParams, Prompt,
    PromptArgument, PromptMessage, PromptMessageRole, RawResource, RawResourceTemplate,
    ReadResourceRequestParams, ReadResourceResult, ResourceContents, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{tool_handler, ErrorData, RoleServer, ServerHandler};

use crate::{config, prompts, skills, CodeSceneServer};

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
        ))
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

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let skill_list = skills::load_skills();
        let resources = skill_list
            .iter()
            .flat_map(|skill| {
                let main_uri = skills::skill_uri(&skill.name, "SKILL.md");
                let manifest_uri_str = skills::manifest_uri(&skill.name);
                let manifest_name = format!("{} manifest", skill.name);
                let manifest_desc =
                    format!("File manifest for the {} skill", skill.name);
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
        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        let (skill_name, path) =
            skills::parse_skill_uri(&request.uri).ok_or_else(|| {
                ErrorData::resource_not_found(
                    format!("Invalid skill URI: {}", request.uri),
                    None,
                )
            })?;

        let skill_list = skills::load_skills();
        let skill = skill_list
            .iter()
            .find(|s| s.name == skill_name)
            .ok_or_else(|| {
                ErrorData::resource_not_found(
                    format!("Skill not found: {skill_name}"),
                    None,
                )
            })?;

        match path {
            "SKILL.md" => Ok(ReadResourceResult::new(vec![
                ResourceContents::text(skill.content, &request.uri)
                    .with_mime_type("text/markdown"),
            ])),
            "_manifest" => {
                let manifest = skills::build_manifest(skill);
                Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(manifest, &request.uri)
                        .with_mime_type("application/json"),
                ]))
            }
            _ => Err(ErrorData::resource_not_found(
                format!("File not found in skill {skill_name}: {path}"),
                None,
            )),
        }
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        let template = RawResourceTemplate::new(
            "skill://{skill_name}/{path}",
            "Skill file",
        )
        .with_description(
            "Access a specific file within a CodeScene skill. \
             Use skill_name from the resource list and path from the manifest.",
        )
        .with_mime_type("text/markdown");
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![template.no_annotation()],
            next_cursor: None,
            meta: None,
        })
    }
}

fn protocol_version_2025_11_25() -> rmcp::model::ProtocolVersion {
    serde_json::from_str("\"2025-11-25\"").expect("valid MCP protocol version literal")
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
         - get_config / set_config: Manage server configuration.\n\
         \n\
         RESOURCES:\n\
         - skill://<name>/SKILL.md: Agent skill instructions for Code Health workflows.\n\
         - skill://<name>/_manifest: File listing for a skill.\n\
         Use resources/list to discover available skills.\n",
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_is_2025_11_25() {
        assert_eq!(protocol_version_2025_11_25().as_str(), "2025-11-25");
    }
}
