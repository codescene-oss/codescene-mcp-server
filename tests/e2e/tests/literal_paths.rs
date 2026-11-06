//! Single-file reviews must treat glob metacharacters literally, including
//! Next.js dynamic routes, even when a glob would select a different file.

use super::*;

const TIMEOUT: Duration = Duration::from_secs(60);
const SOURCE: &str = "export function add(a: number, b: number) {\n  return a + b;\n}\n";
const FILE_PATHS: &[&str] = &[
    "app/plain/page.ts",
    "app/[locale]/page.ts",
    "app/[locale]/[slug]/page.ts",
    "app/[...slug]/page.ts",
    "app/[[...slug]]/page.ts",
    "app/left[/page.ts",
    "app/right]/page.ts",
    "app/{draft,live}/page.ts",
    "app/with spaces/page.ts",
    #[cfg(unix)]
    "app/star*/page.ts",
    #[cfg(unix)]
    "app/question?/page.ts",
];

fn write_fixtures(repo_dir: &Path) {
    for path in FILE_PATHS {
        let full_path = repo_dir.join(path);
        std::fs::create_dir_all(full_path.parent().unwrap()).expect("create route directory");
        std::fs::write(full_path, SOURCE).expect("write route fixture");
    }
    for path in ["app/l/page.ts", "app/draft/page.ts", "app/live/page.ts"] {
        let full_path = repo_dir.join(path);
        std::fs::create_dir_all(full_path.parent().unwrap()).expect("create decoy directory");
        std::fs::write(full_path, "").expect("write unscorable glob decoy");
    }
}

fn assert_literal_review(client: &mut MCPClient, tool: &str, file_path: &Path) {
    let response = client
        .call_tool(
            tool,
            json!({"file_path": file_path.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("tool call should complete");
    let text = extract_result_text(&response);
    assert!(
        response.get("error").is_none() && response["result"]["isError"] != true,
        "{tool} failed for {}: {response}",
        file_path.display()
    );
    if tool == "code_health_score" {
        assert_eq!(
            extract_code_health_score(&text),
            Some(10.0),
            "expected the literal file's score for {}: {text}",
            file_path.display()
        );
    } else {
        let review: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("expected review JSON: {error}: {text}"));
        assert!(review.is_object(), "expected one review, got: {text}");
        assert_eq!(review["score"], 10.0, "unexpected review: {text}");
        assert_eq!(review["review"], json!([]), "unexpected findings: {text}");
    }
}

fn check_literal_paths(tool: &str) {
    let (command, env, repo_dir, _tmp) = setup();
    write_fixtures(&repo_dir);
    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "server should start");
    client.initialize().expect("initialize should succeed");

    for path in FILE_PATHS {
        assert_literal_review(&mut client, tool, &repo_dir.join(path));
        if !is_docker() {
            assert_literal_review(&mut client, tool, Path::new(path));
        }
    }
}

pub fn test_literal_paths_code_health_score() {
    check_literal_paths("code_health_score");
}

pub fn test_literal_paths_code_health_review() {
    check_literal_paths("code_health_review");
}
