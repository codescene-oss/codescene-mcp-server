use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::skills;
use crate::tools::common::tool_error;
use crate::tools::SkillNameParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: SkillNameParam,
) -> Result<CallToolResult, ErrorData> {
    server.version_checker.check_in_background();
    let skill_list = skills::load_skills();
    let skill = skill_list.iter().find(|s| s.name == params.skill_name);
    match skill {
        Some(s) => {
            let manifest = skills::build_manifest(s);
            let text = server.maybe_version_warning(&manifest).await;
            Ok(CallToolResult::success(vec![Content::text(text)]))
        }
        None => Ok(tool_error(&format!(
            "Unknown skill: '{}'. Use list_skills to see available skills.",
            params.skill_name
        ))),
    }
}

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;

    use crate::tests::{
        assert_error_contains, assert_success_contains, make_server, set_token,
    };
    use crate::tools::SkillNameParam;

    #[tokio::test]
    async fn returns_manifest_for_valid_skill() {
        let _g = set_token("tok");
        let params = SkillNameParam {
            skill_name: "safeguarding-ai-generated-code".to_string(),
        };
        let result = make_server(false)
            .get_skill_manifest(Parameters(params))
            .await
            .unwrap();
        assert_success_contains(&result, "safeguarding-ai-generated-code");
        assert_success_contains(&result, "SKILL.md");
        assert_success_contains(&result, "sha256:");
    }

    #[tokio::test]
    async fn returns_error_for_unknown_skill() {
        let _g = set_token("tok");
        let params = SkillNameParam {
            skill_name: "nonexistent".to_string(),
        };
        let result = make_server(false)
            .get_skill_manifest(Parameters(params))
            .await
            .unwrap();
        assert_error_contains(&result, "Unknown skill");
    }
}
