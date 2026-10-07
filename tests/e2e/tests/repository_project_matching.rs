//! Live repository-to-project matching integration tests.
//!
//! Validates that Git remotes resolve to the project IDs returned by CodeScene,
//! including repositories from the active account and other accessible accounts.
//! Linked worktrees must inherit those mappings from the main checkout.

use super::fake_http_server::FakeHttpServer;
use super::*;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Instant;

const REPOSITORY_PROJECTS_URL: &str = "https://api.codescene.io/v2/mcp/repository-projects";
const PROJECTS_URL: &str = "https://api.codescene.io/v2/projects";
const EVENT_TYPE: &str = "mcp-get-config";
const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Deserialize)]
struct RepositoryMapping {
    repository_id: String,
    project_ids: Vec<i64>,
}

#[derive(Deserialize)]
struct RepositoryMappingsResponse {
    repositories: Vec<RepositoryMapping>,
}

#[derive(Deserialize)]
struct Project {
    id: i64,
}

#[derive(Deserialize)]
struct ProjectsResponse {
    projects: Vec<Project>,
    page: usize,
    max_pages: usize,
}

struct LiveMappings {
    all: Vec<RepositoryMapping>,
    active_account: RepositoryMapping,
    other_account: Option<RepositoryMapping>,
}

pub fn test_matches_repository_from_active_account() {
    let mapping = live_mappings().active_account.clone();

    assert_repository_matches(&[("origin", remote_url(&mapping))], mapping.project_ids);
}

pub fn test_matches_repository_from_other_account() {
    let Some(mapping) = live_mappings().other_account.clone() else {
        eprintln!("Skipping cross-account matching: the E2E token has one account");
        return;
    };

    assert_repository_matches(&[("origin", scp_remote_url(&mapping))], mapping.project_ids);
}

pub fn test_matches_worktree_projects_from_active_account() {
    if is_docker() {
        skip_if_docker("linked worktree metadata points outside the mounted checkout");
        return;
    }
    let mapping = live_mappings().active_account.clone();

    assert_worktree_matches(remote_url(&mapping), mapping.project_ids, false);
}

pub fn test_matches_worktree_subdirectory_projects_from_other_account() {
    if is_docker() {
        skip_if_docker("linked worktree metadata points outside the mounted checkout");
        return;
    }
    let Some(mapping) = live_mappings().other_account.clone() else {
        eprintln!("Skipping cross-account worktree matching: no other-account fixture");
        return;
    };

    assert_worktree_matches(scp_remote_url(&mapping), mapping.project_ids, true);
}

pub fn test_unions_projects_across_accounts() {
    let mappings = live_mappings();
    if mappings.other_account.is_none() {
        eprintln!("Skipping cross-account union: the E2E token has one account");
        return;
    }
    let remotes = mappings
        .all
        .iter()
        .enumerate()
        .map(|(index, mapping)| {
            let url = if index % 2 == 0 {
                remote_url(mapping)
            } else {
                scp_remote_url(mapping)
            };
            (format!("repository-{index}"), url)
        })
        .collect::<Vec<_>>();
    let expected = mappings
        .all
        .iter()
        .flat_map(|mapping| mapping.project_ids.iter().copied())
        .collect();
    let remotes = remotes
        .iter()
        .map(|(name, url)| (name.as_str(), url.clone()))
        .collect::<Vec<_>>();

    assert_repository_matches(&remotes, expected);
}

fn live_mappings() -> &'static LiveMappings {
    static MAPPINGS: OnceLock<LiveMappings> = OnceLock::new();
    MAPPINGS.get_or_init(fetch_live_mappings)
}

fn fetch_live_mappings() -> LiveMappings {
    let token = std::env::var("CS_ACCESS_TOKEN")
        .expect("CS_ACCESS_TOKEN is required for repository matching E2E tests");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("create HTTP runtime");
    runtime.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("create HTTP client");
        let mappings: RepositoryMappingsResponse =
            fetch_json(&client, &token, REPOSITORY_PROJECTS_URL).await;
        let active_project_ids = fetch_active_project_ids(&client, &token).await;

        classify_mappings(mappings.repositories, &active_project_ids)
    })
}

async fn fetch_active_project_ids(client: &reqwest::Client, token: &str) -> BTreeSet<i64> {
    let mut project_ids = BTreeSet::new();
    let mut page = 1;
    loop {
        let url = format!("{PROJECTS_URL}?page={page}");
        let response: ProjectsResponse = fetch_json(client, token, &url).await;
        project_ids.extend(response.projects.into_iter().map(|project| project.id));
        if response.page >= response.max_pages {
            return project_ids;
        }
        page = response.page + 1;
    }
}

