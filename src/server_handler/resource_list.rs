use rmcp::model::{
    ListResourceTemplatesResult, ListResourcesResult, ReadResourceResult, Resource,
    ResourceContents, ResourceTemplate,
};
use rmcp::ErrorData;

use crate::skills;

pub(crate) const MCP_USAGE_APP_URI: &str = "ui://codescene/mcp-usage-overview";
const MCP_APPS_MIME_TYPE: &str = "text/html;profile=mcp-app";

pub(super) fn build_resources_list() -> ListResourcesResult {
    let skill_list = skills::load_skills();
    let mut resources: Vec<_> = skill_list
        .iter()
        .flat_map(|skill| {
            let main_uri = skills::skill_uri(&skill.name, "SKILL.md");
            let manifest_uri_str = skills::manifest_uri(&skill.name);
            let manifest_name = format!("{} manifest", skill.name);
            let manifest_desc = format!("File manifest for the {} skill", skill.name);
            vec![
                Resource::new(main_uri, &skill.name)
                    .with_description(&skill.description)
                    .with_mime_type("text/markdown")
                    .with_size(skill.content.len() as u64),
                Resource::new(manifest_uri_str, manifest_name)
                    .with_description(manifest_desc)
                    .with_mime_type("application/json"),
            ]
        })
        .collect();
    resources.push(
        Resource::new(MCP_USAGE_APP_URI, "MCP usage overview")
            .with_description("Interactive overview of CodeHealth MCP safeguard usage")
            .with_mime_type(MCP_APPS_MIME_TYPE)
            .with_size(crate::resources::mcp_usage_app().len() as u64),
    );
    ListResourcesResult {
        resources,
        next_cursor: None,
        meta: None,
    }
}

pub(super) fn resolve_resource(uri: &str) -> Result<ReadResourceResult, ErrorData> {
    if uri == MCP_USAGE_APP_URI {
        let ui_meta = rmcp::model::Meta(
            serde_json::json!({
                "ui": { "prefersBorder": true }
            })
            .as_object()
            .expect("MCP Apps resource metadata must be an object")
            .clone(),
        );
        return Ok(ReadResourceResult::new(vec![ResourceContents::text(
            crate::resources::mcp_usage_app(),
            uri,
        )
        .with_mime_type(MCP_APPS_MIME_TYPE)
        .with_meta(ui_meta)]));
    }

    let (skill_name, path) = skills::parse_skill_uri(uri)
        .ok_or_else(|| ErrorData::resource_not_found(format!("Invalid skill URI: {uri}"), None))?;

    let skill_list = skills::load_skills();
    let skill = skill_list
        .iter()
        .find(|s| s.name == skill_name)
        .ok_or_else(|| {
            ErrorData::resource_not_found(format!("Skill not found: {skill_name}"), None)
        })?;

    match path {
        "SKILL.md" => Ok(ReadResourceResult::new(vec![ResourceContents::text(
            skill.content,
            uri,
        )
        .with_mime_type("text/markdown")])),
        "_manifest" => {
            let manifest = skills::build_manifest(skill);
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                manifest, uri,
            )
            .with_mime_type("application/json")]))
        }
        _ => Err(ErrorData::resource_not_found(
            format!("File not found in skill {skill_name}: {path}"),
            None,
        )),
    }
}

pub(super) fn build_resource_templates() -> ListResourceTemplatesResult {
    let template = ResourceTemplate::new("skill://{skill_name}/{path}", "Skill file")
        .with_description(
            "Access a specific file within a CodeScene skill. \
         Use skill_name from the resource list and path from the manifest.",
        )
        .with_mime_type("text/markdown");
    ListResourceTemplatesResult {
        resource_templates: vec![template],
        next_cursor: None,
        meta: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resources_list_contains_all_skills() {
        let result = build_resources_list();
        let skills = skills::load_skills();
        assert_eq!(result.resources.len(), skills.len() * 2 + 1);
    }

    #[test]
    fn resources_have_correct_mime_types() {
        let result = build_resources_list();
        let md_resources: Vec<_> = result
            .resources
            .iter()
            .filter(|r| r.uri.ends_with("/SKILL.md"))
            .collect();
        assert!(!md_resources.is_empty());
        for r in &md_resources {
            assert_eq!(r.mime_type.as_deref(), Some("text/markdown"));
        }
        let manifest_resources: Vec<_> = result
            .resources
            .iter()
            .filter(|r| r.uri.ends_with("/_manifest"))
            .collect();
        for r in &manifest_resources {
            assert_eq!(r.mime_type.as_deref(), Some("application/json"));
        }
    }

    #[test]
    fn read_skill_md_resource() {
        let uri = "skill://safeguarding-ai-generated-code/SKILL.md";
        let result = resolve_resource(uri).unwrap();
        assert_eq!(result.contents.len(), 1);
    }

    #[test]
    fn read_manifest_resource() {
        let uri = "skill://safeguarding-ai-generated-code/_manifest";
        let result = resolve_resource(uri).unwrap();
        assert_eq!(result.contents.len(), 1);
    }

    #[test]
    fn read_unknown_skill_returns_error() {
        let result = resolve_resource("skill://nonexistent/SKILL.md");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Skill not found"));
    }

    #[test]
    fn read_invalid_uri_returns_error() {
        let result = resolve_resource("file:///etc/passwd");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Invalid skill URI"));
    }

    #[test]
    fn read_unknown_path_in_skill_returns_error() {
        let result = resolve_resource("skill://safeguarding-ai-generated-code/unknown.txt");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("File not found"));
    }

    #[test]
    fn resource_templates_contains_skill_template() {
        let result = build_resource_templates();
        assert_eq!(result.resource_templates.len(), 1);
    }

    #[test]
    fn read_mcp_usage_app_resource() {
        let result = resolve_resource(MCP_USAGE_APP_URI).unwrap();
        assert_eq!(result.contents.len(), 1);
        let serialized = serde_json::to_value(result).unwrap();
        assert_eq!(serialized["contents"][0]["mimeType"], MCP_APPS_MIME_TYPE);
        assert!(serialized["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("<!doctype html>"));
    }
}
