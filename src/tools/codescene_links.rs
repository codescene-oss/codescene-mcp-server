/// Constructs user-facing CodeScene URLs for technical debt pages.
///
/// On-prem and cloud use different path structures:
///   On-prem: {web_root}/{project_id}/analyses/{analysis_id}/...
///   Cloud:   https://codescene.io/projects/{project_id}/jobs/{analysis_id}/results/...
///
/// The `web_root` parameter is `None` for cloud, or `Some("https://host")` for on-prem.
/// Callers obtain it from `auth::resolve_web_root(credential)`.

/// Returns the base path for an analysis page (without trailing slash).
///
/// On-prem: `{web_root}/{project_id}/analyses/{analysis_id}`
/// Cloud:   `https://codescene.io/projects/{project_id}/jobs/{analysis_id}/results`
fn analysis_base(project_id: i64, analysis_id: i64, web_root: Option<&str>) -> String {
    match web_root.map(|base| base.trim_end_matches('/').to_string()) {
        Some(base) => format!("{base}/{project_id}/analyses/{analysis_id}"),
        None => format!("https://codescene.io/projects/{project_id}/jobs/{analysis_id}/results"),
    }
}

/// Builds a link to the technical debt hotspots system map page.
pub fn hotspots_link(project_id: i64, analysis_id: i64, web_root: Option<&str>) -> String {
    let base = analysis_base(project_id, analysis_id, web_root);
    format!(
        "{base}/code/technical-debt/system-map\
         ?max-code-health=10.00\
         &min-change-freq=0\
         &showHotspotsOnly=true\
         &min-coverage=0.00\
         &max-coverage=100.00\
         #hotspots"
    )
}

/// Builds a link to the Code Biomarkers page.
///
/// When `file_name` is provided the link includes a `?name=` query parameter
/// with the file name percent-encoded.
pub fn biomarkers_link(
    project_id: i64,
    analysis_id: i64,
    file_name: Option<&str>,
    web_root: Option<&str>,
) -> String {
    let base = analysis_base(project_id, analysis_id, web_root);
    let path = format!("{base}/code/biomarkers");
    match file_name {
        Some(name) => {
            let encoded = percent_encode_path(name);
            format!("{path}?name={encoded}")
        }
        None => path,
    }
}

/// Minimal percent-encoding for path components in query parameters.
/// Encodes characters that are not unreserved per RFC 3986, except `/`
/// which CodeScene expects as-is in file paths.
fn percent_encode_path(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char)
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{byte:02X}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── analysis_base ───────────────────────────────────────

    #[test]
    fn analysis_base_cloud() {
        assert_eq!(
            analysis_base(72308, 6006312, None),
            "https://codescene.io/projects/72308/jobs/6006312/results"
        );
    }

    #[test]
    fn analysis_base_onprem() {
        assert_eq!(
            analysis_base(147, 37888, Some("https://test-env.enterprise.codescene.io")),
            "https://test-env.enterprise.codescene.io/147/analyses/37888"
        );
    }

    #[test]
    fn analysis_base_trims_trailing_slash() {
        assert_eq!(
            analysis_base(147, 37888, Some("https://my-instance.com/")),
            "https://my-instance.com/147/analyses/37888"
        );
    }

    // ── hotspots_link ───────────────────────────────────────

    #[test]
    fn hotspots_link_cloud() {
        let link = hotspots_link(72308, 6006312, None);
        assert!(link.starts_with(
            "https://codescene.io/projects/72308/jobs/6006312/results/code/technical-debt/system-map?"
        ));
        assert!(link.contains("max-code-health=10.00"));
        assert!(link.contains("showHotspotsOnly=true"));
        assert!(link.ends_with("#hotspots"));
    }

    #[test]
    fn hotspots_link_onprem() {
        let link = hotspots_link(147, 37888, Some("https://test-env.enterprise.codescene.io"));
        assert!(link.starts_with(
            "https://test-env.enterprise.codescene.io/147/analyses/37888/code/technical-debt/system-map?"
        ));
        assert!(link.ends_with("#hotspots"));
    }

    // ── biomarkers_link ─────────────────────────────────────

    #[test]
    fn biomarkers_link_cloud_no_file() {
        let link = biomarkers_link(72308, 6006312, None, None);
        assert_eq!(
            link,
            "https://codescene.io/projects/72308/jobs/6006312/results/code/biomarkers"
        );
    }

    #[test]
    fn biomarkers_link_cloud_with_file() {
        let link = biomarkers_link(
            72308,
            6006312,
            Some("code-coverage-examples-single-component/src/main/java/com/codescene/ConditionalExample.java"),
            None,
        );
        assert_eq!(
            link,
            "https://codescene.io/projects/72308/jobs/6006312/results/code/biomarkers\
             ?name=code-coverage-examples-single-component/src/main/java/com/codescene/ConditionalExample.java"
        );
    }

    #[test]
    fn biomarkers_link_onprem() {
        let base = Some("https://test-env.enterprise.codescene.io");

        // Without file name
        assert_eq!(
            biomarkers_link(147, 37888, None, base),
            "https://test-env.enterprise.codescene.io/147/analyses/37888/code/biomarkers"
        );

        // With file name
        assert_eq!(
            biomarkers_link(147, 37888, Some("seata/mmil-test.java"), base),
            "https://test-env.enterprise.codescene.io/147/analyses/37888/code/biomarkers\
             ?name=seata/mmil-test.java"
        );
    }

    // ── percent_encode_path ─────────────────────────────────

    #[test]
    fn percent_encode_path_preserves_slashes() {
        assert_eq!(percent_encode_path("a/b/c.java"), "a/b/c.java");
    }

    #[test]
    fn percent_encode_path_encodes_spaces() {
        assert_eq!(percent_encode_path("my file.java"), "my%20file.java");
    }

    #[test]
    fn percent_encode_path_encodes_special_chars() {
        assert_eq!(percent_encode_path("a+b=c"), "a%2Bb%3Dc");
    }
}
