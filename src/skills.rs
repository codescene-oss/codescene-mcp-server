use gray_matter::engine::YAML;
use gray_matter::Matter;
use sha2::{Digest, Sha256};

const SKILL_URI_SCHEME: &str = "skill://";

struct EmbeddedSkill {
    dir_name: &'static str,
    content: &'static str,
}

const EMBEDDED_SKILLS: &[EmbeddedSkill] = &[
    EmbeddedSkill {
        dir_name: "configuring-codescene-mcp",
        content: include_str!("../skills/configuring-codescene-mcp/SKILL.md"),
    },
    EmbeddedSkill {
        dir_name: "explaining-code-health",
        content: include_str!("../skills/explaining-code-health/SKILL.md"),
    },
    EmbeddedSkill {
        dir_name: "guiding-refactoring-with-code-health",
        content: include_str!("../skills/guiding-refactoring-with-code-health/SKILL.md"),
    },
    EmbeddedSkill {
        dir_name: "installing-and-activating-codescene-mcp",
        content: include_str!("../skills/installing-and-activating-codescene-mcp/SKILL.md"),
    },
    EmbeddedSkill {
        dir_name: "making-the-business-case-for-code-health",
        content: include_str!("../skills/making-the-business-case-for-code-health/SKILL.md"),
    },
    EmbeddedSkill {
        dir_name: "prioritizing-technical-debt",
        content: include_str!("../skills/prioritizing-technical-debt/SKILL.md"),
    },
    EmbeddedSkill {
        dir_name: "risk-based-testing-with-code-health",
        content: include_str!("../skills/risk-based-testing-with-code-health/SKILL.md"),
    },
    EmbeddedSkill {
        dir_name: "routing-work-with-code-ownership",
        content: include_str!("../skills/routing-work-with-code-ownership/SKILL.md"),
    },
    EmbeddedSkill {
        dir_name: "safeguarding-ai-generated-code",
        content: include_str!("../skills/safeguarding-ai-generated-code/SKILL.md"),
    },
];

/// A parsed skill with metadata extracted from YAML frontmatter.
pub(crate) struct Skill {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) content: &'static str,
    pub(crate) content_hash: String,
}

/// Extract the `description` field from YAML frontmatter.
///
/// Falls back to `"No description"` if frontmatter is absent or the
/// `description` key is missing.
fn extract_description(content: &str) -> String {
    let matter = Matter::<YAML>::new();
    match matter.parse(content) {
        Ok(parsed) => parsed
            .data
            .and_then(|d: gray_matter::Pod| d["description"].as_string().ok())
            .unwrap_or_else(|| "No description".to_string()),
        Err(_) => "No description".to_string(),
    }
}

fn sha256_hex(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex::encode(hash)
}

/// Encode bytes as lowercase hexadecimal.
mod hex {
    pub(super) fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .fold(String::new(), |mut acc, b| {
                use std::fmt::Write;
                let _ = write!(acc, "{b:02x}");
                acc
            })
    }
}

/// Build the skill registry from embedded content.
pub(crate) fn load_skills() -> Vec<Skill> {
    EMBEDDED_SKILLS
        .iter()
        .map(|embedded| {
            let description = extract_description(embedded.content);
            let content_hash = sha256_hex(embedded.content.as_bytes());
            Skill {
                name: embedded.dir_name.to_string(),
                description,
                content: embedded.content,
                content_hash,
            }
        })
        .collect()
}

/// Build a `skill://` URI for a skill file.
pub(crate) fn skill_uri(skill_name: &str, path: &str) -> String {
    format!("{SKILL_URI_SCHEME}{skill_name}/{path}")
}

/// Build a manifest URI for a skill.
pub(crate) fn manifest_uri(skill_name: &str) -> String {
    skill_uri(skill_name, "_manifest")
}

/// Build the JSON manifest for a single skill.
pub(crate) fn build_manifest(skill: &Skill) -> String {
    let size = skill.content.len();
    serde_json::json!({
        "skill": skill.name,
        "files": [
            {
                "path": "SKILL.md",
                "size": size,
                "hash": format!("sha256:{}", skill.content_hash),
            }
        ]
    })
    .to_string()
}

/// Parse a `skill://` URI into `(skill_name, file_path)`.
///
/// Returns `None` if the URI does not use the `skill://` scheme.
pub(crate) fn parse_skill_uri(uri: &str) -> Option<(&str, &str)> {
    let rest = uri.strip_prefix(SKILL_URI_SCHEME)?;
    let (name, path) = rest.split_once('/')?;
    if name.is_empty() || path.is_empty() {
        return None;
    }
    Some((name, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_description_from_frontmatter() {
        let content = "---\nname: test-skill\ndescription: A test skill.\n---\n\n# Body";
        let desc = extract_description(content);
        assert_eq!(desc, "A test skill.");
    }

    #[test]
    fn extract_description_without_frontmatter() {
        let content = "# Title\n\nSome text.";
        let desc = extract_description(content);
        assert_eq!(desc, "No description");
    }

    #[test]
    fn parse_skill_uri_valid() {
        let (name, path) = parse_skill_uri("skill://my-skill/SKILL.md").unwrap();
        assert_eq!(name, "my-skill");
        assert_eq!(path, "SKILL.md");
    }

    #[test]
    fn parse_skill_uri_manifest() {
        let (name, path) = parse_skill_uri("skill://my-skill/_manifest").unwrap();
        assert_eq!(name, "my-skill");
        assert_eq!(path, "_manifest");
    }

    #[test]
    fn parse_skill_uri_rejects_invalid() {
        assert!(parse_skill_uri("file:///foo").is_none());
        assert!(parse_skill_uri("skill://").is_none());
        assert!(parse_skill_uri("skill:///foo").is_none());
    }

    #[test]
    fn load_skills_returns_all_embedded() {
        let skills = load_skills();
        assert_eq!(skills.len(), EMBEDDED_SKILLS.len());
        for skill in &skills {
            assert!(!skill.name.is_empty());
            assert!(!skill.description.is_empty());
            assert!(!skill.content.is_empty());
            assert!(!skill.content_hash.is_empty());
        }
    }

    #[test]
    fn manifest_contains_file_info() {
        let skills = load_skills();
        let manifest = build_manifest(&skills[0]);
        let parsed: serde_json::Value = serde_json::from_str(&manifest).unwrap();
        assert_eq!(parsed["skill"], skills[0].name);
        let files = parsed["files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0]["path"], "SKILL.md");
        assert!(files[0]["size"].as_u64().unwrap() > 0);
        assert!(files[0]["hash"].as_str().unwrap().starts_with("sha256:"));
    }

    #[test]
    fn skill_uri_builds_correctly() {
        assert_eq!(skill_uri("foo", "SKILL.md"), "skill://foo/SKILL.md");
        assert_eq!(manifest_uri("foo"), "skill://foo/_manifest");
    }

    #[test]
    fn hex_encode_works() {
        assert_eq!(hex::encode([0xde, 0xad, 0xbe, 0xef]), "deadbeef");
    }
}
