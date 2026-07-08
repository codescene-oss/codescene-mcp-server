# Tools

The CodeScene MCP Server provides 24 tools organized into four categories:
**Code Health Analysis**, **Code Health Rules Configuration**, **Technical Debt & Project Insights**, and **Server Management**.

Tools marked **All Users** work with any valid license (standalone or API-connected).
Tools marked **CodeScene Core users (cloud or on-prem)** require a CodeScene API Personal Access Token and a CodeScene Core users (cloud or on-prem) instance — they are not available when running with a standalone license.

## Code Health Analysis

These tools analyze source code locally using the embedded CodeScene CLI.

### `code_health_score`

**Availability:** All Users

Calculate the Code Health score for a single source file. Returns one numeric score from 10.0 (optimal) to 1.0 (worst). Use for quick triage, ranking files by maintainability, or checking whether a refactoring improved file-level quality.

### `code_health_review`

**Availability:** All Users

Review the Code Health of a single source file and return a detailed review output that includes the score and code smell findings. Use when you need actionable maintainability diagnostics for one file, not just the score. The output includes a Code Health score and code smell details explaining why the score is high or low.

### `pre_commit_code_health_safeguard`

**Availability:** All Users

Review all modified and staged files in a repository and report Code Health degradations before commit. Use as a pre-commit safeguard on local changes to catch regressions and code smells before creating a commit. Returns quality gates summarizing whether the commit passes or fails Code Health thresholds, along with per-file findings.

### `analyze_change_set`

**Availability:** All Users

Run a branch-level Code Health review for all files that differ between the current HEAD and a base ref. Use as a local PR pre-flight check before opening a pull request, so regressions are caught across the full change set. Returns quality gates (passed/failed) and per-file verdicts (improved, degraded, or stable).

### `code_health_refactoring_business_case`

**Availability:** All Users

Generate a data-driven business case for refactoring a source file. Returns quantified predictions tied to the file's current Code Health, including optimistic and pessimistic outcome estimates for improvements in development speed and defect reduction, with a 90% confidence interval.

## Code Health Rules Configuration

These tools validate and edit `code-health-rules.json` files, which customize CodeScene's Code Health analysis by adjusting rule weights and thresholds. They are local, filesystem-only operations using the embedded CodeScene CLI and require no access token. When a `config_path` is provided it must be an absolute path; otherwise the CLI uses `.codescene/code-health-rules.json` in the current git repository.

### `rules_config_validate`

**Availability:** All Users

Validate a Code Health rules configuration file. Use after creating or editing a rules file to confirm it is well-formed. Returns a status and a human-readable summary of the number of rule sets, rule overrides, and threshold overrides.

### `rules_config_list_thresholds`

**Availability:** All Users

