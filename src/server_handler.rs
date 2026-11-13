use rmcp::handler::server::tool::ToolCallContext;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult, Implementation,
    ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult, PaginatedRequestParams,
    ReadResourceRequestParams, ReadResourceResult, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{tool_handler, ErrorData, RoleServer, ServerHandler};

use crate::{config, environment, setup_hint, tracking, CodeSceneServer};

mod instructions;
mod prompt_list;
mod resource_list;

pub(crate) use instructions::build_instructions;
use prompt_list::{build_prompts_list, resolve_prompt};
pub(crate) use resource_list::MCP_USAGE_APP_URI;
use resource_list::{build_resource_templates, build_resources_list, resolve_resource};

#[tool_handler(router = "self.tool_router")]
impl ServerHandler for CodeSceneServer {
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let client_info = context.peer.peer_info().map(|peer_info| {
            tracking::McpClientInfo::new(
                peer_info.client_info.name.clone(),
                peer_info.client_info.version.clone(),
            )
        });
        tracking::with_mcp_client_info(client_info, async {
            let tool_name = request.name.to_string();
            let action_path = setup_hint::action_path(request.arguments.as_ref());
            let tool_context = ToolCallContext::new(self, request, context);
            let mut result = self.tool_router.call(tool_context).await?;
            setup_hint::add_and_track(self, &mut result, &tool_name, action_path);
            Ok(result)
        })
        .await
    }

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
            "codehealth-mcp-server",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_is_2025_11_25() {
        assert_eq!(protocol_version_2025_11_25().as_str(), "2025-11-25");
    }
}
