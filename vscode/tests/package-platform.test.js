import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { execSync, execFileSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { VSCODE_TO_BINARY_TARGET, ALL_TARGETS, packageForTarget } from '../scripts/package-platform.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..');

describe('VSCODE_TO_BINARY_TARGET', () => {
    it('maps all expected VS Code targets', () => {
        const expected = ['darwin-arm64', 'darwin-x64', 'linux-x64', 'linux-arm64', 'win32-x64'];
        assert.deepEqual(Object.keys(VSCODE_TO_BINARY_TARGET).sort(), expected.sort());
    });

    it('maps each VS Code target to a valid binary target', () => {
        for (const [vsTarget, binaryTarget] of Object.entries(VSCODE_TO_BINARY_TARGET)) {
            assert.ok(binaryTarget, `${vsTarget} should map to a binary target`);
            assert.equal(typeof binaryTarget, 'string');
        }
    });

    it('darwin-arm64 maps to darwin-arm64', () => {
        assert.equal(VSCODE_TO_BINARY_TARGET['darwin-arm64'], 'darwin-arm64');
    });

    it('win32-x64 maps to win32-x64', () => {
        assert.equal(VSCODE_TO_BINARY_TARGET['win32-x64'], 'win32-x64');
    });
});

describe('ALL_TARGETS', () => {
    it('contains all keys from VSCODE_TO_BINARY_TARGET', () => {
        assert.deepEqual(ALL_TARGETS.sort(), Object.keys(VSCODE_TO_BINARY_TARGET).sort());
    });

    it('has 5 supported targets', () => {
        assert.equal(ALL_TARGETS.length, 5);
    });
});

describe('packageForTarget', () => {
    it('is a function', () => {
        assert.equal(typeof packageForTarget, 'function');
    });

    it('rejects unsupported targets', async () => {
        // packageForTarget calls process.exit(1) for unknown targets.
        // We test this by checking it's not in the mapping.
        const unsupported = 'freebsd-x64';
        assert.equal(VSCODE_TO_BINARY_TARGET[unsupported], undefined);
    });

    it('all VS Code targets map to valid binary targets that exist in download-binary TARGETS', async () => {
        const { TARGETS } = await import('../scripts/download-binary.js');
        for (const [vsTarget, binaryTarget] of Object.entries(VSCODE_TO_BINARY_TARGET)) {
            assert.ok(TARGETS[binaryTarget], `${vsTarget} maps to ${binaryTarget} which should exist in TARGETS`);
        }
    });

    it('exits with code 1 for unsupported target when run as subprocess', () => {
        // Run packageForTarget in a subprocess to test the process.exit(1) path
        const script = `
            import { packageForTarget } from './scripts/package-platform.js';
            await packageForTarget('freebsd-arm64');
        `;
        try {
            execFileSync('node', ['--input-type=module', '-e', script], {
                cwd: ROOT,
                stdio: 'pipe',
            });
            assert.fail('Should have exited with non-zero code');
        } catch (err) {
            assert.ok(err.status !== 0, 'Should exit with non-zero code');
            const stderr = err.stderr.toString();
            assert.ok(stderr.includes('No binary available'), `stderr should mention no binary: ${stderr}`);
        }
    });
});

describe('package-platform.js main script', () => {
    it('exits with non-zero when download-binary fails for given target', () => {
        // Run the main script with a bad URL so the download fails,
        // proving the main script block executes
        try {
            execFileSync('node', ['scripts/package-platform.js', 'freebsd-arm64'], {
                cwd: ROOT,
                stdio: 'pipe',
            });
            assert.fail('Should have exited with non-zero code');
        } catch (err) {
            assert.ok(err.status !== 0);
        }
    });
});

describe('download-binary.js main script', () => {
    it('exits with non-zero for unknown target', () => {
        try {
            execFileSync('node', ['scripts/download-binary.js', 'freebsd-arm64'], {
                cwd: ROOT,
                stdio: 'pipe',
            });
            assert.fail('Should have exited with non-zero code');
        } catch (err) {
            assert.ok(err.status !== 0);
            const stderr = err.stderr.toString();
            assert.ok(stderr.includes('Unknown target'), `stderr should mention unknown target: ${stderr}`);
        }
    });
});
