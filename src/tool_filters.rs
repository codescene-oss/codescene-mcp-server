use rmcp::handler::server::router::tool::ToolRouter;

use crate::config::{self, ConfigData};
use crate::CodeSceneServer;

pub(crate) const API_ONLY_TOOLS: &[&str] = &[
    "show_mcp_usage_overview",
    "select_project",
    "list_technical_debt_goals_for_project",
    "list_technical_debt_goals_for_project_file",
    "list_technical_debt_hotspots_for_project",
    "list_technical_debt_hotspots_for_project_file",
    "code_ownership_for_path",
];

/// Tools that cannot be disabled via `enabled_tools` config.
pub(crate) const ALWAYS_ENABLED_TOOLS: &[&str] = &[
    "get_config",
    "set_config",
    "login",
    "logout",
    "switch_account",
];

pub(crate) fn remove_standalone_tools(router: &mut ToolRouter<CodeSceneServer>) {
    for name in API_ONLY_TOOLS {
        router.remove_route(name);
    }
}

/// Tools that cannot work in Docker (OAuth browser callback is unsupported).
pub(crate) fn remove_docker_unsupported_tools(router: &mut ToolRouter<CodeSceneServer>) {
    router.remove_route("login");
    router.remove_route("switch_account");
}

pub(crate) fn apply_enabled_tools_filter(
    router: &mut ToolRouter<CodeSceneServer>,
    config_data: &ConfigData,
) {
    let enabled = match config::enabled_tools(config_data) {
        Some(set) => set,
        None => return,
    };
    let all_names: Vec<String> = router
        .list_all()
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    for name in all_names {
        if !ALWAYS_ENABLED_TOOLS.contains(&name.as_str()) && !enabled.contains(&name) {
            router.remove_route(&name);
        }
    }
}
