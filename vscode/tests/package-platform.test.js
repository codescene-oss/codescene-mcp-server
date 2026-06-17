import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { VSCODE_TO_BINARY_TARGET, ALL_TARGETS } from '../scripts/package-platform.js';

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
