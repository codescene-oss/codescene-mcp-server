import { describe, it, beforeEach, afterEach, before, after } from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import {
  mkdirSync,
  writeFileSync,
  rmSync,
  existsSync,
  readFileSync,
} from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import { getCachedBinaryPath, ensureBinary } from "../lib/download.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = join(__dirname, "..");
const CACHE_BASE = join(PACKAGE_ROOT, ".cache");

/**
 * Creates a minimal zip file containing a single file with the given name.
 * Uses the system `zip` command which is available on macOS and most Linux.
 */
function createZipWithBinary(zipPath, binaryName, binaryContent) {
  const tmpDir = join(dirname(zipPath), "_zip_staging");
  mkdirSync(tmpDir, { recursive: true });
  const binaryPath = join(tmpDir, binaryName);
  writeFileSync(binaryPath, binaryContent, { mode: 0o755 });
  execFileSync("zip", ["-j", zipPath, binaryPath], { stdio: "pipe" });
  rmSync(tmpDir, { recursive: true });
}

/**
 * Starts a local HTTP server that serves files based on configured routes.
 * Returns the server and a function to add routes.
 */
function createTestServer() {
  const routes = new Map();

  const server = http.createServer((req, res) => {
    const handler = routes.get(req.url);
    if (handler) {
      handler(req, res);
    } else {
      res.writeHead(404);
      res.end("Not Found");
    }
  });

  return {
    server,
    routes,
    addRoute(path, handler) {
      routes.set(path, handler);
    },
    addFileRoute(path, filePath) {
      routes.set(path, (_req, res) => {
        const content = readFileSync(filePath);
        res.writeHead(200, { "Content-Length": content.length });
        res.end(content);
      });
    },
    addRedirect(fromPath, toPath) {
      routes.set(fromPath, (_req, res) => {
        res.writeHead(302, { Location: toPath });
        res.end();
      });
    },
    listen() {
      return new Promise((resolve) => {
        server.listen(0, "127.0.0.1", () => {
          const addr = server.address();
          resolve(`http://127.0.0.1:${addr.port}`);
        });
      });
    },
    close() {
      return new Promise((resolve) => server.close(resolve));
    },
  };
}

/**
 * Clean up any cached binaries created during tests.
 */
function cleanCache() {
  if (existsSync(CACHE_BASE)) {
    rmSync(CACHE_BASE, { recursive: true });
  }
}

describe("getCachedBinaryPath", () => {
  it("returns a path under .cache/{version}/", () => {
    const result = getCachedBinaryPath("1.2.3");
    assert.ok(result.includes(join(".cache", "1.2.3")));
    assert.ok(result.endsWith("cs-mcp") || result.endsWith("cs-mcp.exe"));
  });

  it("changes path when version changes", () => {
    const path1 = getCachedBinaryPath("1.0.0");
    const path2 = getCachedBinaryPath("2.0.0");
    assert.notEqual(path1, path2);
    assert.ok(path1.includes("1.0.0"));
    assert.ok(path2.includes("2.0.0"));
  });
});

