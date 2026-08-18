use crate::auth::AuthManager;
use crate::cli::{self, CliRunner};
use crate::config;

#[derive(Debug)]
pub(crate) enum CliAction {
    RunServer,
    PrintHelp,
    PrintVersion(String),
    PrintCliVersion,
    Auth,
    Logout,
    SwitchAccount(i64),
    ListAccounts,
}

pub(crate) fn display_version(raw_version: &str) -> &str {
    raw_version.strip_prefix("MCP-").unwrap_or(raw_version)
}

pub(crate) fn help_text() -> &'static str {
    "CodeScene MCP Server\n\nUsage: cs-mcp [OPTIONS]\n\nOptions:\n  -h, --help              Show this help message and exit\n  -v, --version           Show version and exit\n  --cli-version           Show embedded CLI version and exit\n  auth                    Sign in to CodeScene via OAuth (opens browser)\n  auth logout             Sign out of CodeScene OAuth and clear the stored session\n  auth list-accounts      List Cloud OAuth accounts available to the signed-in user\n  auth switch <account>   Switch Cloud OAuth account (reuses stored session when possible)\n\nEnvironment:\n  CS_ONPREM_URL    Base URL of self-hosted CodeScene instance (for auth subcommand)\n  CS_ACCOUNT_ID    Optional Cloud account/tenant ID for OAuth credential slot selection"
}

pub(crate) async fn ensure_oauth_client_configured() {
    if crate::config::try_read_env("CS_OAUTH_CLIENT").is_some() {
        return;
    }

    if let Err(e) = crate::config::write_env("oauth_client", "mcp").await {
        tracing::warn!(error = %e, "failed to persist default OAuth client");
    }
}

enum ArgToken {
    Help,
    Version,
    CliVersion,
    Auth,
    Logout,
    Switch,
    ListAccounts,
    Value(String),
}

impl ArgToken {
    fn from_arg(arg: &str) -> Self {
        match arg {
            "-h" | "--help" => Self::Help,
            "-v" | "--version" => Self::Version,
            "--cli-version" => Self::CliVersion,
            "auth" => Self::Auth,
            "logout" => Self::Logout,
            "switch" => Self::Switch,
            "list-accounts" => Self::ListAccounts,
            other => Self::Value(other.to_string()),
        }
    }

    fn display(&self) -> &str {
        match self {
            Self::Help => "--help",
            Self::Version => "--version",
            Self::CliVersion => "--cli-version",
            Self::Auth => "auth",
            Self::Logout => "logout",
            Self::Switch => "switch",
            Self::ListAccounts => "list-accounts",
            Self::Value(value) => value,
        }
    }

    fn to_single_action(&self, raw_version: &str) -> Result<CliAction, String> {
        match self {
            Self::Help => Ok(CliAction::PrintHelp),
            Self::Version => Ok(CliAction::PrintVersion(
                display_version(raw_version).to_string(),
            )),
            Self::CliVersion => Ok(CliAction::PrintCliVersion),
            Self::Auth => Ok(CliAction::Auth),
            Self::Logout => Ok(CliAction::Logout),
            other => Err(format!("Unknown argument: {}", other.display())),
        }
    }
}

pub(crate) fn parse_cli_args(args: &[String], raw_version: &str) -> Result<CliAction, String> {
    let tokens: Vec<ArgToken> = args.iter().map(|arg| ArgToken::from_arg(arg)).collect();
    match tokens.as_slice() {
        [] => Ok(CliAction::RunServer),
        [single] => single.to_single_action(raw_version),
        rest => parse_auth_tokens(rest),
    }
}

fn parse_auth_tokens(tokens: &[ArgToken]) -> Result<CliAction, String> {
    match tokens {
        [ArgToken::Auth, ArgToken::Logout] => Ok(CliAction::Logout),
        [ArgToken::Auth, ArgToken::ListAccounts] => Ok(CliAction::ListAccounts),
        [ArgToken::Auth, ArgToken::Switch] => Err(
            "auth switch requires an account id, e.g. cs-mcp auth switch 123".to_string(),
        ),
        [ArgToken::Auth, ArgToken::Switch, ArgToken::Value(id)] => parse_switch_id(id),
        _ => Err(unexpected_cli_args(tokens)),
    }
}

fn parse_switch_id(id: &str) -> Result<CliAction, String> {
    let account_id: i64 = id
        .parse()
        .map_err(|_| format!("Invalid account id for auth switch: {id}"))?;
    if account_id <= 0 {
        return Err("account id for auth switch must be a positive integer".to_string());
    }
    Ok(CliAction::SwitchAccount(account_id))
}

fn unexpected_cli_args(tokens: &[ArgToken]) -> String {
    let rendered: Vec<&str> = tokens.iter().map(ArgToken::display).collect();
    format!("Unexpected arguments: {}", rendered.join(" "))
}

