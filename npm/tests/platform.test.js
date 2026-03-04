import { describe, it, beforeEach, afterEach, mock } from "node:test";
import assert from "node:assert/strict";

/**
 * Re-imports the platform module with a fresh module cache.
 * This is needed because platform.js reads process.platform/arch
 * at call time, and we need each test to see its own mocked values.
 */
async function importPlatform() {
  // Cache-bust by appending a unique query parameter
  const id = `./platform_${Date.now()}_${Math.random()}.js`;
  return import(`../lib/platform.js?v=${id}`);
}

describe("getPlatformInfo", () => {
  let originalPlatform;
  let originalArch;

  beforeEach(() => {
    originalPlatform = process.platform;
    originalArch = process.arch;
  });

  afterEach(() => {
    Object.defineProperty(process, "platform", { value: originalPlatform });
    Object.defineProperty(process, "arch", { value: originalArch });
  });

  const supportedPlatforms = [
    {
      platform: "darwin",
      arch: "arm64",
      asset: "cs-mcp-macos-aarch64.zip",
      binary: "cs-mcp",
      compressed: true,
    },
    {
      platform: "darwin",
      arch: "x64",
      asset: "cs-mcp-macos-amd64.zip",
      binary: "cs-mcp",
      compressed: true,
    },
    {
      platform: "linux",
      arch: "arm64",
      asset: "cs-mcp-linux-aarch64.zip",
      binary: "cs-mcp",
      compressed: true,
    },
    {
      platform: "linux",
      arch: "x64",
      asset: "cs-mcp-linux-amd64.zip",
      binary: "cs-mcp",
      compressed: true,
    },
    {
      platform: "win32",
      arch: "x64",
      asset: "cs-mcp-windows-amd64.exe",
      binary: "cs-mcp.exe",
      compressed: false,
    },
  ];

  for (const tc of supportedPlatforms) {
    it(`returns correct info for ${tc.platform}/${tc.arch}`, async () => {
      Object.defineProperty(process, "platform", { value: tc.platform });
      Object.defineProperty(process, "arch", { value: tc.arch });

      const { getPlatformInfo } = await importPlatform();
      const info = getPlatformInfo();

      assert.equal(info.asset, tc.asset);
      assert.equal(info.binary, tc.binary);
      assert.equal(info.compressed, tc.compressed);
    });
  }

  it("throws for unsupported platform", async () => {
    Object.defineProperty(process, "platform", { value: "freebsd" });
    Object.defineProperty(process, "arch", { value: "x64" });

    const { getPlatformInfo } = await importPlatform();

    assert.throws(() => getPlatformInfo(), {
      message: /Unsupported platform: freebsd\/x64/,
    });
  });

  it("throws for unsupported arch on supported platform", async () => {
    Object.defineProperty(process, "platform", { value: "linux" });
    Object.defineProperty(process, "arch", { value: "ia32" });

    const { getPlatformInfo } = await importPlatform();

    assert.throws(() => getPlatformInfo(), {
      message: /Unsupported platform: linux\/ia32/,
    });
  });

  it("includes supported platforms in error message", async () => {
    Object.defineProperty(process, "platform", { value: "aix" });
    Object.defineProperty(process, "arch", { value: "ppc" });

    const { getPlatformInfo } = await importPlatform();

    assert.throws(() => getPlatformInfo(), {
      message: /Supported platforms:.*darwin\/arm64/,
    });
  });
});

describe("getDownloadUrl", () => {
  let originalEnv;

  beforeEach(() => {
    originalEnv = process.env.CS_MCP_DOWNLOAD_BASE_URL;
  });

  afterEach(() => {
    if (originalEnv === undefined) {
      delete process.env.CS_MCP_DOWNLOAD_BASE_URL;
    } else {
      process.env.CS_MCP_DOWNLOAD_BASE_URL = originalEnv;
    }
  });

  it("builds GitHub release URL by default", async () => {
    delete process.env.CS_MCP_DOWNLOAD_BASE_URL;
    const { getDownloadUrl } = await importPlatform();
    const url = getDownloadUrl("0.2.1", "cs-mcp-macos-aarch64.zip");
    assert.equal(
      url,
      "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-0.2.1/cs-mcp-macos-aarch64.zip"
    );
  });

  it("uses CS_MCP_DOWNLOAD_BASE_URL when set", async () => {
    process.env.CS_MCP_DOWNLOAD_BASE_URL = "http://localhost:9000/downloads";
    const { getDownloadUrl } = await importPlatform();
    const url = getDownloadUrl("1.0.0", "cs-mcp-linux-amd64.zip");
    assert.equal(
      url,
      "http://localhost:9000/downloads/MCP-1.0.0/cs-mcp-linux-amd64.zip"
    );
  });

  it("constructs tag from version with MCP- prefix", async () => {
    delete process.env.CS_MCP_DOWNLOAD_BASE_URL;
    const { getDownloadUrl } = await importPlatform();
    const url = getDownloadUrl("3.5.0", "test-asset.zip");
    assert.match(url, /\/MCP-3\.5\.0\//);
  });
});