describe("ensureBinary", () => {
  let testServer;
  let baseUrl;
  let originalBaseUrl;
  let originalStderrWrite;
  let stderrOutput;

  before(async () => {
    testServer = createTestServer();
    baseUrl = await testServer.listen();
  });

  after(async () => {
    await testServer.close();
  });

  beforeEach(() => {
    cleanCache();
    originalBaseUrl = process.env.CS_MCP_DOWNLOAD_BASE_URL;
    process.env.CS_MCP_DOWNLOAD_BASE_URL = baseUrl;

    stderrOutput = "";
    originalStderrWrite = process.stderr.write;
    process.stderr.write = (chunk) => {
      stderrOutput += chunk;
      return true;
    };
  });

  afterEach(() => {
    process.stderr.write = originalStderrWrite;
    if (originalBaseUrl === undefined) {
      delete process.env.CS_MCP_DOWNLOAD_BASE_URL;
    } else {
      process.env.CS_MCP_DOWNLOAD_BASE_URL = originalBaseUrl;
    }
    cleanCache();
    testServer.routes.clear();
  });

  it("downloads and extracts a compressed binary", async () => {
    const version = "9.0.1";
    const binaryPath = getCachedBinaryPath(version);
    const binaryName = binaryPath.endsWith(".exe")
      ? "cs-mcp-windows-amd64.exe"
      : `cs-mcp-${process.platform === "darwin" ? "macos" : "linux"}-${process.arch === "arm64" ? "aarch64" : "amd64"}`;

    // Skip this test on Windows where we don't have the `zip` command
    if (process.platform === "win32") return;

    // Create a zip containing the binary
    const zipDir = join(CACHE_BASE, "_test_assets");
    mkdirSync(zipDir, { recursive: true });
    const zipPath = join(zipDir, "test.zip");
    createZipWithBinary(zipPath, binaryName, "#!/bin/sh\necho hello\n");

    // Determine the expected asset name via platform info
    const { getPlatformInfo } = await import("../lib/platform.js");
    const platformInfo = getPlatformInfo();
    const tag = `MCP-${version}`;

    testServer.addFileRoute(`/${tag}/${platformInfo.asset}`, zipPath);

    const result = await ensureBinary(version);

    assert.ok(existsSync(result), `Binary should exist at ${result}`);
    assert.ok(stderrOutput.includes("downloading"));
    assert.ok(stderrOutput.includes("Ready:"));
  });

  it("returns cached binary on second call without re-downloading", async () => {
    if (process.platform === "win32") return;

    const version = "9.0.2";
    const binaryPath = getCachedBinaryPath(version);

    // Pre-populate cache
    const cacheDir = dirname(binaryPath);
    mkdirSync(cacheDir, { recursive: true });
    writeFileSync(binaryPath, "fake-binary", { mode: 0o755 });

    let downloadAttempted = false;
    testServer.addRoute("/MCP-9.0.2/test", (_req, res) => {
      downloadAttempted = true;
      res.writeHead(200);
      res.end("data");
    });

    const result = await ensureBinary(version);

    assert.equal(result, binaryPath);
    assert.ok(!downloadAttempted, "Should not re-download cached binary");
  });

  it("follows HTTP redirects during download", async () => {
    if (process.platform === "win32") return;

    const version = "9.0.3";
    const { getPlatformInfo } = await import("../lib/platform.js");
    const platformInfo = getPlatformInfo();
    const tag = `MCP-${version}`;

    // Create the zip asset
    const zipDir = join(CACHE_BASE, "_test_redirect");
    mkdirSync(zipDir, { recursive: true });
    const zipPath = join(zipDir, "test.zip");
    const binaryName = `cs-mcp-${process.platform === "darwin" ? "macos" : "linux"}-${process.arch === "arm64" ? "aarch64" : "amd64"}`;
    createZipWithBinary(zipPath, binaryName, "#!/bin/sh\necho redirect\n");

    // Primary URL redirects to /actual-file
    testServer.addRedirect(
      `/${tag}/${platformInfo.asset}`,
      `${baseUrl}/actual-file`
    );
    testServer.addFileRoute("/actual-file", zipPath);

    const result = await ensureBinary(version);
    assert.ok(existsSync(result), "Binary should exist after redirect");
  });

  it("rejects on HTTP error status", async () => {
    const version = "9.0.4";
    const { getPlatformInfo } = await import("../lib/platform.js");
    const platformInfo = getPlatformInfo();
    const tag = `MCP-${version}`;

    testServer.addRoute(`/${tag}/${platformInfo.asset}`, (_req, res) => {
      res.writeHead(403);
      res.end("Forbidden");
    });

    await assert.rejects(() => ensureBinary(version), {
      message: /Download failed: HTTP 403/,
    });
  });

  it("reports download progress to stderr", async () => {
    if (process.platform === "win32") return;

    const version = "9.0.5";
    const { getPlatformInfo } = await import("../lib/platform.js");
    const platformInfo = getPlatformInfo();
    const tag = `MCP-${version}`;

    const zipDir = join(CACHE_BASE, "_test_progress");
    mkdirSync(zipDir, { recursive: true });
    const zipPath = join(zipDir, "test.zip");
    const binaryName = `cs-mcp-${process.platform === "darwin" ? "macos" : "linux"}-${process.arch === "arm64" ? "aarch64" : "amd64"}`;
    createZipWithBinary(zipPath, binaryName, "x".repeat(1024));

    testServer.addFileRoute(`/${tag}/${platformInfo.asset}`, zipPath);

    await ensureBinary(version);

    assert.ok(
      stderrOutput.includes("Downloading") || stderrOutput.includes("Ready"),
      "Should report progress or completion"
    );
  });
});
