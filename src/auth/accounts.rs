use serde::{Deserialize, Serialize};

use crate::cli::CliRunner;

use super::{run_auth_command, sanitized_output_preview};

/// One Cloud account from `cs auth list-accounts --output-format json`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct CloudAccount {
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub account_type: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    pub authenticated: bool,
    #[serde(default, skip_deserializing)]
    pub current: bool,
}

#[derive(Debug, Deserialize)]
struct ListAccountsResponse {
    accounts: Vec<CloudAccount>,
}

/// How `name` / slug lookup resolved against a list of Cloud accounts.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AccountNameMatch {
    Unique(CloudAccount),
    None,
    Ambiguous(Vec<CloudAccount>),
}

/// Run `cs auth list-accounts --client mcp --output-format json`.
pub(crate) async fn list_accounts(
    cli_runner: &dyn CliRunner,
) -> Result<Vec<CloudAccount>, String> {
    let output = run_auth_command(cli_runner, "list-accounts").await?;
    serde_json::from_str::<ListAccountsResponse>(output.trim())
        .map(|parsed| parsed.accounts)
        .map_err(|e| {
            let output_preview = sanitized_output_preview(&output);
            tracing::warn!(
                error = %e,
                output_preview,
                "failed to parse list-accounts response"
            );
            "Failed to parse list-accounts response from CLI".to_string()
        })
}

/// JSON payload for listing accounts, with `current` set from the active session.
pub(crate) fn accounts_list_json(
    accounts: &[CloudAccount],
    current_account_id: Option<i64>,
) -> serde_json::Value {
    let accounts: Vec<CloudAccount> = accounts
        .iter()
        .cloned()
        .map(|mut account| {
            account.current = current_account_id == Some(account.id);
            account
        })
        .collect();
    serde_json::json!({ "accounts": accounts })
}

/// List Cloud accounts and mark the active session account.
pub(crate) async fn list_accounts_payload(
    cli_runner: &dyn CliRunner,
    current_account_id: Option<i64>,
) -> Result<serde_json::Value, String> {
    let accounts = list_accounts(cli_runner).await?;
    Ok(accounts_list_json(&accounts, current_account_id))
}

/// Resolve a name/slug against `cs auth list-accounts`.
pub(crate) async fn resolve_named_account(
    cli_runner: &dyn CliRunner,
    name: &str,
) -> Result<(Vec<CloudAccount>, AccountNameMatch), String> {
    let accounts = list_accounts(cli_runner).await?;
    let matched = match_account_name(&accounts, name);
    Ok((accounts, matched))
}

/// Case-insensitive exact match on display name or org slug.
pub(crate) fn match_account_name(accounts: &[CloudAccount], query: &str) -> AccountNameMatch {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return AccountNameMatch::None;
    }
    let mut matched = accounts
        .iter()
        .filter(|account| account_matches_name(account, &needle))
        .cloned();
    match (matched.next(), matched.next()) {
        (None, _) => AccountNameMatch::None,
        (Some(only), None) => AccountNameMatch::Unique(only),
        (Some(first), Some(second)) => {
            let mut rest = vec![first, second];
            rest.extend(matched);
            AccountNameMatch::Ambiguous(rest)
        }
    }
}

fn account_matches_name(account: &CloudAccount, needle: &str) -> bool {
    account.name.to_lowercase() == needle
        || account
            .slug
            .as_deref()
            .is_some_and(|slug| slug.to_lowercase() == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::MockCliRunner;

    const SAMPLE_ACCOUNTS_JSON: &str = r#"{"accounts":[{"id":139802,"name":"Martin Säfsten","type":"individual","role":"owner","authenticated":true},{"id":11,"name":"CodeScene Showcase","type":"org","role":"member","slug":"codescene-showcase","authenticated":true}]}"#;

    fn sample_accounts() -> Vec<CloudAccount> {
        serde_json::from_str::<ListAccountsResponse>(SAMPLE_ACCOUNTS_JSON)
            .unwrap()
            .accounts
    }

    #[tokio::test]
    async fn list_accounts_includes_cli_stderr_on_failure() {
        let cli = MockCliRunner::with_err(1, "Failed to list accounts (HTTP 401).");
        let err = list_accounts(&cli).await.unwrap_err();
        assert!(err.contains("HTTP 401"), "got: {err}");
    }

    #[tokio::test]
    async fn list_accounts_parses_cli_json() {
        let cli = MockCliRunner::with_ok(SAMPLE_ACCOUNTS_JSON);
        let accounts = list_accounts(&cli).await.unwrap();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[1].id, 11);
        assert_eq!(accounts[1].slug.as_deref(), Some("codescene-showcase"));
    }

    #[tokio::test]
    async fn list_accounts_reports_parse_error() {
        let err = list_accounts(&MockCliRunner::with_ok("not-json"))
            .await
            .unwrap_err();
        assert!(
            err.contains("Failed to parse list-accounts response"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn list_accounts_payload_and_resolve_named_account() {
        let cli = MockCliRunner::with_ok(SAMPLE_ACCOUNTS_JSON);
        let payload = list_accounts_payload(&cli, Some(11)).await.unwrap();
        assert_eq!(payload["accounts"][1]["current"], true);

        let cli = MockCliRunner::with_ok(SAMPLE_ACCOUNTS_JSON);
        let (accounts, matched) = resolve_named_account(&cli, "codescene-showcase")
            .await
            .unwrap();
        assert_eq!(accounts.len(), 2);
        assert!(matches!(
            matched,
            AccountNameMatch::Unique(ref account) if account.id == 11
        ));
    }

    #[test]
    fn match_account_name_treats_blank_query_as_none() {
        assert!(matches!(
            match_account_name(&sample_accounts(), "   "),
            AccountNameMatch::None
        ));
    }

    #[test]
    fn match_account_name_resolves_unique_name_and_slug() {
        let accounts = sample_accounts();
        assert!(matches!(
            match_account_name(&accounts, "  CodeScene Showcase  "),
            AccountNameMatch::Unique(ref account) if account.id == 11
        ));
        assert!(matches!(
            match_account_name(&accounts, "CODESCENE-SHOWCASE"),
            AccountNameMatch::Unique(ref account) if account.id == 11
        ));
    }

    #[test]
    fn match_account_name_reports_unknown_and_ambiguous() {
        let mut accounts = sample_accounts();
        accounts.push(CloudAccount {
            id: 12,
            name: "CodeScene Showcase".to_string(),
            account_type: "org".to_string(),
            role: "member".to_string(),
            slug: Some("codescene-showcase-eu".to_string()),
            authenticated: true,
            current: false,
        });
        assert!(matches!(
            match_account_name(&accounts, "missing"),
            AccountNameMatch::None
        ));
        assert!(matches!(
            match_account_name(&accounts, "CodeScene Showcase"),
            AccountNameMatch::Ambiguous(ref matched) if matched.len() == 2
        ));
    }

    #[test]
    fn accounts_list_json_marks_current_account() {
        let payload = accounts_list_json(&sample_accounts(), Some(11));
        let accounts = payload["accounts"].as_array().unwrap();
        assert_eq!(accounts[0]["current"], false);
        assert_eq!(accounts[1]["current"], true);
        assert_eq!(accounts[1]["id"], 11);
    }
}
