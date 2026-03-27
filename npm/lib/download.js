/**
 * Downloads and caches the cs-mcp binary from GitHub releases.
 *
 * The binary is cached inside the package's own directory under
 * `.cache/{version}/` so it persists across runs but is cleaned
 * up naturally when the package is reinstalled or upgraded.
 */

import { createWriteStream, existsSync, readdirSync, mkdirSync, chmodSync } from "node:fs";
import { rename, rm } from "node:fs/promises";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import https from "node:https";
import http from "node:http";
import { getPlatformInfo, getDownloadUrl } from "./platform.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = join(__dirname, "..");

/**
 * Returns the directory where binaries are cached for a given version.
 *
 * @param {string} version
 * @returns {string}
 */
function getCacheDir(version) {
  return join(PACKAGE_ROOT, ".cache", version);
}

/**
 * Returns the expected path of the cached binary for a given version.
 *
 * @param {string} version
 * @returns {string}
 */
export function getCachedBinaryPath(version) {
  const { binary } = getPlatformInfo();
  return join(getCacheDir(version), binary);
}

/**
 * Checks whether an HTTP response is a redirect with a location header.
 *
 * @param {import("node:http").IncomingMessage} response
 * @returns {boolean}
 */
function isRedirect(response) {
  const status = response.statusCode ?? 0;
  return status >= 300 && status < 400 && Boolean(response.headers.location);
}

/**
 * Reports download progress to stderr at 10% intervals.
 *
 * @param {number} downloadedBytes
 * @param {number} totalBytes
 * @param {number} lastPercent - The last reported percent value
 * @returns {number} The updated lastPercent value
 */
function reportProgress(downloadedBytes, totalBytes, lastPercent) {
  if (!totalBytes) return lastPercent;

  const percent = Math.floor((downloadedBytes / totalBytes) * 100);
  if (percent !== lastPercent && percent % 10 === 0) {
    const mb = (downloadedBytes / 1024 / 1024).toFixed(1);
    process.stderr.write(`\r  Downloading... ${percent}% (${mb} MB)`);
    return percent;
  }
  return lastPercent;
}

/**
 * Downloads a file from a URL, following redirects (GitHub releases use 302s).
 *
 * @param {string} url
 * @param {string} destPath
 * @returns {Promise<void>}
 */
function downloadFile(url, destPath) {
  return new Promise((resolve, reject) => {
    const proto = url.startsWith("https") ? https : http;

    proto
      .get(url, (response) => {
        if (isRedirect(response)) {
          downloadFile(response.headers.location, destPath)
            .then(resolve)
            .catch(reject);
          response.resume();
          return;
        }

        if (response.statusCode !== 200) {
          response.resume();
          reject(
            new Error(
              `Download failed: HTTP ${response.statusCode} from ${url}`
            )
          );
          return;
        }

        const totalBytes = parseInt(response.headers["content-length"], 10);
        let downloadedBytes = 0;
        let lastPercent = -1;

        const file = createWriteStream(destPath);
        response.on("data", (chunk) => {
          downloadedBytes += chunk.length;
          lastPercent = reportProgress(downloadedBytes, totalBytes, lastPercent);
        });
        response.pipe(file);
        file.on("finish", () => {
          file.close();
          if (totalBytes) {
            process.stderr.write("\n");
          }
          resolve();
        });
        file.on("error", (err) => {
          file.close();
          reject(err);
        });
      })
      .on("error", reject);
  });
}

/**
 * Extracts a zip file to a destination directory.
 *
 * Uses platform-native tools: `unzip` on Unix (available on macOS and
 * most Linux distros) and PowerShell's Expand-Archive on Windows.
 *
 * @param {string} zipPath
 * @param {string} destDir
 * @returns {void}
 */
function extractZip(zipPath, destDir) {
  if (process.platform === "win32") {
    execFileSync(
      "powershell",
      [
        "-NoProfile",
        "-Command",
        `Expand-Archive -Path '${zipPath}' -DestinationPath '${destDir}' -Force`,
      ],
      { stdio: "pipe" }
    );
  } else {
    execFileSync("unzip", ["-o", "-q", zipPath, "-d", destDir], {
      stdio: "pipe",
    });
  }
}

/**
 * Downloads and extracts a compressed binary, or downloads a bare binary.
 *
 * @param {string} url - The download URL
 * @param {{ asset: string, compressed: boolean }} platformInfo
 * @param {string} cacheDir - Directory to download into
 * @param {string} binaryPath - Final expected binary path (for uncompressed)
 * @returns {Promise<void>}
 */
async function downloadAsset(url, platformInfo, cacheDir, binaryPath) {
  if (!platformInfo.compressed) {
    await downloadFile(url, binaryPath);
    return;
  }

  const zipPath = join(cacheDir, platformInfo.asset);
  try {
    await downloadFile(url, zipPath);
    process.stderr.write("  Extracting...\n");
    extractZip(zipPath, cacheDir);
  } finally {
    await rm(zipPath, { force: true }).catch(() => {});
  }
}

/**
 * Finds and renames a platform-named binary to the canonical name.
 *
 * Zip files from GitHub releases contain binaries like "cs-mcp-linux-amd64"
 * instead of plain "cs-mcp". This function locates such a candidate and
 * renames it.
 *
 * @param {string} cacheDir
 * @param {string} binaryPath - The expected canonical binary path
 * @returns {Promise<void>}
 */
async function renameExtractedBinary(cacheDir, binaryPath) {
  if (existsSync(binaryPath)) return;

  const files = readdirSync(cacheDir);
  const candidate = files.find(
    (f) => f.startsWith("cs-mcp") && !f.endsWith(".zip")
  );

  if (candidate) {
    await rename(join(cacheDir, candidate), binaryPath);
  } else {
    throw new Error(
      `Binary not found after download. Expected cs-mcp in ${cacheDir}. ` +
        `Found: ${files.join(", ")}`
    );
  }
}

/**
 * Ensures the cs-mcp binary is available for the given version.
 *
 * If the binary is already cached, returns its path immediately.
 * Otherwise, downloads it from GitHub releases, extracts if needed,
 * and caches it for future use.
 *
 * @param {string} version - The package version (e.g. "0.2.1")
 * @returns {Promise<string>} Path to the binary
 */
export async function ensureBinary(version) {
  if (typeof version !== "string" || !version) {
    throw new Error(
      `ensureBinary requires a non-empty version string, got: ${JSON.stringify(version)}`
    );
  }

  const binaryPath = getCachedBinaryPath(version);

  if (existsSync(binaryPath)) {
    return binaryPath;
  }

  const platformInfo = getPlatformInfo();
  const url = getDownloadUrl(version, platformInfo.asset);
  const cacheDir = getCacheDir(version);

  mkdirSync(cacheDir, { recursive: true });

  process.stderr.write(
    `CodeScene MCP Server v${version} - downloading for ${process.platform}/${process.arch}...\n`
  );

  await downloadAsset(url, platformInfo, cacheDir, binaryPath);
  await renameExtractedBinary(cacheDir, binaryPath);

  if (process.platform !== "win32") {
    chmodSync(binaryPath, 0o755);
  }

  process.stderr.write(`  Ready: ${binaryPath}\n`);
  return binaryPath;
}