List the default Code Health thresholds for a programming language (e.g., Python, JavaScript, Java, C#). Use to discover the built-in threshold names and default values before overriding any of them. Returns a JSON object keyed by rule-set name, each with a `thresholds` array of `{ name, value }` entries.

### `rules_config_set_rule`

**Availability:** All Users

Enable or disable a Code Health rule in a rules file. Disabling a rule removes its impact from the Code Health score. Writes to the rules file and returns a confirmation naming the rule, its new state, and the edited rule set. `matching_content_path` is required only when the file defines multiple rule sets.

### `rules_config_set_threshold`

**Availability:** All Users

Set a Code Health threshold value in a rules file (for example, the number of lines at which a function is flagged as a "Large Method"). The value must be a positive integer. Writes to the rules file and returns a confirmation naming the threshold, its new value, and the edited rule set. `matching_content_path` is required only when the file defines multiple rule sets.

## Technical Debt & Project Insights

These tools connect to the CodeScene API to provide project-level analysis data. They require a CodeScene Core users (cloud or on-prem) instance and a Personal Access Token.

### `select_project`

**Availability:** CodeScene Core users (cloud or on-prem)

List all projects for an organization for selection by the user. Use before other project-scoped tools so the user can pick the project context explicitly. If `default_project_id` is configured, the server returns that project and selection is locked.

### `list_technical_debt_hotspots_for_project`

**Availability:** CodeScene Core users (cloud or on-prem)

List the technical debt hotspots for a project. Use to identify high-impact technical debt hotspots across a project and prioritize refactoring targets. Returns file paths, Code Health scores, revision counts, and lines of code, with a link to the CodeScene hotspots page.

### `list_technical_debt_hotspots_for_project_file`

**Availability:** CodeScene Core users (cloud or on-prem)

List the technical debt hotspots for a specific file in a project. Use to inspect hotspot metrics for one file before deciding if it should be a refactoring candidate. Returns the Code Health score, revision count, and lines of code for the specified file.

### `list_technical_debt_goals_for_project`

**Availability:** CodeScene Core users (cloud or on-prem)

List the technical debt goals for a project. Use to see all files in a project that currently have explicit technical debt goals in CodeScene. Returns goal data from the latest available analysis, including only files with non-empty goals, along with a link to the Code Biomarkers page.

### `list_technical_debt_goals_for_project_file`

**Availability:** CodeScene Core users (cloud or on-prem)

List the technical debt goals for a specific file in a project. Use when you need goal details for one file before planning targeted refactoring work. Returns data from the latest available analysis.

### `code_ownership_for_path`

**Availability:** CodeScene Core users (cloud or on-prem)

Find the owner or owners of a specific path in a project. Use to identify likely reviewers or domain experts for code reviews and technical questions about a file or directory. Returns a list of owners with their key areas and links to the CodeScene System Map page.

## Server Management

These tools manage server configuration, installation verification, and skills.

### `get_config`

**Availability:** All Users

Read current CodeScene MCP Server configuration values. Use to discover available configuration keys, inspect effective values, and understand where each value comes from. Sensitive values (tokens) are masked. This tool cannot be disabled via the `enabled_tools` configuration.

### `set_config`

**Availability:** All Users

Write a CodeScene MCP Server configuration value. Persists the value to the config file and applies it to the running session immediately. To remove a value, pass an empty string. This tool cannot be disabled via the `enabled_tools` configuration.

### `verify_installation`

**Availability:** All Users

Check if the CodeScene MCP Server is correctly installed and configured. Use to diagnose setup issues such as missing tokens, unavailable git, or environment misconfigurations. Returns PASS/FAIL status for Git, Git Repository, Access Token, and Runtime Environment checks.

### `list_skills`

**Availability:** All Users

List all available skills embedded in this MCP server. Use to discover what skills are available for download or inspection. Returns only skills embedded at compile time.

### `get_skill_manifest`

**Availability:** All Users

Get the file manifest for a specific skill. Use to inspect what files a skill contains, their sizes, and SHA256 hashes before downloading.

### `download_skill`

**Availability:** All Users

Download a single skill to a local directory. Use to install a specific skill into your local skills directory. By default, refuses to overwrite existing skills — set `overwrite=true` to replace an existing skill.

### `sync_skills`

**Availability:** All Users

Download all available skills to a local directory. Use to install every embedded skill at once. By default, skips skills that already exist locally — set `overwrite=true` to replace all existing skills.

### `explain_code_health`

**Availability:** All Users

Explains CodeScene's Code Health metric for assessing code quality and maintainability for both human developers and AI. Returns static documentation text (Markdown) explaining the Code Health model and core concepts. Does not analyze a specific repository or file.

### `explain_code_health_productivity`

**Availability:** All Users

Describes how to build a business case for Code Health improvements. Covers empirical data on how healthy code lets you ship faster with fewer defects. Returns static documentation text with productivity and defect-risk implications. Does not compute project-specific forecasts.

## Summary

| Tool | Availability |
|------|-------------|
| `code_health_score` | All Users |
| `code_health_review` | All Users |
| `pre_commit_code_health_safeguard` | All Users |
| `analyze_change_set` | All Users |
| `code_health_refactoring_business_case` | All Users |
| `rules_config_validate` | All Users |
| `rules_config_list_thresholds` | All Users |
| `rules_config_set_rule` | All Users |
| `rules_config_set_threshold` | All Users |
| `select_project` | CodeScene Core users (cloud or on-prem) |
| `list_technical_debt_hotspots_for_project` | CodeScene Core users (cloud or on-prem) |
| `list_technical_debt_hotspots_for_project_file` | CodeScene Core users (cloud or on-prem) |
| `list_technical_debt_goals_for_project` | CodeScene Core users (cloud or on-prem) |
| `list_technical_debt_goals_for_project_file` | CodeScene Core users (cloud or on-prem) |
| `code_ownership_for_path` | CodeScene Core users (cloud or on-prem) |
| `get_config` | All Users |
| `set_config` | All Users |
| `verify_installation` | All Users |
| `list_skills` | All Users |
| `get_skill_manifest` | All Users |
| `download_skill` | All Users |
| `sync_skills` | All Users |
| `explain_code_health` | All Users |
| `explain_code_health_productivity` | All Users |
