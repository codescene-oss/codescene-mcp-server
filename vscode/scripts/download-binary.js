/**
 * Downloads the cs-mcp binary for a given platform target.
 *
 * Usage:
 *   node scripts/download-binary.js <target>
 *
 * Where <target> is one of:
 *   darwin-arm64, darwin-x64, linux-arm64, linux-x64, win32-x64
 *
 * Or use "current" to download for the current platform.
 *
 * The binary will be placed in the bin/ directory.
 * The version is read from package.json.
 */

import { mkdirSync, createWriteStream, chmodSync, existsSync, readFileSync, readdirSync, renameSync } from 'node:fs';
import { rm } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import https from 'node:https';
import http from 'node:http';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..');
const BIN_DIR = join(ROOT, 'bin');

const TARGETS = {
    'darwin-arm64': { asset: 'cs-mcp-macos-aarch64.zip', binary: 'cs-mcp-macos-aarch64', compressed: true },
    'darwin-x64': { asset: 'cs-mcp-macos-amd64.zip', binary: 'cs-mcp-macos-amd64', compressed: true },
    'linux-arm64': { asset: 'cs-mcp-linux-aarch64.zip', binary: 'cs-mcp-linux-aarch64', compressed: true },
    'linux-x64': { asset: 'cs-mcp-linux-amd64.zip', binary: 'cs-mcp-linux-amd64', compressed: true },
    'win32-x64': { asset: 'cs-mcp-windows-amd64.exe', binary: 'cs-mcp-windows-amd64.exe', compressed: false },
};

function getVersion() {
    if (process.env.CS_MCP_VERSION) {
        return process.env.CS_MCP_VERSION;
    }
    const pkg = JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf8'));
    if (!pkg.version || pkg.version === '0.0.0') {
        console.error('Error: package.json version is a placeholder (0.0.0).');
        console.error('Set CS_MCP_VERSION env var or run `npm version <version>` first.');
        process.exit(1);
    }
    return pkg.version;
}

function getDownloadUrl(version, asset) {
    const tag = `MCP-${version}`;
    const baseUrl = process.env.CS_MCP_DOWNLOAD_BASE_URL ||
        'https://github.com/codescene-oss/codescene-mcp-server/releases/download';
    return `${baseUrl}/${tag}/${asset}`;
}

function isRedirect(response) {
    return response.statusCode >= 300 && response.statusCode < 400 && response.headers.location;
}

function downloadFile(url, destPath) {
    return new Promise((resolve, reject) => {
        const proto = url.startsWith('https') ? https : http;
        proto.get(url, (response) => {
            if (isRedirect(response)) {
                downloadFile(response.headers.location, destPath).then(resolve).catch(reject);
                response.resume();
                return;
            }

            if (response.statusCode !== 200) {
                response.resume();
                reject(new Error(`Download failed: HTTP ${response.statusCode} from ${url}`));
                return;
            }

            const totalBytes = parseInt(response.headers['content-length'], 10);
            let downloadedBytes = 0;

            const file = createWriteStream(destPath);
            response.on('data', (chunk) => {
                downloadedBytes += chunk.length;
                if (totalBytes) {
                    const pct = Math.floor((downloadedBytes / totalBytes) * 100);
                    process.stderr.write(`\r  Downloading... ${pct}%`);
                }
            });
            response.pipe(file);
            file.on('finish', () => {
                file.close();
                process.stderr.write('\n');
                resolve();
            });
            file.on('error', reject);
        }).on('error', reject);
    });
}

function extractZip(zipPath, destDir) {
    if (process.platform === 'win32') {
        execFileSync('powershell', [
            '-NoProfile', '-Command',
            `Expand-Archive -Path '${zipPath}' -DestinationPath '${destDir}' -Force`
        ], { stdio: 'pipe' });
    } else {
        execFileSync('unzip', ['-o', '-q', zipPath, '-d', destDir], { stdio: 'pipe' });
    }
}

async function downloadCompressed(url, info, binaryDest) {
    const zipPath = join(BIN_DIR, info.asset);
    try {
        await downloadFile(url, zipPath);
        console.log('  Extracting...');
        extractZip(zipPath, BIN_DIR);

        const files = readdirSync(BIN_DIR);
        const candidate = files.find(f => f.startsWith('cs-mcp') && !f.endsWith('.zip') && f !== info.binary);
        if (candidate && !existsSync(binaryDest)) {
            renameSync(join(BIN_DIR, candidate), binaryDest);
        }
    } finally {
        await rm(zipPath, { force: true }).catch(() => { });
    }
}

async function downloadForTarget(target) {
    const info = TARGETS[target];
    if (!info) {
        console.error(`Unknown target: ${target}`);
        console.error(`Supported targets: ${Object.keys(TARGETS).join(', ')}`);
        process.exit(1);
    }

    const version = getVersion();
    const url = getDownloadUrl(version, info.asset);
    const binaryDest = join(BIN_DIR, info.binary);

    if (existsSync(binaryDest)) {
        console.log(`Binary already exists: ${binaryDest}`);
        return;
    }

    mkdirSync(BIN_DIR, { recursive: true });
    console.log(`Downloading cs-mcp v${version} for ${target}...`);
    console.log(`  URL: ${url}`);

    if (info.compressed) {
        await downloadCompressed(url, info, binaryDest);
    } else {
        await downloadFile(url, binaryDest);
    }

    if (!target.startsWith('win32')) {
        chmodSync(binaryDest, 0o755);
    }

    console.log(`  Ready: ${binaryDest}`);
}

// Main — only runs when executed directly
const isMain = process.argv[1] && import.meta.url.endsWith(process.argv[1].replace(/\\/g, '/'));
if (isMain) {
    const target = process.argv[2] || 'current';
    const resolvedTarget = target === 'current' ? `${process.platform}-${process.arch}` : target;

    downloadForTarget(resolvedTarget).catch(err => {
        console.error('Error:', err.message);
        process.exit(1);
    });
}

export { TARGETS, getVersion, getDownloadUrl, isRedirect, downloadFile, downloadForTarget, extractZip, downloadCompressed, BIN_DIR };
