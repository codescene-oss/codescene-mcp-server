use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::skills;
use crate::CodeSceneServer;

pub(crate) async fn handle(server: &CodeSceneServer) -> Result<CallToolResult, ErrorData> {
    server.version_checker.check_in_background();
    let skill_list = skills::load_skills();
    let entries: Vec<String> = skill_list
        .iter()
        .map(|s| format!("- **{}**: {}", s.name, s.description))
        .collect();
    let text = format!(
        "Available skills ({}):\n\n{}",
        entries.len(),
        entries.join("\n")
    );
    let text = server.maybe_version_warning(&text).await;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

#[cfg(test)]
mod tests {
    use crate::tests::{make_server, result_text, set_token};

    #[tokio::test]
    async fn returns_all_skills() {
        let _g = set_token("tok");
        let result = make_server(false).list_skills().await.unwrap();
        let text = result_text(&result);
        assert!(text.contains("Available skills"));
        assert!(text.contains("safeguarding-ai-generated-code"));
        assert!(text.contains("explaining-code-health"));
    }
}
