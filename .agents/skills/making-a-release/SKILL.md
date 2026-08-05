---
name: making-a-release
description: Use when a developer wants to cut, publish, or ship a new release of the CodeScene MCP Server, or asks how to make a release / bump the version.
---

# Making a Release

To release the CodeScene MCP Server, create and push an annotated `MCP-<version>`
tag. That single push is the entire process — CI does everything else (release
notes, binaries, Docker, npm, packaging metadata, and marking it latest).

1. Decide the next version following semver. The latest release is the most
   recent `MCP-<version>` tag.
2. Make sure the commit you tag (normally the tip of `main`) is exactly what you
   want to ship.
3. Create and push the tag (keep the `MCP-` prefix — CI only triggers on
   `MCP-*` tags):
   ```sh
   git tag -a MCP-1.4.3 -m "Release MCP-1.4.3"
   git push origin MCP-1.4.3
   ```

Do not bump version files, edit the changelog, draft the release in the GitHub
UI, or open any PRs — CI handles all of it.
