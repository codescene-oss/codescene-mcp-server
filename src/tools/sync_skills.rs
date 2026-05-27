use std::fs;
use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::docker;
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
    let dest_path = docker::adapt_path_for_docker(Path::new(&params.destination_dir));
    let dest = Path::new(&dest_path);

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

    use crate::tests::{
        assert_error_contains, assert_success_contains, make_server, result_text, set_token,
    };
    use crate::tools::SyncSkillsParam;

    use super::format_summary;

    async fn sync_with_precreated_skill(overwrite: bool) -> String {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("safeguarding-ai-generated-code");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "old").unwrap();

        let params = SyncSkillsParam {
            destination_dir: tmp.path().to_str().unwrap().to_string(),
            overwrite,
        };
        let result = make_server(false)
            .sync_skills(Parameters(params))
            .await
            .unwrap();
        result_text(&result).to_string()
    }

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
        let text = sync_with_precreated_skill(false).await;
        assert!(text.contains("Skipped 1"));
        assert!(text.contains("Downloaded"));
    }

    #[tokio::test]
    async fn overwrites_when_flag_set() {
        let _g = set_token("tok");
        let text = sync_with_precreated_skill(true).await;
        assert!(!text.contains("Skipped"));
        assert!(text.contains("Downloaded"));
    }

    #[tokio::test]
    async fn returns_error_when_directory_creation_fails() {
        let _g = set_token("tok");
        let params = SyncSkillsParam {
            destination_dir: "/dev/null/impossible".to_string(),
            overwrite: false,
        };
        let result = make_server(false)
            .sync_skills(Parameters(params))
            .await
            .unwrap();
        assert_error_contains(&result, "Failed to write some skills");
    }

    #[tokio::test]
    async fn returns_error_when_write_fails() {
        let _g = set_token("tok");
        let tmp = tempfile::tempdir().unwrap();
        // Make one skill's SKILL.md a directory so write fails
        let skill_dir = tmp.path().join("safeguarding-ai-generated-code");
        std::fs::create_dir_all(skill_dir.join("SKILL.md")).unwrap();

        let params = SyncSkillsParam {
            destination_dir: tmp.path().to_str().unwrap().to_string(),
            overwrite: true,
        };
        let result = make_server(false)
            .sync_skills(Parameters(params))
            .await
            .unwrap();
        assert_error_contains(&result, "Failed to write some skills");
    }

    #[test]
    fn format_summary_with_no_skills() {
        let result = format_summary(&[], &[], std::path::Path::new("/tmp"));
        assert_eq!(result, "No skills to sync.");
    }

    #[test]
    fn format_summary_with_only_skipped() {
        let result = format_summary(
            &[],
            &["skill-a".to_string()],
            std::path::Path::new("/tmp"),
        );
        assert!(result.contains("Skipped 1"));
        assert!(!result.contains("Downloaded"));
    }
}
