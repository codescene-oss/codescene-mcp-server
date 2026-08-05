---
name: making-a-release
description: Use when a developer wants to cut, publish, or ship a new release of the CodeScene MCP Server, or asks how to make a release / bump the version.
---

# Making a Release

## Overview

Use this skill to cut a new release of the CodeScene MCP Server. The release
process is almost entirely automated by GitHub Actions: your job is to pick the
next version and start the pipeline. Everything else — building signed binaries
for every platform, publishing the Docker image, npm package and MCPB bundle,
generating the changelog, updating packaging metadata, and promoting the
release to "latest" — happens on CI with no pull requests to merge.

## When to Use

- The user asks to make, cut, publish, or ship a new release.
- The user wants to bump the version and release it.
- The user asks how the release process works.

Do not use this skill to publish the VS Code extension on its own (that is a
separate manual `workflow_dispatch` on `Publish VS Code Extension`).

## How the Pipeline Works

The developer only performs step 1. CI does the rest.

1. **Start the release.** Run the `Create Release` workflow with the new
   version (e.g. `1.4.3`). It validates the version, creates and pushes the
   `MCP-<version>` tag, and creates the GitHub Release with **deterministic,
   auto-generated release notes** (GitHub's `generate-notes` engine, configured
   by `.github/release.yml`, which categorizes merged PRs by label). The release
   is created but **not** yet marked latest.
2. **Build.** The tag push triggers `Build and publish executables`
   (binaries, signing, MCPB bundle → uploaded to the release) and
   `Build and Publish Release` (Docker image `:tag` and `:latest`).
3. **Publish + update metadata.** On a successful build, `Publish npm Package`
   publishes to npm, and `Update packaging metadata` updates the Homebrew
   formula and Claude Code plugin, committing **directly to `main` — no PRs**.
4. **Promote to latest.** When `Update packaging metadata` finishes
   successfully, `Promote release to latest` marks the GitHub Release as latest.
   This is what signals users they are on an outdated version.

## Implementation

1. Confirm the current version and decide the next one. The version lives in
   `manifest.json` and `claude-code-plugin/.claude-plugin/plugin.json`; the most
   recent release tag is `MCP-<version>`. Follow semver.
2. Make sure `main` is in the desired state to release (all intended PRs merged,
   correctly labeled so they land in the right changelog category).
3. Trigger the `Create Release` workflow with the chosen version. Prefer the
   `gh` CLI:
   ```sh
   gh workflow run "Create Release" -f version=1.4.3
   ```
   If the user cannot run it, direct them to Actions → Create Release → Run
   workflow, and enter the version.
4. Monitor the pipeline and report status:
   ```sh
   gh run list --workflow "Create Release" --limit 1
   gh run watch <run-id>
   ```
5. Confirm completion: the GitHub Release exists with generated notes, npm and
   Docker are published, the Homebrew/Claude Code commit landed on `main`, and
   the release is marked latest. `gh release view MCP-1.4.3` should show it as
   latest.

## Common Mistakes

- Manually drafting the release in the GitHub UI instead of running
  `Create Release`. The workflow owns tag creation and note generation.
- Creating or merging pull requests for the Homebrew or Claude Code updates.
  The pipeline commits those directly to `main`; there are no release PRs.
- Manually marking the release as latest. `Promote release to latest` does this
  automatically once the pipeline finishes.
- Hand-writing the changelog. It is generated deterministically from merged PRs;
  fix PR titles/labels instead of editing notes by hand.
- Including the `MCP-` prefix in the version input (use `1.4.3`, not
  `MCP-1.4.3`).
