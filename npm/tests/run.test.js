import { describe, it, beforeEach, afterEach, mock } from "node:test";
import assert from "node:assert/strict";

/**
 * Tests for the run module which spawns the cs-mcp binary.
 *
 * Since runBinary calls process.exit() and spawnSync, we test by
 * mocking those functions and verifying the correct behavior.
 */

/** Sentinel error thrown when process.exit is called, to stop execution. */
class ExitCalled extends Error {
  constructor(code) {
    super(`process.exit(${code})`);
    this.exitCode = code;
  }
}

/**
 * Dynamically imports the run module with a mocked spawnSync.
 */
async function importRun(spawnSyncMock) {
  mock.module("node:child_process", {
    namedExports: { spawnSync: spawnSyncMock },
  });
  return import(`../lib/run.js?v=${Date.now()}_${Math.random()}`);
}

/**
 * Creates a spawnSync mock that returns the given result shape.
 */
function createSpawnResult({ status = null, signal = null, error = null }) {
  return () => ({ status, signal, error });
}

/**
 * Runs runBinary with a mocked spawnSync and captures the exit code.
 * Returns the exit code thrown via process.exit.
 */
async function runAndCaptureExit(spawnResult, binaryPath = "/bin/test") {
  const { runBinary } = await importRun(createSpawnResult(spawnResult));
  try {
    runBinary(binaryPath, []);
    assert.fail("Expected process.exit to be called");
  } catch (err) {
    if (!(err instanceof ExitCalled)) throw err;
    return err.exitCode;
  }
}

describe("runBinary", () => {
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

  it("calls spawnSync with correct arguments", async () => {
    let capturedArgs;
    const spawnSyncMock = (binary, args, opts) => {
      capturedArgs = { binary, args, opts };
      return { status: 0, signal: null, error: null };
    };

    const { runBinary } = await importRun(spawnSyncMock);

    assert.throws(() => runBinary("/usr/bin/cs-mcp", ["--version"]), ExitCalled);

    assert.equal(capturedArgs.binary, "/usr/bin/cs-mcp");
    assert.deepEqual(capturedArgs.args, ["--version"]);
    assert.equal(capturedArgs.opts.stdio, "inherit");
    assert.equal(capturedArgs.opts.windowsHide, true);
  });

  it("throws on unknown spawn error", async () => {
    const { runBinary } = await importRun(
      createSpawnResult({
        error: new Error("Something unexpected"),
      })
    );

    assert.throws(() => runBinary("/bin/test", []), {
      message: "Something unexpected",
    });
  });

  const exitCodeCases = [
    { name: "child exit code 0", input: { status: 0 }, expected: 0 },
    { name: "child exit code 42", input: { status: 42 }, expected: 42 },
    { name: "SIGTERM signal", input: { signal: "SIGTERM" }, expected: 143 },
    { name: "SIGINT signal", input: { signal: "SIGINT" }, expected: 130 },
    { name: "SIGKILL signal", input: { signal: "SIGKILL" }, expected: 137 },
    { name: "SIGHUP signal", input: { signal: "SIGHUP" }, expected: 129 },
    { name: "unknown signal", input: { signal: "SIGUSR1" }, expected: 129 },
    { name: "no status or signal", input: {}, expected: 1 },
  ];

  for (const tc of exitCodeCases) {
    it(`exits with ${tc.expected} on ${tc.name}`, async () => {
      const code = await runAndCaptureExit(tc.input);
      assert.equal(code, tc.expected);
    });
  }

  const spawnErrorCases = [
    { errorCode: "ENOENT", exitCode: 127, message: "Binary not found" },
    { errorCode: "EACCES", exitCode: 126, message: "Permission denied" },
  ];

  for (const tc of spawnErrorCases) {
    it(`exits with ${tc.exitCode} on ${tc.errorCode}`, async () => {
      let stderrOutput = "";
      const origWrite = process.stderr.write;
      process.stderr.write = (chunk) => {
        stderrOutput += chunk;
        return true;
      };

      try {
        const error = Object.assign(new Error(tc.errorCode), {
          code: tc.errorCode,
        });
        const code = await runAndCaptureExit({ error });
        assert.equal(code, tc.exitCode);
        assert.ok(stderrOutput.includes(tc.message));
      } finally {
        process.stderr.write = origWrite;
      }
    });
  }
});
