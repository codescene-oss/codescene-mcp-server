/**
 * Main orchestrator for the npm wrapper.
 *
 * Resolves the binary path (from CS_MCP_BINARY_PATH or by downloading
 * from GitHub releases) and launches it.
 */

import { readFileSync, existsSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";
import { ensureBinary } from "./download.js";
import { runBinary } from "./run.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = join(__dirname, "..");

/** Maximum number of retry attempts when package files are temporarily unavailable. */
const MAX_RETRIES = 5;

/** Delay in milliseconds between retry attempts. */
const RETRY_DELAY_MS = 1000;

/**
 * Returns true if the error is transient — caused by package files being
 * replaced during an npm update (file missing, partially written, or
 * missing version field).
 *
 * @param {Error} err
 * @returns {boolean}
 */
function isTransientStartupError(err) {
  if (err.code === "ENOENT") return true;
  if (err instanceof SyntaxError) return true;
  if (err.message && err.message.includes("missing version")) return true;
  return false;
}

/**
 * Reads the package version from package.json.
 *
 * @returns {string}
 * @throws {Error} If version is missing or not a string.
 */
function getPackageVersion() {
  const pkgPath = join(PACKAGE_ROOT, "package.json");
  const pkg = JSON.parse(readFileSync(pkgPath, "utf-8"));
  if (typeof pkg.version !== "string" || !pkg.version) {
    throw new Error(
      "Invalid or missing version in package.json — " +
        "the package may be updating. " +
        `Got: ${JSON.stringify(pkg.version)}`
    );
  }
  return pkg.version;
}

/**
 * Main entry point.
 *
 * Resolution order:
 * 1. CS_MCP_BINARY_PATH env var - use the specified binary directly
 * 2. Cached binary for the current package version
 * 3. Download from GitHub releases and cache
 *
 * When package files are temporarily unavailable (e.g. during an npm
 * update), retries up to MAX_RETRIES times before giving up. This
 * prevents transient errors when VS Code restarts the MCP server
 * while the package is being replaced.
 *
 * All command-line arguments (except the node binary and script path)
 * are forwarded to the cs-mcp binary.
 */
export async function main() {
  const args = process.argv.slice(2);

  let lastErr;
  for (let attempt = 0; attempt <= MAX_RETRIES; attempt++) {
    try {
      const binaryPath = await resolveBinaryPath();
      runBinary(binaryPath, args);
      return;
    } catch (err) {
      lastErr = err;
      if (attempt < MAX_RETRIES && isTransientStartupError(err)) {
        process.stderr.write(
          `Package files unavailable (${err.code || err.constructor.name}), ` +
            `retrying in ${RETRY_DELAY_MS / 1000}s ` +
            `(${attempt + 1}/${MAX_RETRIES})...\n`
        );
        await delay(RETRY_DELAY_MS);
        continue;
      }
      break;
    }
  }

  process.stderr.write(`Error: ${lastErr.message}\n`);
  process.exit(1);
}

/**
 * Resolves the path to the cs-mcp binary.
 *
 * @returns {Promise<string>}
 */
async function resolveBinaryPath() {
  // 1. Check for explicit binary path override
  const envPath = process.env.CS_MCP_BINARY_PATH;
  if (envPath) {
    const resolved = resolve(envPath);
    if (!existsSync(resolved)) {
      throw new Error(
        `CS_MCP_BINARY_PATH is set to "${envPath}" but the file does not exist.`
      );
    }
    return resolved;
  }

  // 2. Download or use cached binary matching this package version
  const version = getPackageVersion();
  return ensureBinary(version);
}
