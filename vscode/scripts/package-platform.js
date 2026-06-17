/**
 * Packages the extension for a specific VS Code platform target.
 *
 * Usage:
 *   node scripts/package-platform.js <vscode-target>
 *
 * Where <vscode-target> is one of:
 *   win32-x64, win32-arm64, linux-x64, linux-arm64, linux-armhf,
 *   alpine-x64, alpine-arm64, darwin-x64, darwin-arm64
 *
 * This script:
 * 1. Downloads the correct cs-mcp binary for the target platform
 * 2. Packages the extension as a platform-specific .vsix
 */

import { execSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..');

/** Maps VS Code platform targets to our binary targets. */
const VSCODE_TO_BINARY_TARGET = {
    'darwin-arm64': 'darwin-arm64',
    'darwin-x64': 'darwin-x64',
    'linux-x64': 'linux-x64',
    'linux-arm64': 'linux-arm64',
    'win32-x64': 'win32-x64',
    // These don't have native binaries yet; they won't get a binary bundled
    // 'win32-arm64': null,
    // 'linux-armhf': null,
    // 'alpine-x64': null,
    // 'alpine-arm64': null,
};

const ALL_TARGETS = Object.keys(VSCODE_TO_BINARY_TARGET);

async function packageForTarget(vsTarget) {
    const binaryTarget = VSCODE_TO_BINARY_TARGET[vsTarget];
    if (!binaryTarget) {
        console.error(`No binary available for VS Code target: ${vsTarget}`);
        console.error(`Supported targets: ${ALL_TARGETS.join(', ')}`);
        process.exit(1);
    }

    console.log(`\n=== Packaging for ${vsTarget} ===\n`);

    // Download the binary
    console.log(`Downloading binary for ${binaryTarget}...`);
    execSync(`node scripts/download-binary.js ${binaryTarget}`, {
        cwd: ROOT,
        stdio: 'inherit',
    });

    // Package with vsce
    console.log(`\nPackaging .vsix for --target ${vsTarget}...`);
    execSync(`npx @vscode/vsce package --target ${vsTarget}`, {
        cwd: ROOT,
        stdio: 'inherit',
    });

    console.log(`\n=== Done: ${vsTarget} ===\n`);
}

export { VSCODE_TO_BINARY_TARGET, ALL_TARGETS, packageForTarget };

// Main — only runs when executed directly
const isMain = process.argv[1] && import.meta.url.endsWith(process.argv[1].replace(/\\/g, '/'));
if (isMain) {
    const target = process.argv[2];

    if (!target) {
        console.log('Packaging for all supported platforms...\n');
        for (const t of ALL_TARGETS) {
            await packageForTarget(t);
        }
    } else if (target === 'current') {
        const current = `${process.platform}-${process.arch}`;
        await packageForTarget(current);
    } else {
        await packageForTarget(target);
    }
}
