---
name: making-a-release
description: Use when a developer wants to cut, publish, or ship a new release of the CodeScene MCP Server, or asks how to make a release / bump the version.
---

# Making a Release

## Overview

Use this skill to cut a new release of the CodeScene MCP Server. The release
process is almost entirely automated by GitHub Actions: your job is to pick the
next version and push a tag. Everything else — creating the GitHub Release with
generated notes, building signed binaries for every platform, publishing the
Docker image, npm package and MCPB bundle, updating packaging metadata, and
promoting the release to "latest" — happens on CI with no pull requests to
merge.

## When to Use

- The user asks to make, cut, publish, or ship a new release.
- The user wants to bump the version and release it.
- The user asks how the release process works.

Do not use this skill to publish the VS Code extension on its own (that is a
separate manual `workflow_dispatch` on `Publish VS Code Extension`).

## How the Pipeline Works

The developer only performs step 1: **pushing the tag**. CI does the rest.

1. **Push the tag.** Create and push an annotated `MCP-<version>` tag (e.g.
   `MCP-1.4.3`) to `origin`. This single push is the entry point.
2. **Create the release.** The tag push triggers `Create GitHub Release`, which
   creates the GitHub Release for the tag with **deterministic, auto-generated
   release notes** (GitHub's `generate-notes` engine, configured by
   `.github/release.yml`, which categorizes merged PRs by label). The release is
   created but **not** yet marked latest.
3. **Build.** The same tag push triggers `Build and publish executables`
   (binaries, signing, MCPB bundle → uploaded to the release) and
   `Build and Publish Release` (Docker image `:tag` and `:latest`).
4. **Publish + update metadata.** On a successful build, `Publish npm Package`
   publishes to npm, and `Update packaging metadata` updates the Homebrew
   formula and Claude Code plugin, committing **directly to `main` — no PRs**.
5. **Promote to latest.** When `Update packaging metadata` finishes
   successfully, `Promote release to latest` marks the GitHub Release as latest.
   This is what signals users they are on an outdated version.

## Implementation

1. Confirm the current version and decide the next one. The version lives in
   `manifest.json` and `claude-code-plugin/.claude-plugin/plugin.json`; the most
   recent release tag is `MCP-<version>`. Follow semver.
2. Make sure `main` is in the desired state to release (all intended PRs merged,
   correctly labeled so they land in the right changelog category). Check out and
   pull the exact commit you want to tag.
3. Create and push the tag. The tag name is `MCP-<version>` (include the `MCP-`
   prefix here — it is part of the tag, unlike the old workflow input):
   ```sh
   git tag -a MCP-1.4.3 -m "Release MCP-1.4.3"
   git push origin MCP-1.4.3
   ```
4. Monitor the pipeline and report status:
   ```sh
   gh run list --workflow "Create GitHub Release" --limit 1
   gh run watch <run-id>
   ```
5. Confirm completion: the GitHub Release exists with generated notes, npm and
   Docker are published, the Homebrew/Claude Code commit landed on `main`, and
   the release is marked latest. `gh release view MCP-1.4.3` should show it as
   latest.

## Common Mistakes

- Manually drafting the release in the GitHub UI instead of pushing the tag. The
  `Create GitHub Release` workflow owns release creation and note generation.
- Creating or merging pull requests for the Homebrew or Claude Code updates.
  The pipeline commits those directly to `main`; there are no release PRs.
- Manually marking the release as latest. `Promote release to latest` does this
  automatically once the pipeline finishes.
- Hand-writing the changelog. It is generated deterministically from merged PRs;
  fix PR titles/labels instead of editing notes by hand.
- Forgetting the `MCP-` prefix on the tag. The tag must be `MCP-1.4.3`, not
  `1.4.3` — the build and release workflows only trigger on `MCP-*` tags.
- Tagging the wrong commit. The tag pins the release; make sure `main` (or the
  commit you tag) is exactly what you intend to ship before pushing.
