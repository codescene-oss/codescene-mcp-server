import {
  describe, it, beforeEach, afterEach, before, after, mock,
} from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import {
  mkdirSync, writeFileSync, rmSync, existsSync, readFileSync,
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
 * Returns the server and helper functions for adding routes.
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

/** Clean up any cached binaries created during tests. */
function cleanCache() {
  if (existsSync(CACHE_BASE)) {
    rmSync(CACHE_BASE, { recursive: true });
  }
}

/** Returns the platform-specific binary name for zip entries. */
function getPlatformBinaryName() {
  const osName = process.platform === "darwin" ? "macos" : "linux";
  const archName = process.arch === "arm64" ? "aarch64" : "amd64";
  return `cs-mcp-${osName}-${archName}`;
}

/**
 * Saves and replaces stderr and env state for download tests.
 * Returns an object with captured output and a restore function.
 */
function captureStderrAndEnv(serverBaseUrl) {
  const saved = {
    stderrWrite: process.stderr.write,
    baseUrl: process.env.CS_MCP_DOWNLOAD_BASE_URL,
  };
  const state = { output: "" };
  process.stderr.write = (chunk) => {
    state.output += chunk;
    return true;
  };
  process.env.CS_MCP_DOWNLOAD_BASE_URL = serverBaseUrl;
  return {
    get stderrOutput() { return state.output; },
    restore() {
      process.stderr.write = saved.stderrWrite;
      if (saved.baseUrl === undefined) {
        delete process.env.CS_MCP_DOWNLOAD_BASE_URL;
      } else {
        process.env.CS_MCP_DOWNLOAD_BASE_URL = saved.baseUrl;
      }
    },
  };
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

describe("ensureBinary input validation", () => {
  it("rejects when version is undefined", async () => {
    await assert.rejects(() => ensureBinary(undefined), {
      message: /non-empty version string/,
    });
  });

  it("rejects when version is null", async () => {
    await assert.rejects(() => ensureBinary(null), {
      message: /non-empty version string/,
    });
  });

  it("rejects when version is an empty string", async () => {
    await assert.rejects(() => ensureBinary(""), {
      message: /non-empty version string/,
    });
  });

  it("rejects when version is a number", async () => {
    await assert.rejects(() => ensureBinary(123), {
      message: /non-empty version string/,
    });
  });
});

describe("ensureBinary", () => {
  let testServer;
  let baseUrl;
  let ctx;

  before(async () => {
    testServer = createTestServer();
    baseUrl = await testServer.listen();
  });

  after(async () => {
    await testServer.close();
  });

  beforeEach(() => {
    cleanCache();
    ctx = captureStderrAndEnv(baseUrl);
  });

  afterEach(() => {
    ctx.restore();
    cleanCache();
    testServer.routes.clear();
  });

  it("downloads and extracts a compressed binary", async () => {
    if (process.platform === "win32") return;

    const version = "9.0.1";
    const binaryName = getPlatformBinaryName();
    const zipDir = join(CACHE_BASE, "_test_assets");
    mkdirSync(zipDir, { recursive: true });
    const zipPath = join(zipDir, "test.zip");
    createZipWithBinary(zipPath, binaryName, "#!/bin/sh\necho hello\n");

    const { getPlatformInfo } = await import("../lib/platform.js");
    const platformInfo = getPlatformInfo();
    testServer.addFileRoute(`/MCP-${version}/${platformInfo.asset}`, zipPath);

    const result = await ensureBinary(version);

    assert.ok(existsSync(result), `Binary should exist at ${result}`);
    assert.ok(ctx.stderrOutput.includes("downloading"));
    assert.ok(ctx.stderrOutput.includes("Ready:"));
  });

  it("returns cached binary on second call without re-downloading", async () => {
    const version = "9.0.2";
    const binaryPath = getCachedBinaryPath(version);

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

    const zipDir = join(CACHE_BASE, "_test_redirect");
    mkdirSync(zipDir, { recursive: true });
    const zipPath = join(zipDir, "test.zip");
    createZipWithBinary(zipPath, getPlatformBinaryName(), "#!/bin/sh\necho r\n");

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

    testServer.addRoute(`/MCP-${version}/${platformInfo.asset}`, (_req, res) => {
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

    const zipDir = join(CACHE_BASE, "_test_progress");
    mkdirSync(zipDir, { recursive: true });
    const zipPath = join(zipDir, "test.zip");
    createZipWithBinary(zipPath, getPlatformBinaryName(), "x".repeat(1024));

    testServer.addFileRoute(`/MCP-${version}/${platformInfo.asset}`, zipPath);

    await ensureBinary(version);

    assert.ok(
      ctx.stderrOutput.includes("Downloading") ||
        ctx.stderrOutput.includes("Ready"),
      "Should report progress or completion"
    );
  });

  it("rejects when file write fails during download", async () => {
    if (process.platform === "win32") return;

    const version = "9.0.8";
    const { getPlatformInfo } = await import("../lib/platform.js");
    const platformInfo = getPlatformInfo();

    const zipDir = join(CACHE_BASE, "_test_write_err");
    mkdirSync(zipDir, { recursive: true });
    const zipPath = join(zipDir, "test.zip");
    createZipWithBinary(zipPath, getPlatformBinaryName(), "data");
    testServer.addFileRoute(`/MCP-${version}/${platformInfo.asset}`, zipPath);

    // Create a directory where the zip file would be written, causing EISDIR
    const cacheDir = join(PACKAGE_ROOT, ".cache", version);
    mkdirSync(join(cacheDir, platformInfo.asset), { recursive: true });

    await assert.rejects(() => ensureBinary(version), (err) => {
      assert.ok(err.code === "EISDIR" || err.message.includes("EISDIR"));
      return true;
    });
  });

  it("rejects when zip contains no cs-mcp binary", async () => {
    if (process.platform === "win32") return;

    const version = "9.0.6";
    const { getPlatformInfo } = await import("../lib/platform.js");
    const platformInfo = getPlatformInfo();

    const zipDir = join(CACHE_BASE, "_test_no_binary");
    mkdirSync(zipDir, { recursive: true });
    const zipPath = join(zipDir, "test.zip");
    createZipWithBinary(zipPath, "wrong-binary-name", "not the right file");

    testServer.addFileRoute(`/MCP-${version}/${platformInfo.asset}`, zipPath);

    await assert.rejects(() => ensureBinary(version), {
      message: /Binary not found after download/,
    });
  });

  it("handles chunked delivery at non-boundary progress", async () => {
    if (process.platform === "win32") return;

    const version = "9.0.7";
    const { getPlatformInfo } = await import("../lib/platform.js");
    const platformInfo = getPlatformInfo();

    const zipDir = join(CACHE_BASE, "_test_chunks");
    mkdirSync(zipDir, { recursive: true });
    const zipPath = join(zipDir, "test.zip");
    createZipWithBinary(
      zipPath, getPlatformBinaryName(), "x".repeat(4096)
    );

    const zipContent = readFileSync(zipPath);
    testServer.addRoute(`/MCP-${version}/${platformInfo.asset}`, (_req, res) => {
      res.writeHead(200, { "Content-Length": zipContent.length });
      const chunkSize = Math.ceil(zipContent.length / 7);
      let offset = 0;
      const interval = setInterval(() => {
        const end = Math.min(offset + chunkSize, zipContent.length);
        res.write(zipContent.subarray(offset, end));
        offset = end;
        if (offset >= zipContent.length) {
          clearInterval(interval);
          res.end();
        }
      }, 5);
    });

    const result = await ensureBinary(version);
    assert.ok(existsSync(result));
  });
});

describe("ensureBinary with uncompressed asset", () => {
  let testServer;
  let baseUrl;
  let ctx;

  before(async () => {
    testServer = createTestServer();
    baseUrl = await testServer.listen();
  });

  after(async () => {
    await testServer.close();
    mock.restoreAll();
  });

  beforeEach(() => {
    cleanCache();
    ctx = captureStderrAndEnv(baseUrl);
  });

  afterEach(() => {
    ctx.restore();
    cleanCache();
    testServer.routes.clear();
    mock.restoreAll();
  });

  it("downloads bare binary when asset is not compressed", async () => {
    mock.module("../lib/platform.js", {
      namedExports: {
        getPlatformInfo: () => ({
          asset: "cs-mcp-windows-amd64.exe",
          binary: "cs-mcp.exe",
          compressed: false,
        }),
        getDownloadUrl: (version, asset) => {
          const base = process.env.CS_MCP_DOWNLOAD_BASE_URL;
          return `${base}/MCP-${version}/${asset}`;
        },
      },
    });

    const mod = await import(`../lib/download.js?v=${Date.now()}`);
    const version = "9.1.0";

    const binaryContent = "#!/bin/sh\necho bare\n";
    testServer.addRoute(`/MCP-${version}/cs-mcp-windows-amd64.exe`, (_req, res) => {
      res.writeHead(200, { "Content-Length": Buffer.byteLength(binaryContent) });
      res.end(binaryContent);
    });

    const result = await mod.ensureBinary(version);
    assert.ok(existsSync(result), `Binary should exist at ${result}`);
    assert.ok(result.endsWith("cs-mcp.exe"));

    const content = readFileSync(result, "utf8");
    assert.equal(content, binaryContent);
  });
});
