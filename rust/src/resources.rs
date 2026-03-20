/// Static documentation resources — mirrors Python's FileResource setup.
///
/// Embeds markdown documentation files and exposes them as MCP resources.

/// Code Health: how it works (embedded at compile time).
pub const HOW_IT_WORKS: &str =
    include_str!("docs/code-health/how-it-works.md");

/// The business case for Code Health (embedded at compile time).
pub const BUSINESS_CASE: &str =
    include_str!("docs/code-health/business-case.md");

/// Resource URI for the how-it-works document.
pub const HOW_IT_WORKS_URI: &str =
    "file://codescene-docs/code-health/how-it-works.md";

/// Resource URI for the business-case document.
pub const BUSINESS_CASE_URI: &str =
    "file://codescene-docs/code-health/business-case.md";
