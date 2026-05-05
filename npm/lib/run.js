/**
 * Spawns the cs-mcp binary as a child process.
 *
 * Uses `stdio: "inherit"` so that stdin, stdout, and stderr are passed
 * directly through to the child process. This is critical for the MCP
 * stdio transport where the MCP client communicates via JSON-RPC over
 * the process's stdin/stdout.
 *
 * Signals are forwarded to the child process so that graceful shutdown
 * works as expected.
 */

import { spawn } from "node:child_process";

/** Signals that indicate a normal client-initiated shutdown. */
const CLEAN_SHUTDOWN_SIGNALS = new Set(["SIGTERM", "SIGINT"]);

/** Maps signal names to their numeric values for exit code encoding. */
const SIGNAL_NUMBERS = { SIGKILL: 9, SIGHUP: 1 };

/**
 * Handle spawn errors (binary not found, permission denied, etc.).
 * Throws for unexpected errors.
 *
 * @param {Error} error - The spawn error
 * @param {string} binaryPath - Path to the binary that failed to spawn
 * @returns {never}
 */
function handleSpawnError(error, binaryPath) {
  if (error.code === "ENOENT") {
    process.stderr.write(`Error: Binary not found at ${binaryPath}\n`);
    process.exit(127);
  }
  if (error.code === "EACCES") {
    process.stderr.write(
      `Error: Permission denied executing ${binaryPath}\n`
    );
    process.exit(126);
  }
  throw error;
}

/**
 * Determine the exit code when the child was killed by a signal.
 *
 * SIGTERM and SIGINT are normal shutdown signals sent by MCP clients
 * (e.g. VS Code, Zed) when the user closes the agent. Exiting with
 * a non-zero code causes the client to surface a "fatal error" dialog,
 * so treat these as clean exits.
 *
 * @param {string} signal - The signal name (e.g. "SIGTERM")
 * @returns {number} The exit code to use
 */
function exitCodeForSignal(signal) {
  if (CLEAN_SHUTDOWN_SIGNALS.has(signal)) {
    return 0;
  }
  return 128 + (SIGNAL_NUMBERS[signal] || 1);
}

/**
 * Runs the cs-mcp binary, forwarding all stdio and signals.
 *
 * This function does not return until the binary exits.
 * The current process exits with the same exit code as the binary.
 *
 * @param {string} binaryPath - Absolute path to the cs-mcp binary
 * @param {string[]} args - Command-line arguments to pass through
 * @returns {never}
 */
export function runBinary(binaryPath, args) {
  const child = spawn(binaryPath, args, {
    stdio: "inherit",
    env: process.env,
    windowsHide: true,
  });

  let exited = false;

  /**
   * Exit exactly once and unregister signal handlers.
   *
   * @param {number} code
   */
  function exitOnce(code) {
    if (exited) {
      return;
    }
    exited = true;

    process.off("SIGTERM", forwardSigterm);
    process.off("SIGINT", forwardSigint);
    process.exit(code);
  }

  /**
   * Forward a signal to the child process.
   *
   * @param {NodeJS.Signals} signal
   */
  function forwardSignal(signal) {
    if (child.exitCode !== null || child.signalCode !== null) {
      return;
    }
    try {
      child.kill(signal);
    } catch {
      // Ignore race conditions where the child exits before kill().
    }
  }

  const forwardSigterm = () => forwardSignal("SIGTERM");
  const forwardSigint = () => forwardSignal("SIGINT");

  process.on("SIGTERM", forwardSigterm);
  process.on("SIGINT", forwardSigint);

  child.once("error", (error) => {
    handleSpawnError(error, binaryPath);
  });

  child.once("exit", (status, signal) => {
    if (status !== null) {
      exitOnce(status);
      return;
    }
    if (signal) {
      exitOnce(exitCodeForSignal(signal));
      return;
    }
    exitOnce(1);
  });

  // Keep the wrapper process alive while the child is running.
  // Actual exit happens in the child "exit" event handler above.
}
