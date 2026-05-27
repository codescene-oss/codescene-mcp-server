use std::fs;
use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::docker;
use crate::skills;
use crate::tools::common::tool_error;
use crate::tools::DownloadSkillParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: DownloadSkillParam,
) -> Result<CallToolResult, ErrorData> {
    server.version_checker.check_in_background();
    let skill_list = skills::load_skills();
    let skill = skill_list.iter().find(|s| s.name == params.skill_name);
    let skill = match skill {
        Some(s) => s,
        None => {
            return Ok(tool_error(&format!(
                "Unknown skill: '{}'. Use list_skills to see available skills.",
                params.skill_name
            )));
        }
    };

    let dest_path = docker::adapt_path_for_docker(Path::new(&params.destination_dir));
    let skill_dir = Path::new(&dest_path).join(&skill.name);
    let skill_file = skill_dir.join("SKILL.md");

    if skill_file.exists() && !params.overwrite {
        return Ok(tool_error(&format!(
            "Skill '{}' already exists at {}. Set overwrite=true to replace it.",
            skill.name,
            skill_file.display()
        )));
    }

    if let Err(e) = fs::create_dir_all(&skill_dir) {
        return Ok(tool_error(&format!(
            "Failed to create directory {}: {e}",
            skill_dir.display()
        )));
    }

    if let Err(e) = fs::write(&skill_file, skill.content) {
        return Ok(tool_error(&format!(
            "Failed to write {}: {e}",
            skill_file.display()
        )));
    }

    let msg = format!(
        "Downloaded skill '{}' to {}",
        skill.name,
        skill_file.display()
    );
    let text = server.maybe_version_warning(&msg).await;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;

    use crate::tests::{
        assert_error_contains, assert_success_contains, make_server, set_token,
    };
    use crate::tools::DownloadSkillParam;

    #[tokio::test]
    async fn downloads_skill_to_directory() {
        let _g = set_token("tok");
        let tmp = tempfile::tempdir().unwrap();
        let params = DownloadSkillParam {
            skill_name: "safeguarding-ai-generated-code".to_string(),
            destination_dir: tmp.path().to_str().unwrap().to_string(),
            overwrite: false,
        };
        let result = make_server(false)
            .download_skill(Parameters(params))
            .await
            .unwrap();
        assert_success_contains(&result, "Downloaded skill");

        let file = tmp
            .path()
            .join("safeguarding-ai-generated-code")
            .join("SKILL.md");
        assert!(file.exists());
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.contains("---"));
    }

    #[tokio::test]
    async fn refuses_overwrite_by_default() {
        let _g = set_token("tok");
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("safeguarding-ai-generated-code");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "old").unwrap();

        let params = DownloadSkillParam {
            skill_name: "safeguarding-ai-generated-code".to_string(),
            destination_dir: tmp.path().to_str().unwrap().to_string(),
            overwrite: false,
        };
        let result = make_server(false)
            .download_skill(Parameters(params))
            .await
            .unwrap();
        assert_error_contains(&result, "already exists");
    }

    #[tokio::test]
    async fn overwrites_when_flag_set() {
        let _g = set_token("tok");
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("safeguarding-ai-generated-code");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "old").unwrap();

        let params = DownloadSkillParam {
            skill_name: "safeguarding-ai-generated-code".to_string(),
            destination_dir: tmp.path().to_str().unwrap().to_string(),
            overwrite: true,
        };
        let result = make_server(false)
            .download_skill(Parameters(params))
            .await
            .unwrap();
        assert_success_contains(&result, "Downloaded skill");
    }

    #[tokio::test]
    async fn returns_error_for_unknown_skill() {
        let _g = set_token("tok");
        let tmp = tempfile::tempdir().unwrap();
        let params = DownloadSkillParam {
            skill_name: "nonexistent".to_string(),
            destination_dir: tmp.path().to_str().unwrap().to_string(),
            overwrite: false,
        };
        let result = make_server(false)
            .download_skill(Parameters(params))
            .await
            .unwrap();
        assert_error_contains(&result, "Unknown skill");
    }

    #[tokio::test]
    async fn returns_error_when_directory_creation_fails() {
        let _g = set_token("tok");
        // Use /dev/null as parent — cannot create subdirectories under a device file
        let params = DownloadSkillParam {
            skill_name: "safeguarding-ai-generated-code".to_string(),
            destination_dir: "/dev/null/impossible".to_string(),
            overwrite: false,
        };
        let result = make_server(false)
            .download_skill(Parameters(params))
            .await
            .unwrap();
        assert_error_contains(&result, "Failed to create directory");
    }

    #[tokio::test]
    async fn returns_error_when_write_fails() {
        let _g = set_token("tok");
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("safeguarding-ai-generated-code");
        // Create SKILL.md as a directory so fs::write fails
        std::fs::create_dir_all(skill_dir.join("SKILL.md")).unwrap();

        let params = DownloadSkillParam {
            skill_name: "safeguarding-ai-generated-code".to_string(),
            destination_dir: tmp.path().to_str().unwrap().to_string(),
            overwrite: true,
        };
        let result = make_server(false)
            .download_skill(Parameters(params))
            .await
            .unwrap();
        assert_error_contains(&result, "Failed to write");
    }
}
