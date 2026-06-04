// Re-export crate-root items so `use super::*;` works in submodules.
pub use crate::file_utils::{create_git_repo, create_temp_dir};
pub use crate::fixtures::get_sample_files;
pub use crate::mcp_client::MCPClient;
pub use crate::response_parsers::{extract_code_health_score, extract_result_text};
pub use crate::server_backends::{
    base_env, create_backend, docker_ca_bundle, docker_config_dir, fake_server_bind_host, fake_server_url_host,
    is_docker, skip_if_docker, ServerBackend,
};
pub use crate::{find_or_build_executable, make_client, setup};

pub use serde_json::json;
pub use std::path::Path;
pub use std::time::Duration;

#[allow(dead_code)]
pub mod fake_http_server;
pub mod fake_https_server;

pub mod analyze_change_set;
pub mod analytics_environment_override;
pub mod analytics_tracking;
pub mod bundled_docs;
pub mod business_case;
pub mod cloudfront_headers;
pub mod configure;
pub mod docker_path_translation;
pub mod enabled_tools;
pub mod error_logging;
pub mod git_subtree;
pub mod git_worktree;
pub mod platform_specific;
pub mod relative_paths;
pub mod require_access_token;
pub mod shutdown_during_handshake;
pub mod skill_resources;
pub mod ssl_api_ca_bundle;
pub mod ssl_cli_truststore;
pub mod standalone_license;
pub mod stress_code_health_review;
pub mod version_check;
