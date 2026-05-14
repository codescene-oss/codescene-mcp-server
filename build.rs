/// Build script for cs-mcp — embeds the git tag as CS_MCP_VERSION
/// and downloads the CS CLI binary for the current platform.
///
/// The git tag (e.g., "v1.0.0") is available as `env!("CS_MCP_VERSION")`.
/// The CS CLI zip is downloaded to `OUT_DIR/cs-cli.zip` and embedded at compile time.
///
/// Security: downloaded CLI zips are verified against SHA-256 checksums
/// committed in `cli-checksums.sha256`. Update that file when upgrading
/// the CLI version.

use std::collections::HashMap;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/tags");
    println!("cargo:rerun-if-changed=cli-checksums.sha256");
    println!("cargo:rerun-if-env-changed=CS_MCP_VERSION");
    println!("cargo:rerun-if-env-changed=CS_CLI_EMBED_PATH");

    embed_version();
    download_cli();
}

fn embed_version() {
    // Priority: CS_MCP_VERSION env var > git tag > Cargo.toml version
    let version = std::env::var("CS_MCP_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(git_tag_version)
        .unwrap_or_else(|| format!("v{}", env!("CARGO_PKG_VERSION")));
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
    if tag.is_empty() {
        None
    } else {
        Some(tag.to_string())
    }
}

fn download_cli() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest = format!("{out_dir}/cs-cli.zip");

    if maybe_embed_local_cli(&dest) {
        return;
    }

    // Skip re-download if already present and verified
    if std::path::Path::new(&dest).exists() {
        if verify_checksum(&dest) {
            return;
        }
        // Checksum mismatch on cached file — re-download
        eprintln!("Cached CLI zip failed checksum verification, re-downloading");
        std::fs::remove_file(&dest).ok();
    }

    let url = cli_download_url();
    eprintln!("Downloading CS CLI from {url}");

    let status = std::process::Command::new("curl")
        .args([
            "--proto", "=https",
            "--tlsv1.2",
            "-fsSL",
            "-o", &dest,
            &url,
        ])
        .status()
        .expect("failed to run curl");

    assert!(status.success(), "Failed to download CS CLI from {url}");

    assert!(
        verify_checksum(&dest),
        "SHA-256 checksum verification failed for downloaded CLI zip.\n\
         This could indicate a corrupted download or a supply-chain attack.\n\
         If you are intentionally upgrading the CLI, update cli-checksums.sha256."
    );
}

/// Verify the downloaded file against the committed SHA-256 checksums.
fn verify_checksum(file_path: &str) -> bool {
    let checksums = load_checksums();
    let zip_name = cli_zip_filename();

    let expected = match checksums.get(&zip_name) {
        Some(hash) => hash,
        None => {
            panic!(
                "No checksum entry found for '{zip_name}' in cli-checksums.sha256.\n\
                 Add a line: <sha256>  {zip_name}"
            );
        }
    };

    let actual = sha256_hex(file_path);
    if actual != *expected {
        eprintln!(
            "Checksum mismatch for {zip_name}:\n  expected: {expected}\n  actual:   {actual}"
        );
        return false;
    }

    true
}

fn sha256_hex(path: &str) -> String {
    use std::io::Read;
    let mut file = std::fs::File::open(path).expect("failed to open file for checksum");
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf).expect("failed to read file");
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    hasher.finalize_hex()
}

/// Minimal SHA-256 implementation for build script (no external crate dependencies).
struct Sha256 {
    state: [u32; 8],
    buffer: Vec<u8>,
    total_len: u64,
}

impl Sha256 {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c,
                0x1f83d9ab, 0x5be0cd19,
            ],
            buffer: Vec::new(),
            total_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.total_len += data.len() as u64;
        self.buffer.extend_from_slice(data);
        while self.buffer.len() >= 64 {
            let block: [u8; 64] = self.buffer[..64].try_into().unwrap();
            self.compress(&block);
            self.buffer.drain(..64);
        }
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(Self::K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }

    fn finalize_hex(mut self) -> String {
        let bit_len = self.total_len * 8;
        self.buffer.push(0x80);
        while (self.buffer.len() % 64) != 56 {
            self.buffer.push(0);
        }
        self.buffer.extend_from_slice(&bit_len.to_be_bytes());

        let padded = std::mem::take(&mut self.buffer);
        for chunk in padded.chunks(64) {
            let block: [u8; 64] = chunk.try_into().unwrap();
            self.compress(&block);
        }

        self.state
            .iter()
            .map(|v| format!("{v:08x}"))
            .collect::<String>()
    }
}

/// Parse cli-checksums.sha256 into a map of filename -> hash.
fn load_checksums() -> HashMap<String, String> {
    let content = std::fs::read_to_string("cli-checksums.sha256")
        .expect("Failed to read cli-checksums.sha256 — is it committed in the repo root?");

    content
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next().expect("malformed checksum line: missing hash");
            let filename = parts
                .next()
                .expect("malformed checksum line: missing filename");
            (filename.to_string(), hash.to_string())
        })
        .collect()
}

fn maybe_embed_local_cli(dest_zip: &str) -> bool {
    let local_cli = match std::env::var("CS_CLI_EMBED_PATH") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => return false,
    };

    let local_cli = local_cli.trim();
    let local_path = std::path::Path::new(local_cli);
    assert!(
        local_path.exists(),
        "CS_CLI_EMBED_PATH does not exist: {local_cli}"
    );
    assert!(
        local_path.is_file(),
        "CS_CLI_EMBED_PATH is not a file: {local_cli}"
    );

    // Ensure Cargo rebuilds when the local CLI changes.
    println!("cargo:rerun-if-changed={}", local_path.display());

    // Overwrite destination zip with a one-file archive containing `cs`.
    let status = std::process::Command::new("zip")
        .args(["-j", "-q", "-FS", dest_zip, local_cli])
        .status()
        .expect("failed to run zip");

    assert!(
        status.success(),
        "Failed to create embedded CLI zip from {local_cli}"
    );
    true
}

fn cli_download_url() -> String {
    let (os_part, arch_part) = cli_platform_parts();
    format!("https://downloads.codescene.io/enterprise/cli/cs-{os_part}-{arch_part}-latest.zip")
}

fn cli_zip_filename() -> String {
    let (os_part, arch_part) = cli_platform_parts();
    format!("cs-{os_part}-{arch_part}-latest.zip")
}

fn cli_platform_parts() -> (&'static str, &'static str) {
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

    (os_part, arch_part)
}
