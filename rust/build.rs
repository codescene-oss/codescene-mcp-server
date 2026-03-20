/// Build script for cs-mcp — embeds the git tag as CS_MCP_VERSION
/// and downloads the CS CLI binary for the current platform.
///
/// The git tag (e.g., "v1.0.0") is available as `env!("CS_MCP_VERSION")`.
/// The CS CLI zip is downloaded to `OUT_DIR/cs-cli.zip` and embedded at compile time.

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/tags");

    embed_version();
    download_cli();
}

fn embed_version() {
    let version =
        git_tag_version().unwrap_or_else(|| format!("v{}", env!("CARGO_PKG_VERSION")));
    println!("cargo:rustc-env=CS_MCP_VERSION={version}");
}

fn git_tag_version() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let tag = String::from_utf8(output.stdout).ok()?;
    let tag = tag.trim();
    if tag.is_empty() { None } else { Some(tag.to_string()) }
}

fn download_cli() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest = format!("{out_dir}/cs-cli.zip");

    // Skip re-download if already present (cargo caches OUT_DIR across rebuilds)
    if std::path::Path::new(&dest).exists() {
        return;
    }

    let url = cli_download_url();
    eprintln!("Downloading CS CLI from {url}");

    let status = std::process::Command::new("curl")
        .args(["-fsSL", "-o", &dest, &url])
        .status()
        .expect("failed to run curl");

    assert!(status.success(), "Failed to download CS CLI from {url}");
}

fn cli_download_url() -> String {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    let os_part = match target_os.as_str() {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        other => panic!("Unsupported target OS: {other}"),
    };

    let arch_part = match target_arch.as_str() {
        "x86_64" => "amd64",
        "aarch64" => "aarch64",
        other => panic!("Unsupported target arch: {other}"),
    };

    format!(
        "https://downloads.codescene.io/enterprise/cli/cs-{os_part}-{arch_part}-latest.zip"
    )
}
