---
name: update-cli-version
description: Update the embedded CodeScene CLI to a new version, including downloading new binaries, computing SHA-256 checksums, and updating all pinned hashes in cli-checksums.sha256 and Dockerfile.
metadata:
  audience: contributors
  language: rust
---

## Purpose

Use this skill when upgrading the CodeScene CLI (`cs`) that is embedded in the MCP server binary and installed in the Docker image. The CLI is integrity-verified at build time, so upgrading requires updating multiple checksum files in lockstep.

## Files involved

| File | What to update | Why |
|---|---|---|
| `cli-checksums.sha256` | SHA-256 hashes for all 5 platform zip files | `build.rs` verifies the downloaded zip against these hashes at compile time |
| `Dockerfile` | `CS_CLI_INSTALLER_SHA256` ARG value | The Docker build verifies the installer script hash before executing it |
| `cli-checksums.sha256` line 1 | Version comment | Keeps the file self-documenting |

The download URL in `build.rs` uses `-latest.zip` (not a versioned URL), so the URL itself does not change — only the checksums change.

## Step-by-Step

### 1. Download all platform zips and compute checksums

Download each of the 5 platform variants and compute their SHA-256 hashes:

```bash
for variant in macos-aarch64 macos-amd64 linux-amd64 linux-aarch64 windows-amd64; do
  url="https://downloads.codescene.io/enterprise/cli/cs-${variant}-latest.zip"
  sha=$(curl --proto '=https' --tlsv1.2 -fsSL "$url" | shasum -a 256 | cut -d' ' -f1)
  echo "${sha}  cs-${variant}-latest.zip"
done
```

### 2. Get the new CLI version string

Extract the version from one of the downloaded binaries:

```bash
curl --proto '=https' --tlsv1.2 -fsSL \
  "https://downloads.codescene.io/enterprise/cli/cs-macos-aarch64-latest.zip" \
  -o /tmp/cs-cli-version-check.zip
unzip -o /tmp/cs-cli-version-check.zip -d /tmp/cs-cli-version-check
chmod +x /tmp/cs-cli-version-check/cs
/tmp/cs-cli-version-check/cs version
```

The first line of output looks like: `cs version 1.0.28 (commit-hash)`

### 3. Update `cli-checksums.sha256`

Replace the entire file contents with the new hashes and version comment. The format is:

```
# SHA-256 checksums for CodeScene CLI v<NEW_VERSION>
# Verify with: shasum -a 256 -c cli-checksums.sha256
# Update by downloading each variant and running: shasum -a 256 <file>
<hash>  cs-macos-aarch64-latest.zip
<hash>  cs-macos-amd64-latest.zip
<hash>  cs-linux-amd64-latest.zip
<hash>  cs-linux-aarch64-latest.zip
<hash>  cs-windows-amd64-latest.zip
```

### 4. Update the Dockerfile installer checksum

Compute the SHA-256 of the installer script:

```bash
curl --proto '=https' --tlsv1.2 -fsSL \
  https://downloads.codescene.io/enterprise/cli/install-cs-tool.sh \
  | shasum -a 256 | cut -d' ' -f1
```

Then update the `CS_CLI_INSTALLER_SHA256` ARG in `Dockerfile` (line 34):

```dockerfile
ARG CS_CLI_INSTALLER_SHA256="<new_hash>"
```

### 5. Clean build and verify

The build cache may contain the old zip. Clean and rebuild:

```bash
cargo clean
cargo build
```

The build will:
1. Download the CLI zip via curl with `--proto =https --tlsv1.2`
2. Compute its SHA-256
3. Compare against `cli-checksums.sha256`
4. Fail with a clear error if there's a mismatch

### 6. Run tests

```bash
cargo test
```

All existing tests must still pass — the CLI version upgrade should be transparent to the test suite.

## Troubleshooting

**Build fails with "SHA-256 checksum verification failed":**
The downloaded zip doesn't match `cli-checksums.sha256`. Either the checksums file has a typo, or the CDN is serving different content than what you hashed. Re-run step 1 and compare.

**Build fails with "No checksum entry found for '...'":**
A new platform variant was added to `build.rs` but not to `cli-checksums.sha256`. Add the missing line.

**Dockerfile build fails with "sha256sum: WARNING: 1 computed checksum did NOT match":**
The installer script changed. Re-run step 4 to get the new hash.

## Checklist

Before considering the upgrade complete:

- [ ] All 5 platform hashes updated in `cli-checksums.sha256`
- [ ] Version comment in `cli-checksums.sha256` updated to new CLI version
- [ ] `CS_CLI_INSTALLER_SHA256` in `Dockerfile` updated
- [ ] `cargo clean && cargo build` succeeds (verifies checksum logic end-to-end)
- [ ] `cargo test` passes
- [ ] New CLI version confirmed with `cargo run -- --cli-version`
