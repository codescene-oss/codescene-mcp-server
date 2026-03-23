use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::configure;
use crate::event_properties;
use crate::tools::SetConfigParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: SetConfigParam,
) -> Result<CallToolResult, ErrorData> {
    server.version_checker.check_in_background();
    let props =
        event_properties::config_properties(event_properties::ConfigAction::Set, &params.key);
    server.track("set-config", props);

    let result = configure::set_value(&params.key, &params.value);
    let text = server.maybe_version_warning(&result).await;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;

    use crate::tests::{make_server, result_text};
    use crate::tools::SetConfigParam;

    #[tokio::test]
    async fn unknown_key_returns_error() {
        let result = make_server(false)
            .set_config(Parameters(SetConfigParam {
                key: "nonexistent_key_xyz".to_string(),
                value: "test".to_string(),
            }))
            .await
            .unwrap();
        let text = result_text(&result);
        assert!(text.contains("error") || text.contains("unknown") || text.contains("Unknown"));
    }
}
