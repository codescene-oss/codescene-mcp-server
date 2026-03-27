use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::configure;
use crate::event_properties;
use crate::tools::GetConfigParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: GetConfigParam,
) -> Result<CallToolResult, ErrorData> {
    server.version_checker.check_in_background();
    let key_str = params.key.as_deref().unwrap_or("");
    let props = event_properties::config_properties(event_properties::ConfigAction::Get, key_str);
    server.track("get-config", props);

    let result = match &params.key {
        Some(k) => configure::get_single(k, &server.config_data, server.is_standalone),
        None => configure::get_all(&server.config_data, server.is_standalone),
    };
    let text = server.maybe_version_warning(&result).await;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;

    use crate::tests::{make_server, result_text};
    use crate::tools::GetConfigParam;

    #[tokio::test]
    async fn lists_all_options() {
        let result = make_server(false)
            .get_config(Parameters(GetConfigParam { key: None }))
            .await
            .unwrap();
        assert!(result_text(&result).contains("config_dir"));
    }

    #[tokio::test]
    async fn reads_single_key() {
        let result = make_server(false)
            .get_config(Parameters(GetConfigParam {
                key: Some("access_token".to_string()),
            }))
            .await
            .unwrap();
        assert!(result_text(&result).contains("access_token"));
    }
}
