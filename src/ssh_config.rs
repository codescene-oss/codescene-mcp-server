use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

#[derive(Default)]
pub(crate) struct SshOptions {
    pub user: Option<String>,
    pub port: Option<u16>,
}

pub(crate) async fn resolve_hostname(host: &str, options: &SshOptions) -> Option<String> {
    let config = dirs::home_dir()?.join(".ssh/config");
    match config.try_exists() {
        Ok(false) => return None,
        Ok(true) => {}
        Err(_) => {
            tracing::warn!("Cannot inspect SSH config; using original repository host");
            return None;
        }
    }
    match configured_hostname(&config, host, options).await {
        Ok(hostname) => Some(hostname),
        Err(reason) => {
            tracing::warn!(
                reason,
                "SSH host resolution failed; using original repository host"
            );
            None
        }
    }
}

async fn configured_hostname(
    config: &Path,
    host: &str,
    options: &SshOptions,
) -> Result<String, &'static str> {
    let mut command = ssh_command(config, host, options);
    let output = tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .map_err(|_| "ssh-config-timeout")?
        .map_err(|_| "ssh-unavailable")?;
    if !output.status.success() {
        return Err("ssh-config-failed");
    }
    let output = std::str::from_utf8(&output.stdout).map_err(|_| "invalid-ssh-config-output")?;
    parse_hostname(output).ok_or("invalid-ssh-hostname")
}

fn ssh_command(config: &Path, host: &str, options: &SshOptions) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("ssh");
    command
        .arg("-G")
        .arg("-F")
        .arg(config)
        .args([
            "-o",
            "CanonicalizeHostname=no",
            "-o",
            "PermitLocalCommand=no",
        ])
        .stdin(Stdio::null())
        .kill_on_drop(true);
    if let Some(user) = &options.user {
        command.arg("-l").arg(user);
    }
    if let Some(port) = options.port {
        command.arg("-p").arg(port.to_string());
    }
    command.arg("--").arg(host);
    for variable in crate::config::sensitive_env_vars() {
        command.env_remove(variable);
    }
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    command
}

fn parse_hostname(output: &str) -> Option<String> {
    let hostname = output.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let key = fields.next()?;
        let value = fields.next()?;
        (key.eq_ignore_ascii_case("hostname") && fields.next().is_none()).then_some(value)
    })?;
    let url = reqwest::Url::parse(&format!("ssh://{hostname}/")).ok()?;
    (url.username().is_empty()
        && url.port().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none())
    .then(|| url.host_str().map(str::to_string))
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_the_effective_hostname() {
        assert_eq!(
            parse_hostname("host github-work\nuser git\nhostname github.com\nport 22\n"),
            Some("github.com".to_string())
        );
        assert_eq!(
            parse_hostname("hostname ssh.dev.azure.com\n"),
            Some("ssh.dev.azure.com".to_string())
        );
    }

    #[test]
    fn rejects_missing_or_invalid_hostnames() {
        for output in [
            "",
            "host github-work\n",
            "hostname\n",
            "hostname invalid host\n",
            "hostname user@github.com\n",
            "hostname github.com/path\n",
            "hostname github.com:2222\n",
            "hostname github.com?query\n",
            "hostname github.com#fragment\n",
        ] {
            assert_eq!(parse_hostname(output), None, "{output}");
        }
    }

    #[test]
    fn passes_ssh_options_without_shell_interpretation() {
        let options = SshOptions {
            user: Some("git".to_string()),
            port: Some(2222),
        };
        let command = ssh_command(Path::new("config"), "-host;echo", &options);
        let args: Vec<_> = command.as_std().get_args().collect();
        assert_eq!(
            args,
            [
                "-G",
                "-F",
                "config",
                "-o",
                "CanonicalizeHostname=no",
                "-o",
                "PermitLocalCommand=no",
                "-l",
                "git",
                "-p",
                "2222",
                "--",
                "-host;echo",
            ]
        );
    }
}
