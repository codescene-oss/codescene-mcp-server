/**
 * Main orchestrator for the npm wrapper.
 *
 * Resolves the binary path (from CS_MCP_BINARY_PATH or by downloading
 * from GitHub releases) and launches it.
 */

import { readFileSync, existsSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { ensureBinary } from "./download.js";
import { runBinary } from "./run.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = join(__dirname, "..");

/**
 * Reads the package version from package.json.
 *
 * @returns {string}
 */
function getPackageVersion() {
  const pkgPath = join(PACKAGE_ROOT, "package.json");
  const pkg = JSON.parse(readFileSync(pkgPath, "utf-8"));
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
 * All command-line arguments (except the node binary and script path)
 * are forwarded to the cs-mcp binary.
 */
export async function main() {
  const args = process.argv.slice(2);

  try {
    const binaryPath = await resolveBinaryPath();
    runBinary(binaryPath, args);
  } catch (err) {
    process.stderr.write(`Error: ${err.message}\n`);
    process.exit(1);
  }
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
