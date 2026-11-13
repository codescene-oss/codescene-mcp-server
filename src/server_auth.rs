use rmcp::model::{CallToolResult, ContentBlock as Content};

use crate::auth::AuthCredential;
use crate::{analytics_attribution, environment, tools, CodeSceneServer, ContextualErrorEvent};

const TOKEN_MISSING_MSG: &str = "\
No access token configured.\n\n\
Call the `login` tool now to sign in with OAuth. Do not ask the user for a token first.\n\n\
Use a Personal Access Token only when OAuth is not suitable, such as CI/CD or other headless environments.\n\
If a token must be configured manually:\n\
1. Use the `set_config` tool: set_config(key=\"access_token\", value=\"your-token\")\n\
2. Set the CS_ACCESS_TOKEN environment variable in your MCP client configuration\n\n\
To get a Personal Access Token, see:\n\
https://github.com/codescene-oss/codehealth-mcp/blob/main/docs/authentication.md";

const TOKEN_MISSING_MSG_DOCKER: &str = "\
No access token configured.\n\n\
OAuth login is not available in Docker. Configure a Personal Access Token:\n\
1. Set the CS_ACCESS_TOKEN environment variable in your MCP client / Docker configuration\n\
2. Or use the `set_config` tool: set_config(key=\"access_token\", value=\"your-token\")\n\n\
To get a Personal Access Token, see:\n\
https://github.com/codescene-oss/codehealth-mcp/blob/main/docs/authentication.md";

pub(crate) fn token_missing_msg() -> &'static str {
    if environment::is_docker() {
        TOKEN_MISSING_MSG_DOCKER
    } else {
        TOKEN_MISSING_MSG
    }
}

impl CodeSceneServer {
    pub(crate) async fn require_token(&self) -> Option<CallToolResult> {
        self.resolve_auth_credential().await.err()
    }

    pub(crate) async fn require_token_with_context(
        &self,
        tool: &str,
        context: analytics_attribution::AnalyticsContext,
    ) -> Option<CallToolResult> {
        let result = self
            .resolve_auth_credential_for_context(&context)
            .await
            .err()?;
        self.track_error_with_context(ContextualErrorEvent {
            error_kind: "authentication_unavailable",
            tool,
            detail: None,
            context,
        });
        Some(result)
    }

    pub(crate) async fn resolve_auth_credential(&self) -> Result<AuthCredential, CallToolResult> {
        self.resolve_auth_credential_for_context(
            &analytics_attribution::AnalyticsContext::CurrentWorkspace,
        )
        .await
    }

    async fn resolve_auth_credential_for_context(
        &self,
        context: &analytics_attribution::AnalyticsContext,
    ) -> Result<AuthCredential, CallToolResult> {
        match self
            .auth_manager
            .resolve_credential(&*self.cli_runner)
            .await
        {
            Ok(Some(credential)) => {
                self.log_credential_resolved(&credential, "resolved auth credential for API tool");
                self.route_oauth_account(credential, context)
                    .await
                    .map_err(|error| {
                        tracing::warn!(%error, "automatic OAuth account switch failed");
                        tools::common::tool_error(format!(
                            "Automatic CodeScene account switching failed: {error}\n\
                             Use login or switch_account to retry."
                        ))
                    })
            }
            Ok(None) => {
                self.log_no_credential("failed to resolve auth credential for API tool");
                Err(CallToolResult::success(vec![Content::text(
                    token_missing_msg(),
                )]))
            }
            Err(e) => {
                tracing::warn!(error = %e, "auth token check failed");
                Err(CallToolResult::success(vec![Content::text(
                    token_missing_msg(),
                )]))
            }
        }
    }

    pub(crate) async fn require_project_api(
        &self,
        tool: &str,
        project_id: i64,
    ) -> Result<AuthCredential, CallToolResult> {
        let context = analytics_attribution::AnalyticsContext::ExplicitProjectIds(vec![project_id]);
        let credential = self
            .resolve_auth_credential_for_context(&context)
            .await
            .inspect_err(|_| {
                self.track_error_with_context(ContextualErrorEvent::for_project(
                    "authentication_unavailable",
                    tool,
                    project_id,
                ));
            })?;
        if self.is_standalone {
            self.track_error_with_context(ContextualErrorEvent::for_project(
                "standalone_license",
                tool,
                project_id,
            ));
            return Err(tools::common::tool_error(
                "This tool requires a CodeScene API token (not a standalone license).",
            ));
        }
        Ok(credential)
    }

    fn log_credential_resolved(&self, credential: &AuthCredential, message: &'static str) {
        let source = credential_source(credential);
        let has_web_root = credential.web_root().is_some();
        tracing::info!(source, has_web_root, "{}", message);
    }

    fn log_no_credential(&self, message: &'static str) {
        let has_configured_token = crate::auth::configured_credential().is_some();
        let has_oauth_token = self.auth_manager.try_cached_access_token().is_some();
        let oauth_expires_at = crate::config::try_read_env("CS_OAUTH_EXPIRES_AT");
        let signed_out_sentinel = oauth_expires_at.as_deref() == Some("0");
        let oauth_client = crate::config::try_read_env("CS_OAUTH_CLIENT");
        tracing::warn!(
            has_configured_token,
            has_oauth_token,
            oauth_expires_at,
            signed_out_sentinel,
            oauth_client,
            "{}",
            message
        );
    }
}

pub(crate) fn credential_source(credential: &AuthCredential) -> &'static str {
    match credential {
        AuthCredential::Configured { .. } => "configured",
        AuthCredential::OAuth { .. } => "oauth",
    }
}