async fn fetch_json<T: DeserializeOwned>(client: &reqwest::Client, token: &str, url: &str) -> T {
    client
        .get(url)
        .header("Accept", "application/json")
        .header(
            "User-Agent",
            format!("codescene-mcp/{}", env!("CS_MCP_VERSION")),
        )
        .header("X-CS-Source", "mcp")
        .bearer_auth(token)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .unwrap_or_else(|error| panic!("CodeScene API request failed: {error}"))
        .json()
        .await
        .unwrap_or_else(|error| panic!("CodeScene API returned invalid JSON: {error}"))
}

fn classify_mappings(
    mut mappings: Vec<RepositoryMapping>,
    active_project_ids: &BTreeSet<i64>,
) -> LiveMappings {
    mappings.sort_by(|left, right| left.repository_id.cmp(&right.repository_id));
    let active_account = mappings
        .iter()
        .find(|mapping| {
            !mapping.project_ids.is_empty()
                && mapping
                    .project_ids
                    .iter()
                    .all(|id| active_project_ids.contains(id))
        })
        .cloned()
        .expect("The E2E token must expose one repository in its active account");
    let other_account = mappings
        .iter()
        .find(|mapping| {
            !mapping.project_ids.is_empty()
                && mapping
                    .project_ids
                    .iter()
                    .all(|id| !active_project_ids.contains(id))
        })
        .cloned();

    LiveMappings {
        all: mappings,
        active_account,
        other_account,
    }
}

fn remote_url(mapping: &RepositoryMapping) -> String {
    format!("https://{}.git", mapping.repository_id)
}

fn scp_remote_url(mapping: &RepositoryMapping) -> String {
    let (host, path) = mapping
        .repository_id
        .split_once('/')
        .expect("repository ID should contain a host and path");
    format!("git@{host}:{path}.git")
}

fn assert_repository_matches(remotes: &[(&str, String)], expected_project_ids: Vec<i64>) {
    let (command, env, repo_dir, _tmp) = setup();
    for (name, url) in remotes {
        git_in(&repo_dir, &["remote", "add", name, url]);
    }

    assert_workspace_matches(&command, env, &repo_dir, expected_project_ids);
}

fn assert_worktree_matches(remote: String, expected_project_ids: Vec<i64>, nested: bool) {
    let (command, env, repo_dir, _tmp) = setup();
    git_in(&repo_dir, &["remote", "add", "origin", &remote]);
    let worktree_dir = repo_dir
        .parent()
        .expect("repository parent")
        .join("ai-worktree");
    git_in(
        &repo_dir,
        &[
            "worktree",
            "add",
            "-b",
            "ai-feature",
            &worktree_dir.to_string_lossy(),
        ],
    );
    assert!(
        worktree_dir.join(".git").is_file(),
        "Expected a linked worktree"
    );
    let workspace_dir = if nested {
        worktree_dir.join("src/utils")
    } else {
        worktree_dir
    };

    assert_workspace_matches(&command, env, &workspace_dir, expected_project_ids);
}

fn assert_workspace_matches(
    command: &[String],
    mut env: Vec<(String, String)>,
    workspace_dir: &Path,
    expected_project_ids: Vec<i64>,
) {
    use_isolated_config_dir(&mut env, workspace_dir, ".cs_config_repository_matching");
    let tracking_server = FakeHttpServer::always_ok();
    replace_env(&mut env, "CS_TRACKING_URL", &tracking_server.url());
    replace_env(&mut env, "CS_DISABLE_TRACKING", "0");
    env.retain(|(key, _)| key != "CS_ONPREM_URL");

    let mut client = make_client(command, &env, workspace_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");
    let response = client
        .call_tool("get_config", json!({}), TIMEOUT)
        .expect("get_config should succeed");
    assert!(!extract_result_text(&response).is_empty());

    let properties = wait_for_event_properties(&tracking_server);
    let actual_project_ids = properties
        .get("project-ids")
        .and_then(serde_json::Value::as_array)
        .expect("matching should emit project-ids")
        .iter()
        .map(|id| id.as_i64().expect("project ID should be an integer"))
        .collect::<Vec<_>>();
    let expected_project_ids = expected_project_ids
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    assert!(
        actual_project_ids == expected_project_ids,
        "Matched project IDs differ: expected {} IDs, received {}",
        expected_project_ids.len(),
        actual_project_ids.len()
    );
    assert!(properties.get("no-project-matching-reason").is_none());
}

fn wait_for_event_properties(server: &FakeHttpServer) -> serde_json::Value {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(properties) = server.get_payloads().iter().find_map(|payload| {
            (payload
                .get("event-type")
                .and_then(serde_json::Value::as_str)
                == Some(EVENT_TYPE))
            .then(|| payload.get("event-properties").cloned())
            .flatten()
        }) {
            return properties;
        }
        assert!(
            Instant::now() < deadline,
            "Timed out waiting for repository matching analytics"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn replace_env(env: &mut Vec<(String, String)>, key: &str, value: &str) {
    env.retain(|(existing, _)| existing != key);
    env.push((key.to_string(), value.to_string()));
}

fn git_in(repo_dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_dir)
        .output()
        .expect("run git command");
    assert!(output.status.success(), "git command failed");
}
