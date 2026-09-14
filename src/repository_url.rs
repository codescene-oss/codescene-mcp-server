use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

// CodeScene's repository IDs preserve the same characters as ring.util.codec/url-encode.
const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'+');

struct RemoteParts {
    host: String,
    path: Vec<String>,
}

struct RepositoryIdentity {
    host: String,
    owner: Vec<String>,
    repository: String,
    provider: Provider,
}

#[derive(Clone, Copy)]
enum Provider {
    Azure,
    Bitbucket,
    GitHub,
    GitLab,
    Other,
}

enum AzurePath {
    Ssh,
    Http,
}

impl RemoteParts {
    fn repository_identity(self) -> Option<RepositoryIdentity> {
        let host = self.host.trim_end_matches('.').to_string();
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

        self.generic_identity(host)
    }

    fn is_azure_ssh(&self) -> bool {
        self.host.eq_ignore_ascii_case("ssh.dev.azure.com")
            || self.host.eq_ignore_ascii_case("vs-ssh.visualstudio.com")
    }

    fn canonicalize_azure(&self, format: AzurePath) -> Option<RepositoryIdentity> {
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

    fn canonicalize_visual_studio(&self, organization: &str) -> Option<RepositoryIdentity> {
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

    fn generic_identity(mut self, host: String) -> Option<RepositoryIdentity> {
        if self
            .path
            .first()
            .is_some_and(|part| part.eq_ignore_ascii_case("scm") || part.eq_ignore_ascii_case("a"))
        {
            self.path.remove(0);
        }
        let repository = self.path.pop()?;
        Some(RepositoryIdentity {
            provider: provider_for_host(&host),
            host,
            owner: self.path,
            repository,
        })
    }
}

impl RepositoryIdentity {
    fn canonical_id(self) -> Option<String> {
        let path = self
            .owner
            .iter()
            .chain(std::iter::once(&self.repository))
            .map(|part| canonical_path_segment(part))
            .collect::<Option<Vec<_>>>()?
            .join("/");
        let id = format!("{}/{path}", self.host).to_lowercase();
        Some(self.provider.normalize_id(&id).to_string())
    }
}

fn canonical_path_segment(value: &str) -> Option<String> {
    let decoded = percent_decode_str(value).decode_utf8().ok()?;
    Some(utf8_percent_encode(&decoded, PATH_SEGMENT_ENCODE_SET).to_string())
}

impl Provider {
    fn normalize_id<'a>(self, id: &'a str) -> &'a str {
        match self {
            Self::Azure | Self::Bitbucket | Self::GitHub | Self::GitLab | Self::Other => {
                id.strip_suffix(".git").unwrap_or(id)
            }
        }
    }
}

pub(crate) fn canonical_repository_id(remote: &str) -> Option<String> {
    let remote = remote.trim();
    if remote.is_empty() || is_filesystem_remote(remote.as_bytes()) {
        return None;
    }

    let parts = if remote.contains("://") {
        parse_url(remote)?
    } else {
        parse_scp_like(remote)?
    };
    parts.repository_identity()?.canonical_id()
}

fn is_filesystem_remote(remote: &[u8]) -> bool {
    let is_file_url = remote
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"file://"));
    let has_windows_drive =
        remote.len() >= 2 && remote[0].is_ascii_alphabetic() && remote[1] == b':';
    let is_slash_delimited_scp = remote
        .iter()
        .position(|byte| *byte == b'/')
        .is_some_and(|slash| remote[..slash].contains(&b'@') && remote.get(slash + 1).is_some());

    is_file_url
        || has_windows_drive
        || remote.starts_with(b"/")
        || remote.starts_with(b"\\")
        || (!remote.contains(&b':') && !is_slash_delimited_scp)
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

fn provider_for_host(host: &str) -> Provider {
    if host.eq_ignore_ascii_case("github.com") {
        Provider::GitHub
    } else if host.eq_ignore_ascii_case("gitlab.com") {
        Provider::GitLab
    } else if host.eq_ignore_ascii_case("bitbucket.org") {
        Provider::Bitbucket
    } else {
        Provider::Other
    }
}

fn azure_identity(path: [&str; 3]) -> Option<RepositoryIdentity> {
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
    let [organization, project, repository]: [String; 3] = path.try_into().ok()?;
    Some(RepositoryIdentity {
        host: "dev.azure.com".to_string(),
        owner: vec![organization, project],
        repository,
        provider: Provider::Azure,
    })
}

#[cfg(test)]
mod tests {
    use super::canonical_repository_id;

    fn assert_canonical(remote: &str, expected: &str) {
        assert_eq!(canonical_repository_id(remote).as_deref(), Some(expected));
    }

