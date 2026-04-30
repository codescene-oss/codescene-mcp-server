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

import { spawnSync } from "node:child_process";

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
  const result = spawnSync(binaryPath, args, {
    stdio: "inherit",
    env: process.env,
    windowsHide: true,
  });

  if (result.error) {
    // Handle spawn errors (e.g. binary not found, permission denied)
    if (result.error.code === "ENOENT") {
      process.stderr.write(`Error: Binary not found at ${binaryPath}\n`);
      process.exit(127);
    }
    if (result.error.code === "EACCES") {
      process.stderr.write(
        `Error: Permission denied executing ${binaryPath}\n`
      );
      process.exit(126);
    }
    throw result.error;
  }

  // Exit with the same code as the child process.
  // If the child was killed by a signal, use 128 + signal number convention.
  if (result.status !== null) {
    process.exit(result.status);
  }
  if (result.signal) {
    // SIGTERM and SIGINT are normal shutdown signals sent by MCP clients
    // (e.g. VS Code, Zed) when the user closes the agent. Exiting with
    // a non-zero code causes the client to surface a "fatal error" dialog,
    // so treat these as clean exits.
    if (result.signal === "SIGTERM" || result.signal === "SIGINT") {
      process.exit(0);
    }
    // For other signals, use the 128 + signal number convention.
    const signalNumbers = { SIGKILL: 9, SIGHUP: 1 };
    const sigNum = signalNumbers[result.signal] || 1;
    process.exit(128 + sigNum);
  }
  process.exit(1);
}
