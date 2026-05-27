use std::fmt;
use std::path::Path;

use crate::cli;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A validation check that can be applied before invoking the CLI.
///
/// Each variant represents an independent, composable precondition.
/// Add new variants here to extend the validation pipeline without
/// touching existing tool handlers.
pub(crate) enum Check<'a> {
    /// The file at the given path must exist on disk.
    FileExists(&'a Path),

    /// The file extension must be in the set of languages supported
    /// by the CodeScene CLI for Code Health analysis.
    SupportedFileType(&'a Path),

    /// The path must reside inside a git repository (i.e. an ancestor
    /// directory contains a `.git` folder).
    InsideGitRepo(&'a Path),
}

/// A failed validation check, carrying both a user-facing message and a
/// safe telemetry label.
pub(crate) struct ValidationError {
    /// User-friendly error description (safe to show in tool output).
    pub message: String,
    /// Fixed telemetry label (never contains sensitive data).
    pub kind: &'static str,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

// ---------------------------------------------------------------------------
// Validator trait
// ---------------------------------------------------------------------------

/// Abstraction over input validation so it can be mocked in unit tests.
///
/// The production implementation performs real filesystem and git checks.
/// Tests inject a [`MockValidator`] that always succeeds (or can be
/// configured to fail for specific checks).
pub(crate) trait Validator: Send + Sync {
    fn run_checks(&self, checks: &[Check<'_>]) -> Result<(), ValidationError>;
}

/// Production validator that performs real filesystem and git checks.
pub(crate) struct ProductionValidator;

impl Validator for ProductionValidator {
    fn run_checks(&self, checks: &[Check<'_>]) -> Result<(), ValidationError> {
        for check in checks {
            match check {
                Check::FileExists(path) => validate_file_exists(path)?,
                Check::SupportedFileType(path) => validate_supported_file_type(path)?,
                Check::InsideGitRepo(path) => validate_inside_git_repo(path)?,
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Individual validators
// ---------------------------------------------------------------------------

fn validate_file_exists(path: &Path) -> Result<(), ValidationError> {
    if !path.exists() {
        return Err(ValidationError {
            message: format!(
                "The file '{}' does not exist. Please check the path and try again.",
                path.display()
            ),
            kind: "file_not_found",
        });
    }
    Ok(())
}

fn validate_supported_file_type(path: &Path) -> Result<(), ValidationError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext {
        Some(ref e) if is_supported_extension(e) => Ok(()),
        Some(e) => Err(ValidationError {
            message: format!(
                "Unsupported file type \".{e}\". \
                 CodeScene Code Health analysis supports these file types: {}.\n\n\
                 See https://codescene.io/docs/usage/language-support.html for details.",
                SUPPORTED_EXTENSIONS.join(", ")
            ),
            kind: "unsupported_file_type",
        }),
        None => Err(ValidationError {
            message: format!(
                "The file '{}' has no file extension. \
                 CodeScene Code Health analysis requires a recognized source file type.\n\n\
                 See https://codescene.io/docs/usage/language-support.html for supported languages.",
                path.display()
            ),
            kind: "unsupported_file_type",
        }),
    }
}

fn validate_inside_git_repo(path: &Path) -> Result<(), ValidationError> {
    if cli::find_git_root(path).is_none() {
        return Err(ValidationError {
            message: format!(
                "The path '{}' is not inside a git repository. \
                 The CodeScene CLI requires a git repository to function. \
                 Please make sure the path points to a file or directory within a git repository.",
                path.display()
            ),
            kind: "not_a_git_repo",
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Supported extensions
// ---------------------------------------------------------------------------

/// The canonical set of file extensions supported by the CodeScene CLI
/// for Code Health analysis (full support). Derived from the CLI's own
/// error output and https://codescene.io/docs/usage/language-support.html.
///
/// Terraform (`.tf`) is intentionally excluded — it has X-Ray support
/// only, not full Code Health support.
const SUPPORTED_EXTENSIONS: &[&str] = &[
    // C / C++
    ".c", ".cc", ".cpp", ".cxx", ".h", ".hh", ".hpp", ".hxx", ".ipp",
    // C#
    ".cs",
    // Java
    ".java",
    // Groovy
    ".groovy",
    // JavaScript / TypeScript / React / Vue / ESM
    ".js", ".mjs", ".cjs", ".sj", ".ts", ".mts", ".cts", ".jsx", ".tsx", ".vue",
    // Objective-C
    ".m", ".mm",
    // Scala
    ".scala",
    // Python
    ".py", ".pyi",
    // Swift
    ".swift",
    // Go
    ".go",
    // Dart
    ".dart",
    // Visual Basic .NET
    ".vb",
    // PHP
    ".php",
    // Rust
    ".rs",
    // Ruby
    ".rb",
    // Kotlin
    ".kt", ".kts",
    // Perl
    ".pl", ".pm",
    // Erlang
    ".erl", ".hrl",
    // Elixir
    ".ex", ".exs",
    // Clojure
    ".clj", ".cljc", ".cljs",
    // PowerShell
    ".ps1", ".psm1", ".psd1",
    // TCL
    ".tcl",
    // Apex (Salesforce)
    ".cls", ".trigger", ".tgr",
    // BrightScript / BrighterScript
    ".brs", ".bs",
    // Rational Software Architect models (C++)
    ".efx", ".emx",
];

fn is_supported_extension(ext: &str) -> bool {
    // Compare without the leading dot since Path::extension() strips it.
    SUPPORTED_EXTENSIONS
        .iter()
        .any(|supported| supported.trim_start_matches('.') == ext)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // is_supported_extension
    // -----------------------------------------------------------------------

    #[test]
    fn recognises_common_extensions() {
        for ext in ["rs", "py", "js", "ts", "java", "go", "c", "cpp", "rb"] {
            assert!(
                is_supported_extension(ext),
                "Expected .{ext} to be supported"
            );
        }
    }

    #[test]
    fn rejects_unsupported_extensions() {
        for ext in ["txt", "md", "json", "yaml", "toml", "xml", "html", "css", "tf"] {
            assert!(
                !is_supported_extension(ext),
                "Expected .{ext} to be unsupported"
            );
        }
    }

    #[test]
    fn case_insensitive_via_caller_lowercasing() {
        assert!(is_supported_extension("rs"));
    }

    // -----------------------------------------------------------------------
    // validate_file_exists
    // -----------------------------------------------------------------------

    #[test]
    fn file_exists_passes_for_existing_file() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(validate_file_exists(&path).is_ok());
    }

    #[test]
    fn file_exists_fails_for_missing_file() {
        let err = validate_file_exists(Path::new("/nonexistent/file.rs")).unwrap_err();
        assert!(err.message.contains("does not exist"));
        assert_eq!(err.kind, "file_not_found");
    }

    // -----------------------------------------------------------------------
    // validate_supported_file_type
    // -----------------------------------------------------------------------

    #[test]
    fn supported_file_type_passes_for_known_extension() {
        assert!(validate_supported_file_type(Path::new("foo.rs")).is_ok());
        assert!(validate_supported_file_type(Path::new("/a/b/c.py")).is_ok());
        assert!(validate_supported_file_type(Path::new("test.JS")).is_ok());
    }

    #[test]
    fn supported_file_type_fails_for_unknown_extension() {
        let err = validate_supported_file_type(Path::new("readme.txt")).unwrap_err();
        assert!(err.message.contains("Unsupported file type"));
        assert!(err.message.contains(".txt"));
        assert!(err.message.contains("codescene.io"));
        assert_eq!(err.kind, "unsupported_file_type");
    }

    #[test]
    fn supported_file_type_fails_for_no_extension() {
        let err = validate_supported_file_type(Path::new("Makefile")).unwrap_err();
        assert!(err.message.contains("no file extension"));
        assert_eq!(err.kind, "unsupported_file_type");
    }

    // -----------------------------------------------------------------------
    // validate_inside_git_repo
    // -----------------------------------------------------------------------

    #[test]
    fn inside_git_repo_passes_for_repo_path() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(validate_inside_git_repo(path).is_ok());
    }

    #[test]
    fn inside_git_repo_fails_for_non_repo() {
        let dir = tempfile::tempdir().unwrap();
        let err = validate_inside_git_repo(dir.path()).unwrap_err();
        assert!(err.message.contains("not inside a git repository"));
        assert_eq!(err.kind, "not_a_git_repo");
    }

    // -----------------------------------------------------------------------
    // ProductionValidator::run_checks
    // -----------------------------------------------------------------------

    #[test]
    fn run_checks_passes_when_all_ok() {
        let v = ProductionValidator;
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let result = v.run_checks(&[
            Check::FileExists(&path),
            Check::InsideGitRepo(&path),
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn run_checks_short_circuits_on_first_failure() {
        let v = ProductionValidator;
        let path = Path::new("/nonexistent/file.rs");
        let err = v.run_checks(&[
            Check::FileExists(path),
            Check::SupportedFileType(path),
        ]);
        assert!(err.is_err());
        assert!(err.unwrap_err().message.contains("does not exist"));
    }

    #[test]
    fn run_checks_empty_is_ok() {
        let v = ProductionValidator;
        assert!(v.run_checks(&[]).is_ok());
    }

    // -----------------------------------------------------------------------
    // Supported extensions consistency
    // -----------------------------------------------------------------------

    #[test]
    fn supported_extensions_all_start_with_dot() {
        for ext in SUPPORTED_EXTENSIONS {
            assert!(ext.starts_with('.'), "Extension {ext} should start with '.'");
        }
    }

    #[test]
    fn no_duplicate_extensions() {
        let mut seen = std::collections::HashSet::new();
        for ext in SUPPORTED_EXTENSIONS {
            assert!(seen.insert(ext), "Duplicate extension: {ext}");
        }
    }
}