    #[test]
    fn canonicalizes_generic_protocol_and_scp_remotes() {
        assert_eq!(
            canonical_repository_id("git@GitHub.com:Acme/Platform/Web.git"),
            Some("github.com/acme/platform/web".to_string())
        );
        assert_eq!(
            canonical_repository_id("ssh://git@github.com:2222/Acme/Platform/Web.GIT"),
            Some("github.com/acme/platform/web.git".to_string())
        );
    }

    #[test]
    fn encodes_owner_and_repository_as_separate_path_values() {
        assert_eq!(
            canonical_repository_id("https://gitlab.com/Acme/Core Platform/Web App.git"),
            Some("gitlab.com/acme/core%20platform/web%20app".to_string())
        );
        assert_canonical(
            "git@example.com:Team+Tools/Repo@Home.git",
            "example.com/team+tools/repo%40home",
        );
    }

    #[test]
    fn excludes_http_credentials_from_identity() {
        assert_canonical(
            "https://user:secret@example.com/Owner/Repo.git",
            "example.com/owner/repo",
        );
        assert_canonical(
            "https://token@example.com/Owner/Repo",
            "example.com/owner/repo",
        );
    }

    #[test]
    fn canonicalizes_azure_and_visual_studio_remotes() {
        assert_eq!(
            canonical_repository_id("git@ssh.dev.azure.com:v3/Acme/Core%20Platform/Web%20App.git"),
            Some("dev.azure.com/acme/core%20platform/web%20app.git".to_string())
        );
        assert_eq!(
            canonical_repository_id("https://dev.azure.com/Acme/Core%20Platform/_git/Web%20App"),
            Some("dev.azure.com/acme/core%20platform/web%20app".to_string())
        );
        assert_eq!(
            canonical_repository_id("https://Acme.visualstudio.com/Core%20Platform/_git/Web%20App"),
            Some("dev.azure.com/acme/core%20platform/web%20app".to_string())
        );
    }

    #[test]
    fn canonicalizes_azure_on_prem_and_nested_visual_studio_paths() {
        assert_eq!(
            canonical_repository_id(
                "ssh://server.local:22/tfs/DefaultCollection/Project/_git/Repo"
            ),
            Some("server.local/defaultcollection/project/repo".to_string())
        );
        assert_eq!(
            canonical_repository_id(
                "https://Org.visualstudio.com/DefaultCollection/Project/_git/Repo"
            ),
            Some("dev.azure.com/org/defaultcollection/project/repo".to_string())
        );
    }

    #[test]
    fn falls_back_to_generic_parsing_for_non_azure_paths() {
        assert_eq!(
            canonical_repository_id("https://dev.azure.com/Acme/Web.git"),
            Some("dev.azure.com/acme/web".to_string())
        );
        assert_eq!(
            canonical_repository_id("https://Org.visualstudio.com/Acme/Web.git"),
            Some("org.visualstudio.com/acme/web".to_string())
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
    fn preserves_provider_paths_that_resemble_special_prefixes() {
        assert_canonical("https://github.com/a/Repo.git", "github.com/a/repo");
        assert_canonical("https://example.com/scm/Repo.git", "example.com/scm/repo");
    }

    #[test]
    fn accepts_slash_delimited_scp_like_remotes() {
        assert_eq!(
            canonical_repository_id("git@git.example.com/Team/Repo.git"),
            Some("git.example.com/team/repo".to_string())
        );
    }

    #[test]
    fn rejects_blank_and_unparseable_remotes() {
        assert_eq!(canonical_repository_id("  "), None);
        assert_eq!(canonical_repository_id("not a repository URL"), None);
        assert_eq!(canonical_repository_id("file:///tmp/repository.git"), None);
    }

    #[test]
    fn rejects_filesystem_remotes() {
        for remote in [
            "repository.git",
            "nested/repository.git",
            "./repository.git",
            "../repository.git",
            "/tmp/repository.git",
            r"C:\work\repository.git",
            "C:/work/repository.git",
            r"\\server\share\repository.git",
            "//server/share/repository.git",
            "file:///tmp/repository.git",
            "FILE://server/share/repository.git",
        ] {
            assert_eq!(canonical_repository_id(remote), None);
        }
    }

    #[test]
    fn retains_network_loopback_remotes() {
        assert_canonical("http://localhost/Owner/Repo.git", "localhost/owner/repo");
        assert_canonical(
            "ssh://git@127.0.0.1:2222/Owner/Repo.git",
            "127.0.0.1/owner/repo",
        );
        assert_canonical("git@localhost:Owner/Repo.git", "localhost/owner/repo");
    }
}
