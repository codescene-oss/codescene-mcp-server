use percent_encoding::percent_decode_str;

struct RemoteParts {
    host: String,
    path: Vec<String>,
}

enum AzurePath {
    Ssh,
    Http,
}

impl RemoteParts {
    fn canonicalize(self) -> Option<(String, Vec<String>)> {
        let host = self.host.trim_end_matches('.');
        if host.is_empty() {
            return None;
        }

        if self.is_azure_ssh() {
            return self.canonicalize_azure(AzurePath::Ssh);
        }
        if host.eq_ignore_ascii_case("dev.azure.com") {
            return self.canonicalize_azure(AzurePath::Http);
        }
        if let Some(organization) = self.visual_studio_organization() {
            return self.canonicalize_visual_studio(organization);
        }

        let host = host.to_string();
        let path = self.generic_path()?;
        Some((host, path))
    }

    fn is_azure_ssh(&self) -> bool {
        self.host.eq_ignore_ascii_case("ssh.dev.azure.com")
            || self.host.eq_ignore_ascii_case("vs-ssh.visualstudio.com")
    }

    fn canonicalize_azure(&self, format: AzurePath) -> Option<(String, Vec<String>)> {
        let (marker_index, marker, identity_indices) = match format {
            AzurePath::Ssh => (0, "v3", [1, 2, 3]),
            AzurePath::Http => (2, "_git", [0, 1, 3]),
        };
        if self.path.len() != 4 || !self.path[marker_index].eq_ignore_ascii_case(marker) {
            return None;
        }
        azure_identity(identity_indices.map(|index| self.path[index].as_str()))
    }

    fn visual_studio_organization(&self) -> Option<&str> {
        let suffix = ".visualstudio.com";
        if self.host.len() <= suffix.len()
            || !self.host[self.host.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
        {
            return None;
        }
        Some(&self.host[..self.host.len() - suffix.len()])
    }

    fn canonicalize_visual_studio(&self, organization: &str) -> Option<(String, Vec<String>)> {
        let path = if self.path.first()?.eq_ignore_ascii_case("DefaultCollection") {
            &self.path[1..]
        } else {
            &self.path
        };
        if path.len() != 3 || !path[1].eq_ignore_ascii_case("_git") {
            return None;
        }
        azure_identity([organization, path[0].as_str(), path[2].as_str()])
    }

    fn generic_path(mut self) -> Option<Vec<String>> {
        if self
            .path
            .first()
            .is_some_and(|part| part.eq_ignore_ascii_case("scm") || part.eq_ignore_ascii_case("a"))
        {
            self.path.remove(0);
        }
        (!self.path.is_empty()).then_some(self.path)
    }
}

pub(crate) fn canonical_repository_id(remote: &str) -> Option<String> {
    let remote = remote.trim();
    if remote.is_empty() {
        return None;
    }

    let parts = if remote.contains("://") {
        parse_url(remote)?
    } else {
        parse_scp_like(remote)?
    };
    let (host, path) = parts.canonicalize()?;
    let id = format!("{host}/{}", path.join("/")).to_lowercase();
    Some(id.strip_suffix(".git").unwrap_or(&id).to_string())
}

fn parse_url(remote: &str) -> Option<RemoteParts> {
    let url = reqwest::Url::parse(remote).ok()?;
    if url.scheme() == "file" {
        return None;
    }

    Some(RemoteParts {
        host: url.host_str()?.to_string(),
        path: url
            .path_segments()?
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect(),
    })
}

fn parse_scp_like(remote: &str) -> Option<RemoteParts> {
    let (authority, path) = remote.rsplit_once(':')?;
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    if host.is_empty() || host.contains(['/', '\\']) {
        return None;
    }
    let host = reqwest::Url::parse(&format!("ssh://{host}/"))
        .ok()?
        .host_str()?
        .to_string();

    let path = path
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if path.is_empty() {
        return None;
    }
    Some(RemoteParts { host, path })
}

fn azure_identity(path: [&str; 3]) -> Option<(String, Vec<String>)> {
    let path = path
        .into_iter()
        .map(|part| {
            percent_decode_str(part)
                .decode_utf8()
                .ok()
                .map(|value| value.into_owned())
        })
        .collect::<Option<Vec<_>>>()?;
    if path.iter().any(String::is_empty) {
        return None;
    }
    Some(("dev.azure.com".to_string(), path))
}

#[cfg(test)]
mod tests {
    use super::canonical_repository_id;

    #[test]
    fn canonicalizes_generic_protocol_and_scp_remotes() {
        assert_eq!(
            canonical_repository_id("git@GitHub.com:Acme/Platform/Web.git"),
            Some("github.com/acme/platform/web".to_string())
        );
        assert_eq!(
            canonical_repository_id("ssh://git@github.com:2222/Acme/Platform/Web.GIT"),
            Some("github.com/acme/platform/web".to_string())
        );
    }

    #[test]
    fn excludes_http_credentials_from_identity() {
        assert_eq!(
            canonical_repository_id("https://user:secret@example.com/Owner/Repo.git"),
            Some("example.com/owner/repo".to_string())
        );
        assert_eq!(
            canonical_repository_id("https://token@example.com/Owner/Repo"),
            Some("example.com/owner/repo".to_string())
        );
    }

    #[test]
    fn canonicalizes_azure_and_visual_studio_remotes() {
        let expected = Some("dev.azure.com/acme/core platform/web app".to_string());
        assert_eq!(
            canonical_repository_id("git@ssh.dev.azure.com:v3/Acme/Core%20Platform/Web%20App.git"),
            expected
        );
        assert_eq!(
            canonical_repository_id("https://dev.azure.com/Acme/Core%20Platform/_git/Web%20App"),
            expected
        );
        assert_eq!(
            canonical_repository_id("https://Acme.visualstudio.com/Core%20Platform/_git/Web%20App"),
            expected
        );
    }

    #[test]
    fn canonicalizes_bitbucket_server_http_and_ssh_remotes() {
        let expected = Some("git.example.com/platform/web".to_string());
        assert_eq!(
            canonical_repository_id("https://git.example.com/scm/PLATFORM/Web.git"),
            expected
        );
        assert_eq!(
            canonical_repository_id("ssh://git@git.example.com:7999/PLATFORM/Web.git"),
            expected
        );
    }

    #[test]
    fn removes_gerrit_authenticated_prefix() {
        assert_eq!(
            canonical_repository_id("https://review.example.com/a/Team/Service.git"),
            Some("review.example.com/team/service".to_string())
        );
    }

    #[test]
    fn rejects_blank_and_unparseable_remotes() {
        assert_eq!(canonical_repository_id("  "), None);
        assert_eq!(canonical_repository_id("not a repository URL"), None);
        assert_eq!(canonical_repository_id("file:///tmp/repository.git"), None);
    }
}
