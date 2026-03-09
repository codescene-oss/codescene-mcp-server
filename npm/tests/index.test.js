import { describe, it, beforeEach, afterEach, mock } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, writeFileSync, mkdirSync, rmSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";

const __dirname = dirname(fileURLToPath(import.meta.url));

/** Sentinel error thrown when process.exit is called. */
class ExitCalled extends Error {
  constructor(code) {
    super(`process.exit(${code})`);
    this.exitCode = code;
  }
}

/**
 * Creates a temporary fake binary file and returns its path info.
 */
function createTempBinary(name) {
  const dir = join(tmpdir(), `cs-mcp-test-${Date.now()}`);
  mkdirSync(dir, { recursive: true });
  const filePath = join(dir, name);
  writeFileSync(filePath, "fake binary", { mode: 0o755 });
  return { dir, filePath };
}

/**
 * Sets up module mocks for download.js and run.js.
 * Returns a tracker object to inspect what was called.
 */
function setupMocks({ ensureBinaryFn, runBinaryFn }) {
  const tracker = { ensureBinaryCalled: false, runBinaryArgs: null };

  mock.module("../lib/download.js", {
    namedExports: {
      ensureBinary: async (version) => {
        tracker.ensureBinaryCalled = true;
        tracker.ensureBinaryVersion = version;
        return ensureBinaryFn(version);
      },
      getCachedBinaryPath: () => "unused",
    },
  });
  mock.module("../lib/run.js", {
    namedExports: {
      runBinary: (binaryPath, args) => {
        tracker.runBinaryArgs = { binaryPath, args };
        if (runBinaryFn) runBinaryFn(binaryPath, args);
      },
    },
  });

  return tracker;
}

/**
 * Imports the index module with a cache-busting query param.
 */
async function importIndex() {
  return import(`../lib/index.js?v=${Date.now()}_${Math.random()}`);
}

/**
 * Saves and restores the CS_MCP_BINARY_PATH env var around a callback.
 */
async function withEnvPath(value, fn) {
  const original = process.env.CS_MCP_BINARY_PATH;
  if (value === undefined) {
    delete process.env.CS_MCP_BINARY_PATH;
  } else {
    process.env.CS_MCP_BINARY_PATH = value;
  }
  try {
    await fn();
  } finally {
    if (original === undefined) {
      delete process.env.CS_MCP_BINARY_PATH;
    } else {
      process.env.CS_MCP_BINARY_PATH = original;
    }
  }
}

describe("main", () => {
  let originalExit;
  let originalArgv;
  let stderrOutput;
  let originalStderrWrite;

  beforeEach(() => {
    originalExit = process.exit;
    originalArgv = process.argv;
    process.exit = (code) => {
      throw new ExitCalled(code);
    };
    stderrOutput = "";
    originalStderrWrite = process.stderr.write;
    process.stderr.write = (chunk) => {
      stderrOutput += chunk;
      return true;
    };
  });

  afterEach(() => {
    process.exit = originalExit;
    process.argv = originalArgv;
    process.stderr.write = originalStderrWrite;
    mock.restoreAll();
  });

  it("uses CS_MCP_BINARY_PATH when set to a valid file", async () => {
    const { dir, filePath } = createTempBinary("cs-mcp");
    const tracker = setupMocks({
      ensureBinaryFn: () => {
        throw new Error("Should not be called");
      },
    });
    process.argv = ["node", "cs-mcp.js", "--version"];

    try {
      await withEnvPath(filePath, async () => {
        const { main } = await importIndex();
        await main();
      });
      assert.ok(tracker.runBinaryArgs, "runBinary should have been called");
      assert.equal(tracker.runBinaryArgs.binaryPath, filePath);
      assert.deepEqual(tracker.runBinaryArgs.args, ["--version"]);
    } finally {
      rmSync(dir, { recursive: true });
    }
  });

  /**
   * Asserts that main() exits with code 1 and stderr contains the message.
   */
  async function assertMainFails(envPath, expectedMessage) {
    process.argv = ["node", "cs-mcp.js"];
    await withEnvPath(envPath, async () => {
      const { main } = await importIndex();
      await assert.rejects(
        () => main(),
        (err) => err instanceof ExitCalled && err.exitCode === 1
      );
    });
    assert.ok(
      stderrOutput.includes(expectedMessage),
      `Expected stderr to include "${expectedMessage}"`
    );
  }

  it("exits with error when CS_MCP_BINARY_PATH points to nonexistent file", async () => {
    setupMocks({
      ensureBinaryFn: () => {
        throw new Error("Should not download");
      },
    });
    await assertMainFails("/nonexistent/path/cs-mcp", "does not exist");
  });

  it("falls back to ensureBinary when CS_MCP_BINARY_PATH is not set", async () => {
    const tracker = setupMocks({
      ensureBinaryFn: () => "/mock/path/cs-mcp",
    });
    process.argv = ["node", "cs-mcp.js"];

    await withEnvPath(undefined, async () => {
      const { main } = await importIndex();
      await main();
    });
    assert.ok(tracker.ensureBinaryCalled, "ensureBinary should be called");
  });

  it("forwards process.argv to runBinary", async () => {
    const tracker = setupMocks({
      ensureBinaryFn: () => "/mock/cs-mcp",
    });
    process.argv = ["node", "cs-mcp.js", "--stdio", "--debug"];

    await withEnvPath(undefined, async () => {
      const { main } = await importIndex();
      await main();
    });
    assert.deepEqual(tracker.runBinaryArgs.args, ["--stdio", "--debug"]);
  });

  it("catches and reports errors from ensureBinary", async () => {
    setupMocks({
      ensureBinaryFn: () => {
        throw new Error("Network failure simulation");
      },
    });
    await assertMainFails(undefined, "Network failure simulation");
  });
});

describe("getPackageVersion", () => {
  let originalExit;

  beforeEach(() => {
    originalExit = process.exit;
    process.exit = (code) => {
      throw new ExitCalled(code);
    };
  });

  afterEach(() => {
    process.exit = originalExit;
    mock.restoreAll();
  });

  it("reads version from package.json and passes to ensureBinary", async () => {
    const tracker = setupMocks({
      ensureBinaryFn: () => "/mock/cs-mcp",
    });

    await withEnvPath(undefined, async () => {
      const { main } = await importIndex();
      await main();
    });
    const pkg = JSON.parse(readFileSync(join(__dirname, "..", "package.json"), "utf8"));
    assert.equal(tracker.ensureBinaryVersion, pkg.version);
  });
});
