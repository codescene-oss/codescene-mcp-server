---
name: update-cli-version
description: Update the embedded CodeScene CLI to a new version, including downloading new binaries, computing SHA-256 checksums, and updating all pinned hashes in cli-checksums.sha256 and Dockerfile.
metadata:
  audience: contributors
  language: rust
---

## Purpose

Use this skill when upgrading the CodeScene CLI (`cs`) that is embedded in the MCP server binary and installed in the Docker image. The CLI is integrity-verified at build time, so upgrading requires updating the pinned version, checksums, and Dockerfile in lockstep.

## Files involved

| File | What to update | Why |
|---|---|---|
| `build.rs` | `CLI_VERSION` constant (commit hash) | Pins the exact CLI version downloaded at build time |
| `cli-checksums.sha256` | SHA-256 hashes for all 5 platform zip files | `build.rs` verifies the downloaded zip against these hashes at compile time |
| `Dockerfile` | `CS_CLI_INSTALLER_SHA256` ARG value | The Docker build verifies the installer script hash before executing it |
| `cli-checksums.sha256` line 1 | Version comment | Keeps the file self-documenting |

The download URL in `build.rs` uses the commit hash from `CLI_VERSION` to pin an exact CLI release:
`https://downloads.codescene.io/enterprise/cli/cs-{os}-{arch}-{CLI_VERSION}.zip`

## Step-by-Step

### 1. Get the new CLI version and commit hash

Download the latest CLI and extract both the version string and commit hash:

```bash
curl --proto '=https' --tlsv1.2 -fsSL \
  "https://downloads.codescene.io/enterprise/cli/cs-macos-aarch64-latest.zip" \
  -o /tmp/cs-cli-version-check.zip
unzip -o /tmp/cs-cli-version-check.zip -d /tmp/cs-cli-version-check
chmod +x /tmp/cs-cli-version-check/cs
/tmp/cs-cli-version-check/cs version
```

The output looks like: `cs version 1.0.29 (379b46808f26ed3c4eaefc2e11bee6c55dc44000)`

The commit hash in parentheses (e.g., `379b46808f26ed3c4eaefc2e11bee6c55dc44000`) is used as the `CLI_VERSION` in `build.rs` and in download URLs/checksum filenames.

### 2. Download all platform zips using the commit hash and compute checksums

Download each of the 5 platform variants using the **commit hash** (not `-latest`) and compute their SHA-256 hashes:

```bash
COMMIT_HASH="<commit_hash_from_step_1>"
for variant in macos-aarch64 macos-amd64 linux-amd64 linux-aarch64 windows-amd64; do
  url="https://downloads.codescene.io/enterprise/cli/cs-${variant}-${COMMIT_HASH}.zip"
  sha=$(curl --proto '=https' --tlsv1.2 -fsSL "$url" | shasum -a 256 | cut -d' ' -f1)
  echo "${sha}  cs-${variant}-${COMMIT_HASH}.zip"
done
```

### 3. Update `CLI_VERSION` in `build.rs`

Update the `CLI_VERSION` constant at the top of the `cli_download_url` / `cli_zip_filename` section:

```rust
const CLI_VERSION: &str = "<commit_hash_from_step_1>";
```

### 4. Update `cli-checksums.sha256`

Replace the entire file contents with the new hashes and version comment. The format is:

```
# SHA-256 checksums for CodeScene CLI v<NEW_VERSION> (<COMMIT_HASH>)
# Verify with: shasum -a 256 -c cli-checksums.sha256
# Update by downloading each variant and running: shasum -a 256 <file>
<hash>  cs-macos-aarch64-<COMMIT_HASH>.zip
<hash>  cs-macos-amd64-<COMMIT_HASH>.zip
<hash>  cs-linux-amd64-<COMMIT_HASH>.zip
<hash>  cs-linux-aarch64-<COMMIT_HASH>.zip
<hash>  cs-windows-amd64-<COMMIT_HASH>.zip
```

### 5. Update the Dockerfile installer checksum

Compute the SHA-256 of the installer script:

```bash
curl --proto '=https' --tlsv1.2 -fsSL \
  https://downloads.codescene.io/enterprise/cli/install-cs-tool.sh \
  | shasum -a 256 | cut -d' ' -f1
```

Then update the `CS_CLI_INSTALLER_SHA256` ARG in `Dockerfile`:

```dockerfile
ARG CS_CLI_INSTALLER_SHA256="<new_hash>"
```

### 6. Clean build and verify

The build cache may contain the old zip. Clean and rebuild:

```bash
cargo clean
cargo build
```

The build will:
1. Download the CLI zip via curl with `--proto =https --tlsv1.2` using the pinned commit hash URL
2. Compute its SHA-256
3. Compare against `cli-checksums.sha256`
4. Fail with a clear error if there's a mismatch

### 7. Run tests

```bash
cargo test
```

All existing tests must still pass — the CLI version upgrade should be transparent to the test suite.

## Troubleshooting

**Build fails with "SHA-256 checksum verification failed":**
The downloaded zip doesn't match `cli-checksums.sha256`. Either the checksums file has a typo, or the CDN is serving different content than what you hashed. Re-run step 2 and compare.

**Build fails with "No checksum entry found for '...'":**
The `CLI_VERSION` in `build.rs` doesn't match the commit hash used in `cli-checksums.sha256` filenames. Ensure they are identical.

**Dockerfile build fails with "sha256sum: WARNING: 1 computed checksum did NOT match":**
The installer script changed. Re-run step 5 to get the new hash.

## Checklist

Before considering the upgrade complete:

- [ ] `CLI_VERSION` in `build.rs` updated to new commit hash
- [ ] All 5 platform hashes updated in `cli-checksums.sha256` with commit hash filenames
- [ ] Version comment in `cli-checksums.sha256` updated to new CLI version and commit hash
- [ ] `CS_CLI_INSTALLER_SHA256` in `Dockerfile` updated (if changed)
- [ ] `cargo clean && cargo build` succeeds (verifies checksum logic end-to-end)
- [ ] `cargo test` passes
- [ ] New CLI version confirmed with `cargo run -- --cli-version`
