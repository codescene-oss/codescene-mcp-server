use std::fs;
use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::skills;
use crate::tools::common::tool_error;
use crate::tools::SyncSkillsParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: SyncSkillsParam,
) -> Result<CallToolResult, ErrorData> {
    server.version_checker.check_in_background();
    let skill_list = skills::load_skills();
    let dest = Path::new(&params.destination_dir);

    let mut downloaded = Vec::new();
    let mut skipped = Vec::new();
    let mut errors = Vec::new();

    for skill in &skill_list {
        let skill_dir = dest.join(&skill.name);
        let skill_file = skill_dir.join("SKILL.md");

        if skill_file.exists() && !params.overwrite {
            skipped.push(skill.name.clone());
            continue;
        }

        if let Err(e) = fs::create_dir_all(&skill_dir) {
            errors.push(format!("{}: {e}", skill.name));
            continue;
        }

        match fs::write(&skill_file, skill.content) {
            Ok(()) => downloaded.push(skill.name.clone()),
            Err(e) => errors.push(format!("{}: {e}", skill.name)),
        }
    }

    if !errors.is_empty() {
        return Ok(tool_error(&format!(
            "Failed to write some skills:\n{}",
            errors.join("\n")
        )));
    }

    let msg = format_summary(&downloaded, &skipped, dest);
    let text = server.maybe_version_warning(&msg).await;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

fn format_summary(downloaded: &[String], skipped: &[String], dest: &Path) -> String {
    let mut parts = Vec::new();
    if !downloaded.is_empty() {
        parts.push(format!(
            "Downloaded {} skill(s) to {}:\n{}",
            downloaded.len(),
            dest.display(),
            downloaded.iter().map(|s| format!("  - {s}")).collect::<Vec<_>>().join("\n")
        ));
    }
    if !skipped.is_empty() {
        parts.push(format!(
            "Skipped {} existing skill(s) (use overwrite=true to replace):\n{}",
            skipped.len(),
            skipped.iter().map(|s| format!("  - {s}")).collect::<Vec<_>>().join("\n")
        ));
    }
    if parts.is_empty() {
        return "No skills to sync.".to_string();
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;

    use crate::tests::{assert_success_contains, make_server, result_text, set_token};
    use crate::tools::SyncSkillsParam;

    #[tokio::test]
    async fn syncs_all_skills() {
        let _g = set_token("tok");
        let tmp = tempfile::tempdir().unwrap();
        let params = SyncSkillsParam {
            destination_dir: tmp.path().to_str().unwrap().to_string(),
            overwrite: false,
        };
        let result = make_server(false)
            .sync_skills(Parameters(params))
            .await
            .unwrap();
        assert_success_contains(&result, "Downloaded");

        let entries: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(entries.len() >= 9);
    }

    #[tokio::test]
    async fn skips_existing_without_overwrite() {
        let _g = set_token("tok");
        let tmp = tempfile::tempdir().unwrap();

        // Pre-create one skill
        let skill_dir = tmp.path().join("safeguarding-ai-generated-code");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "old").unwrap();

        let params = SyncSkillsParam {
            destination_dir: tmp.path().to_str().unwrap().to_string(),
            overwrite: false,
        };
        let result = make_server(false)
            .sync_skills(Parameters(params))
            .await
            .unwrap();
        let text = result_text(&result);
        assert!(text.contains("Skipped 1"));
        assert!(text.contains("Downloaded"));
    }

    #[tokio::test]
    async fn overwrites_when_flag_set() {
        let _g = set_token("tok");
        let tmp = tempfile::tempdir().unwrap();

        let skill_dir = tmp.path().join("safeguarding-ai-generated-code");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "old").unwrap();

        let params = SyncSkillsParam {
            destination_dir: tmp.path().to_str().unwrap().to_string(),
            overwrite: true,
        };
        let result = make_server(false)
            .sync_skills(Parameters(params))
            .await
            .unwrap();
        let text = result_text(&result);
        assert!(!text.contains("Skipped"));
        assert!(text.contains("Downloaded"));
    }
}
