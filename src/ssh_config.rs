use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

#[derive(Default)]
pub(crate) struct SshOptions {
    pub user: Option<String>,
    pub port: Option<u16>,
}

pub(crate) async fn resolve_hostname(host: &str, options: &SshOptions) -> Option<String> {
    let config = ssh_home_dir()?.join(".ssh/config");
    resolve_from_config(&config, host, options).await
}

fn ssh_home_dir() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    if let Some(profile) = std::env::var_os("USERPROFILE").filter(|value| !value.is_empty()) {
        return Some(profile.into());
    }
    dirs::home_dir()
}

async fn resolve_from_config(config: &Path, host: &str, options: &SshOptions) -> Option<String> {
    match config.try_exists() {
        Ok(false) => return None,
        Ok(true) => {}
        Err(_) => {
            tracing::warn!("Cannot inspect SSH config; using original repository host");
            return None;
        }
    }
    match configured_hostname(config, host, options).await {
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
    evaluate_command(ssh_command(config, host, options)).await
}

async fn evaluate_command(mut command: tokio::process::Command) -> Result<String, &'static str> {
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

    #[cfg(windows)]
    #[test]
    fn ssh_home_honors_userprofile_override() {
        let _lock = crate::config::lock_test_env();
        let saved = std::env::var_os("USERPROFILE");
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("USERPROFILE", home.path());
        let actual = ssh_home_dir();
        match saved {
            Some(profile) => std::env::set_var("USERPROFILE", profile),
            None => std::env::remove_var("USERPROFILE"),
        }
        assert_eq!(actual.as_deref(), Some(home.path()));
    }

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

    #[cfg(unix)]
    mod evaluation {
        use super::*;
        use std::ffi::OsString;
        use std::os::unix::fs::{symlink, PermissionsExt};

        struct SshEnvironment {
            directory: tempfile::TempDir,
            saved: Option<OsString>,
            _lock: std::sync::MutexGuard<'static, ()>,
        }

        impl SshEnvironment {
            fn new(script: &str) -> Self {
                let lock = crate::config::lock_test_env();
                let directory = tempfile::tempdir().unwrap();
                let saved = std::env::var_os("HOME");
                std::env::set_var("HOME", directory.path());
                let fixture = Self {
                    directory,
                    saved,
                    _lock: lock,
                };
                std::fs::create_dir(fixture.directory.path().join(".ssh")).unwrap();
                std::fs::write(fixture.config(), "Host alias\nHostName github.com\n").unwrap();
                fixture.write_script(script);
                fixture
            }

            fn config(&self) -> std::path::PathBuf {
                self.directory.path().join(".ssh/config")
            }

            fn write_script(&self, script: &str) {
                let executable = self.directory.path().join("ssh");
                std::fs::write(&executable, format!("#!/bin/sh\n{script}\n")).unwrap();
                std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700))
                    .unwrap();
            }

            fn command(&self) -> tokio::process::Command {
                let mut command = tokio::process::Command::new(self.directory.path().join("ssh"));
                command.kill_on_drop(true);
                command
            }
        }

        impl Drop for SshEnvironment {
            fn drop(&mut self) {
                match &self.saved {
                    Some(value) => std::env::set_var("HOME", value),
                    None => std::env::remove_var("HOME"),
                }
            }
        }

        #[tokio::test]
        async fn resolves_configured_host_before_provider_normalization() {
            let fixture = SshEnvironment::new("printf 'hostname github.com\\n'");
            let options = SshOptions::default();
            assert_eq!(
                resolve_hostname("alias", &options).await.as_deref(),
                Some("github.com")
            );
            let remote = crate::repository_url::parse_remote("git@alias:Acme/Web.git").unwrap();
            assert_eq!(
                remote.resolve_id().await.as_deref(),
                Some("github.com/acme/web")
            );

            std::fs::write(fixture.config(), "Host alias\nHostName ssh.dev.azure.com\n").unwrap();
            let remote =
                crate::repository_url::parse_remote("ssh://git@alias/v3/Acme/Platform/Web.git")
                    .unwrap();
            assert_eq!(
                remote.resolve_id().await.as_deref(),
                Some("dev.azure.com/acme/platform/web")
            );
        }

        #[tokio::test]
        async fn missing_or_inaccessible_config_keeps_original_host() {
            let fixture = SshEnvironment::new("exit 99");
            std::fs::remove_file(fixture.config()).unwrap();
            assert_eq!(
                resolve_hostname("alias", &SshOptions::default()).await,
                None
            );
            symlink(fixture.config(), fixture.config()).unwrap();
            assert_eq!(
                resolve_hostname("alias", &SshOptions::default()).await,
                None
            );
        }

        #[tokio::test]
        async fn reports_command_and_output_failures() {
            let fixture = SshEnvironment::new("exit 1");
            let options = SshOptions::default();
            for (script, reason) in [
                ("exit 1", "ssh-config-failed"),
                ("printf '\\377'", "invalid-ssh-config-output"),
                ("printf 'host alias\\n'", "invalid-ssh-hostname"),
            ] {
                fixture.write_script(script);
                assert_eq!(evaluate_command(fixture.command()).await, Err(reason));
            }
            std::fs::write(fixture.config(), "InvalidOption yes\n").unwrap();
            assert_eq!(resolve_hostname("alias", &options).await, None);
            std::fs::remove_file(fixture.directory.path().join("ssh")).unwrap();
            assert_eq!(
                evaluate_command(fixture.command()).await,
                Err("ssh-unavailable")
            );
        }

        #[tokio::test]
        async fn slow_config_evaluation_times_out() {
            let fixture = SshEnvironment::new("exec /bin/sleep 30");
            let started = std::time::Instant::now();
            assert_eq!(
                evaluate_command(fixture.command()).await,
                Err("ssh-config-timeout")
            );
            assert!(started.elapsed() < Duration::from_secs(15));
        }
    }
}
