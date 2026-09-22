use std::fs;
use std::path::{Path, PathBuf};

const INSTRUCTION_FILES: &[&str] = &[
    "AGENTS.md",
    "agents.md",
    "CLAUDE.md",
    "claude.md",
    "GEMINI.md",
    "gemini.md",
    ".github/copilot-instructions.md",
    ".cursorrules",
];
const INSTRUCTION_DIRECTORIES: &[&str] = &[".cursor/rules", ".amazonq/rules"];
const CODESCENE_MCP_MARKERS: &[&str] =
    &["codescene mcp", "codescene-mcp", "codehealth-mcp", "cs-mcp"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AgentInstructions {
    pub(crate) file_present: bool,
    pub(crate) codescene_mcp_instructions_present: bool,
}

pub(crate) fn detect(action_path: Option<&Path>) -> AgentInstructions {
    let Some(repository_root) = repository_root(action_path) else {
        return AgentInstructions::default();
    };
    detect_in_repository(&repository_root)
}

fn repository_root(action_path: Option<&Path>) -> Option<PathBuf> {
    let path = match action_path {
        Some(path) => PathBuf::from(crate::docker::adapt_path_for_docker(path)),
        None => {
            crate::docker::container_workspace_dir().or_else(|| std::env::current_dir().ok())?
        }
    };
    crate::cli::find_git_root(&path)
}

fn detect_in_repository(repository_root: &Path) -> AgentInstructions {
    let files = instruction_files(repository_root);
    AgentInstructions {
        file_present: !files.is_empty(),
        codescene_mcp_instructions_present: files.iter().any(|path| contains_codescene_mcp(path)),
    }
}

fn instruction_files(repository_root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = INSTRUCTION_FILES
        .iter()
        .map(|relative| repository_root.join(relative))
        .filter(|path| path.is_file())
        .collect();
    for relative in INSTRUCTION_DIRECTORIES {
        collect_regular_files(&repository_root.join(relative), &mut files);
    }
    files
}

fn collect_regular_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = directory.read_dir() else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() {
            files.push(path);
        } else if file_type.is_dir() {
            collect_regular_files(&path, files);
        }
    }
}

fn contains_codescene_mcp(path: &Path) -> bool {
    fs::read_to_string(path)
        .map(|content| {
            let content = content.to_ascii_lowercase();
            CODESCENE_MCP_MARKERS
                .iter()
                .any(|marker| content.contains(marker))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join(".git")).unwrap();
        directory
    }

    #[test]
    fn reports_no_instructions_when_supported_files_are_absent() {
        let repository = repository();

        assert_eq!(
            detect(Some(repository.path())),
            AgentInstructions::default()
        );
    }

    #[test]
    fn detects_generic_agent_instructions_without_codescene_guidance() {
        let repository = repository();
        fs::write(repository.path().join("AGENTS.md"), "Run the test suite.").unwrap();

        assert_eq!(
            detect(Some(repository.path())),
            AgentInstructions {
                file_present: true,
                codescene_mcp_instructions_present: false,
            }
        );
    }

    #[test]
    fn detects_codescene_guidance_case_insensitively_from_nested_path() {
        let repository = repository();
        let nested = repository.path().join("src/module");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            repository.path().join("CLAUDE.md"),
            "Always use the CodeScene MCP tools.",
        )
        .unwrap();

        assert_eq!(
            detect(Some(&nested)),
            AgentInstructions {
                file_present: true,
                codescene_mcp_instructions_present: true,
            }
        );
    }

    #[test]
    fn detects_package_name_in_lowercase_instruction_file() {
        let repository = repository();
        fs::write(
            repository.path().join("agents.md"),
            "Run @codescene/codehealth-mcp safeguards.",
        )
        .unwrap();

        let result = detect(Some(repository.path()));

        assert!(result.file_present);
        assert!(result.codescene_mcp_instructions_present);
    }

    #[test]
    fn detects_codescene_guidance_in_shipped_agents_templates() {
        for template in [
            include_str!("../docs/AGENTS-full.md"),
            include_str!("../docs/AGENTS-standalone.md"),
        ] {
            let repository = repository();
            fs::write(repository.path().join("AGENTS.md"), template).unwrap();

            assert_eq!(
                detect(Some(repository.path())),
                AgentInstructions {
                    file_present: true,
                    codescene_mcp_instructions_present: true,
                }
            );
        }
    }

    #[test]
    fn detects_instruction_files_in_rule_directories() {
        let repository = repository();
        let rules = repository.path().join(".cursor/rules/code-health");
        fs::create_dir_all(&rules).unwrap();
        fs::write(
            rules.join("safeguards.mdc"),
            "Use CodeScene MCP before commit.",
        )
        .unwrap();

        let result = detect(Some(repository.path()));

        assert!(result.file_present);
        assert!(result.codescene_mcp_instructions_present);
    }

    #[test]
    fn ignores_supported_names_outside_a_git_repository() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("AGENTS.md"), "CodeScene MCP").unwrap();

        assert_eq!(detect(Some(directory.path())), AgentInstructions::default());
    }
}
