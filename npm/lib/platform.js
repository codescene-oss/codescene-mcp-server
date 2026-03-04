/**
 * Platform and architecture detection for downloading the correct binary.
 *
 * Maps Node.js platform/arch identifiers to the GitHub release asset names
 * used by the codescene-mcp-server build pipeline.
 */

const PLATFORM_MAP = {
  "darwin-arm64": {
    asset: "cs-mcp-macos-aarch64.zip",
    binary: "cs-mcp",
    compressed: true,
  },
  "darwin-x64": {
    asset: "cs-mcp-macos-amd64.zip",
    binary: "cs-mcp",
    compressed: true,
  },
  "linux-arm64": {
    asset: "cs-mcp-linux-aarch64.zip",
    binary: "cs-mcp",
    compressed: true,
  },
  "linux-x64": {
    asset: "cs-mcp-linux-amd64.zip",
    binary: "cs-mcp",
    compressed: true,
  },
  "win32-x64": {
    asset: "cs-mcp-windows-amd64.exe",
    binary: "cs-mcp.exe",
    compressed: false,
  },
};

/**
 * Returns the platform info for the current OS and architecture.
 *
 * @returns {{ asset: string, binary: string, compressed: boolean }}
 * @throws {Error} If the current platform/arch combination is unsupported.
 */
export function getPlatformInfo() {
  const key = `${process.platform}-${process.arch}`;
  const info = PLATFORM_MAP[key];

  if (!info) {
    const supported = Object.keys(PLATFORM_MAP)
      .map((k) => k.replace("-", "/"))
      .join(", ");
    throw new Error(
      `Unsupported platform: ${process.platform}/${process.arch}. ` +
        `Supported platforms: ${supported}`
    );
  }

  return info;
}

/**
 * Constructs the download URL for a given version and asset.
 *
 * By default, downloads from GitHub releases. Set CS_MCP_DOWNLOAD_BASE_URL
 * to override the base URL (useful for testing, mirrors, or air-gapped
 * environments). The env var should include the full base up to the tag
 * directory, e.g. "http://localhost:8080/releases/download".
 *
 * @param {string} version - The package version (e.g. "0.2.1")
 * @param {string} asset - The asset filename (e.g. "cs-mcp-macos-aarch64.zip")
 * @returns {string} The full download URL
 */
export function getDownloadUrl(version, asset) {
  const tag = `MCP-${version}`;
  const baseUrl =
    process.env.CS_MCP_DOWNLOAD_BASE_URL ||
    "https://github.com/codescene-oss/codescene-mcp-server/releases/download";
  return `${baseUrl}/${tag}/${asset}`;
}