pub(crate) async fn fetch_cli_version(cli_runner: &dyn CliRunner) -> anyhow::Result<String> {
    Ok(cli_runner.run(&["version"], None).await?)
}

async fn prepare_auth_cli_env() {
    config::snapshot_client_env_vars();
    ensure_oauth_client_configured().await;
    let config_data = config::load().unwrap_or_default();
    config::apply_to_env(&config_data);
}

/// Testable login CLI flow using an injected runner.
pub(crate) async fn run_auth_flow_with(cli_runner: &dyn CliRunner) -> anyhow::Result<()> {
    prepare_auth_cli_env().await;

    let auth_manager = AuthManager::new();

    if let Ok(Some(_)) = auth_manager.current_token(cli_runner).await {
        let result = serde_json::json!({"status": "already_signed_in"});
        println!("{}", result);
        return Ok(());
    }

    match auth_manager.login(cli_runner).await {
        Ok(resp) if resp.is_signed_in() => {
            let result = serde_json::json!({"status": "signed_in"});
            println!("{}", result);
            Ok(())
        }
        Ok(resp) => {
            let result = serde_json::json!({"status": resp.status, "error": "Login did not complete"});
            println!("{}", result);
            anyhow::bail!("Login did not complete");
        }
        Err(e) => {
            let result = serde_json::json!({"status": "error", "error": e});
            println!("{}", result);
            anyhow::bail!("Login failed: {e}");
        }
    }
}

/// Testable account-switch CLI flow using an injected runner.
pub(crate) async fn run_switch_account_flow_with(
    cli_runner: &dyn CliRunner,
    account_id: i64,
) -> anyhow::Result<()> {
    prepare_auth_cli_env().await;

    let auth_manager = AuthManager::new();
    match auth_manager.switch_account(cli_runner, account_id).await {
        Ok(result) => {
            let payload = serde_json::json!({
                "status": result.status_str(),
                "account_id": result.account_id,
            });
            println!("{payload}");
            Ok(())
        }
        Err(e) => {
            let payload = serde_json::json!({
                "status": "error",
                "account_id": account_id,
                "error": e,
            });
            println!("{payload}");
            anyhow::bail!("Account switch failed: {e}");
        }
    }
}

/// Testable account-list CLI flow using an injected runner.
pub(crate) async fn run_list_accounts_flow_with(cli_runner: &dyn CliRunner) -> anyhow::Result<()> {
    prepare_auth_cli_env().await;

    let current_account_id = AuthManager::new().try_cached_oauth_account_id();
    match crate::auth::list_accounts_payload(cli_runner, current_account_id).await {
        Ok(payload) => {
            println!("{payload}");
            Ok(())
        }
        Err(e) => {
            let payload = serde_json::json!({"status": "error", "error": e});
            println!("{payload}");
            anyhow::bail!("List accounts failed: {e}");
        }
    }
}

/// Testable logout CLI flow using an injected runner.
pub(crate) async fn run_logout_flow_with(cli_runner: &dyn CliRunner) -> anyhow::Result<()> {
    prepare_auth_cli_env().await;

    let auth_manager = AuthManager::new();
    match auth_manager.logout(cli_runner).await {
        Ok(()) => {
            let result = serde_json::json!({"status": "signed_out"});
            println!("{}", result);
            Ok(())
        }
        Err(e) => {
            // Local OAuth state is already cleared; still report the CLI error.
            let result = serde_json::json!({"status": "error", "error": e});
            println!("{}", result);
            anyhow::bail!("Logout failed: {e}");
        }
    }
}

/// Run a parsed CLI subcommand. Returns `true` when the process should exit.
pub(crate) async fn dispatch_cli_action(action: CliAction) -> anyhow::Result<bool> {
    dispatch_cli_action_with(action, &cli::ProductionCliRunner).await
}

/// Testable CLI dispatch using an injected runner.
pub(crate) async fn dispatch_cli_action_with(
    action: CliAction,
    cli_runner: &dyn CliRunner,
) -> anyhow::Result<bool> {
    match action {
        CliAction::RunServer => Ok(false),
        CliAction::PrintHelp => {
            println!("{}", help_text());
            Ok(true)
        }
        CliAction::PrintVersion(version) => {
            println!("{version}");
            Ok(true)
        }
        CliAction::PrintCliVersion => {
            let output = fetch_cli_version(cli_runner).await?;
            print!("{output}");
            Ok(true)
        }
        CliAction::Auth => {
            run_auth_flow_with(cli_runner).await?;
            Ok(true)
        }
        CliAction::Logout => {
            run_logout_flow_with(cli_runner).await?;
            Ok(true)
        }
        CliAction::SwitchAccount(account_id) => {
            run_switch_account_flow_with(cli_runner, account_id).await?;
            Ok(true)
        }
        CliAction::ListAccounts => {
            run_list_accounts_flow_with(cli_runner).await?;
            Ok(true)
        }
    }
}
