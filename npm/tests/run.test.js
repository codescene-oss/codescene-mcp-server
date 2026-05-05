import { describe, it, beforeEach, afterEach, mock } from "node:test";
import assert from "node:assert/strict";

/**
 * Tests for the run module which spawns the cs-mcp binary.
 *
 * Since runBinary calls process.exit() and spawns a child process,
 * we test by mocking those functions and verifying the correct behavior.
 */

/** Sentinel error thrown when process.exit is called, to stop execution. */
class ExitCalled extends Error {
  constructor(code) {
    super(`process.exit(${code})`);
    this.exitCode = code;
  }
}

/**
 * Dynamically imports the run module with a mocked spawn.
 */
async function importRun(spawnMock) {
  mock.module("node:child_process", {
    namedExports: { spawn: spawnMock },
  });
  return import(`../lib/run.js?v=${Date.now()}_${Math.random()}`);
}

/**
 * Creates a lightweight child-process mock used by runBinary.
 */
function createChildMock() {
  const handlers = new Map();
  return {
    handlers,
    exitCode: null,
    signalCode: null,
    once(event, cb) {
      handlers.set(event, cb);
      return this;
    },
    emit(event, ...args) {
      const cb = handlers.get(event);
      if (cb) cb(...args);
    },
    kill(signal) {
      this.lastKillSignal = signal;
      return true;
    },
  };
}

/**
 * Runs runBinary with a mocked spawn and captures the exit code.
 * Returns the exit code thrown via process.exit.
 */
async function runAndCaptureExit(triggerExit, binaryPath = "/bin/test") {
  const child = createChildMock();
  const { runBinary } = await importRun(() => child);
  try {
    runBinary(binaryPath, []);
    triggerExit(child);
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
    const child = createChildMock();
    const spawnMock = (binary, args, opts) => {
      capturedArgs = { binary, args, opts };
      return child;
    };

    const { runBinary } = await importRun(spawnMock);

    assert.throws(() => {
      runBinary("/usr/bin/cs-mcp", ["--version"]);
      child.emit("exit", 0, null);
    }, ExitCalled);

    assert.equal(capturedArgs.binary, "/usr/bin/cs-mcp");
    assert.deepEqual(capturedArgs.args, ["--version"]);
    assert.equal(capturedArgs.opts.stdio, "inherit");
    assert.equal(capturedArgs.opts.windowsHide, true);
  });

  it("throws on unknown spawn error", async () => {
    const child = createChildMock();
    const { runBinary } = await importRun(() => child);

    assert.throws(() => {
      runBinary("/bin/test", []);
      child.emit("error", new Error("Something unexpected"));
    }, {
      message: "Something unexpected",
    });
  });

  const exitCodeCases = [
    {
      name: "child exit code 0",
      trigger: (child) => child.emit("exit", 0, null),
      expected: 0,
    },
    {
      name: "child exit code 42",
      trigger: (child) => child.emit("exit", 42, null),
      expected: 42,
    },
    {
      name: "SIGTERM signal",
      trigger: (child) => child.emit("exit", null, "SIGTERM"),
      expected: 0,
    },
    {
      name: "SIGINT signal",
      trigger: (child) => child.emit("exit", null, "SIGINT"),
      expected: 0,
    },
    {
      name: "SIGKILL signal",
      trigger: (child) => child.emit("exit", null, "SIGKILL"),
      expected: 137,
    },
    {
      name: "SIGHUP signal",
      trigger: (child) => child.emit("exit", null, "SIGHUP"),
      expected: 129,
    },
    {
      name: "unknown signal",
      trigger: (child) => child.emit("exit", null, "SIGUSR1"),
      expected: 129,
    },
    {
      name: "no status or signal",
      trigger: (child) => child.emit("exit", null, null),
      expected: 1,
    },
  ];

  for (const tc of exitCodeCases) {
    it(`exits with ${tc.expected} on ${tc.name}`, async () => {
      const code = await runAndCaptureExit(tc.trigger);
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
        const code = await runAndCaptureExit((child) => {
          const error = Object.assign(new Error(tc.errorCode), {
            code: tc.errorCode,
          });
          child.emit("error", error);
        });
        assert.equal(code, tc.exitCode);
        assert.ok(stderrOutput.includes(tc.message));
      } finally {
        process.stderr.write = origWrite;
      }
    });
  }

  it("forwards SIGTERM to child process", async () => {
    let child;
    const { runBinary } = await importRun(() => {
      child = createChildMock();
      return child;
    });

    assert.throws(() => {
      runBinary("/bin/test", []);
      process.emit("SIGTERM");
      assert.equal(child.lastKillSignal, "SIGTERM");
      child.emit("exit", null, "SIGTERM");
    }, ExitCalled);
  });
});
