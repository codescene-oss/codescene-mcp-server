/**
 * Tests for vscode/src/extension.ts.
 *
 * Run with:
 *   node --require ./tests/vscode-mock-preload.cjs --test tests/extension.test.js
 *
 * The preload script injects a mock 'vscode' module so that out/extension.js
 * can be loaded without the real VS Code runtime.
 */

import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdirSync, writeFileSync, rmSync, existsSync } from 'node:fs';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Access the mock injected by the preload script
const { state, reset } = globalThis.__vscodeMock;

// We must require() the CJS output (not import) so the preload mock is in scope.
const { createRequire } = await import('node:module');
const require = createRequire(import.meta.url);
const extension = require('../out/extension.js');

// ── Helpers ────────────────────────────────────────────────────────────────

const FAKE_EXT_PATH = join(__dirname, '..', '.test-ext');

function makeContext({ extensionPath, version } = {}) {
    return {
        extensionPath: extensionPath ?? FAKE_EXT_PATH,
        extension: { packageJSON: { version: version ?? '0.1.0' } },
        subscriptions: [],
    };
}

function findProvider() {
    return state.registeredProviders[0]?.provider;
}

function findCommand(name) {
    return state.registeredCommands[name];
}

// ── Tests ──────────────────────────────────────────────────────────────────

describe('activate', () => {
    beforeEach(() => {
        reset();
    });

    it('creates a status bar item', () => {
        const ctx = makeContext();
        extension.activate(ctx);

        assert.equal(state.statusBarItems.length, 1);
        const sbi = state.statusBarItems[0];
        assert.equal(sbi.command, 'codescene.showStatus');
        assert.equal(sbi.text, '$(shield) CodeScene');
        assert.equal(sbi.tooltip, 'CodeScene CodeHealth MCP — Active');
    });

    it('pushes disposables onto context.subscriptions', () => {
        const ctx = makeContext();
        extension.activate(ctx);

        // status bar + provider + 3 commands + config watcher = 6
        assert.equal(ctx.subscriptions.length, 6);
    });

    it('registers MCP server definition provider with id codesceneMcp', () => {
        const ctx = makeContext();
        extension.activate(ctx);

        assert.equal(state.registeredProviders.length, 1);
        assert.equal(state.registeredProviders[0].id, 'codesceneMcp');
    });

    it('registers three commands', () => {
        const ctx = makeContext();
        extension.activate(ctx);

        assert.ok(state.registeredCommands['codescene.configure']);
        assert.ok(state.registeredCommands['codescene.restart']);
        assert.ok(state.registeredCommands['codescene.showStatus']);
    });

    it('registers a configuration change listener', () => {
        const ctx = makeContext();
        extension.activate(ctx);

        assert.equal(state.onDidChangeConfigListeners.length, 1);
    });
});

describe('deactivate', () => {
    beforeEach(() => { reset(); });

    it('disposes the status bar item', () => {
        const ctx = makeContext();
        extension.activate(ctx);
        const sbi = state.statusBarItems[0];

        extension.deactivate();
        assert.equal(sbi._disposed, true);
    });
});

describe('provideMcpServerDefinitions', () => {
    beforeEach(() => { reset(); });

    it('returns empty array when extension is disabled', async () => {
        state.configValues.enabled = false;
        const ctx = makeContext();
        extension.activate(ctx);

        const provider = findProvider();
        const defs = await provider.provideMcpServerDefinitions();

        assert.deepEqual(defs, []);
        // Status bar should show "off"
        const sbi = state.statusBarItems[0];
        assert.ok(sbi.text.includes('(off)'));
    });

    it('returns empty array and warns when binary is missing', async () => {
        state.configValues.enabled = true;
        const ctx = makeContext({ extensionPath: '/nonexistent/path' });
        extension.activate(ctx);

        const provider = findProvider();
        const defs = await provider.provideMcpServerDefinitions();

        assert.deepEqual(defs, []);
        assert.equal(state.shownWarnings.length, 1);
        assert.ok(state.shownWarnings[0].message.includes('No binary available'));
    });

    it('returns server definition when binary exists', async () => {
        // Create a fake binary file
        const binDir = join(FAKE_EXT_PATH, 'bin');
        mkdirSync(binDir, { recursive: true });

        const { getBinaryName } = require('../out/config.js');
        const binaryName = getBinaryName(`${process.platform}-${process.arch}`);

        if (!binaryName) {
            // Skip on unsupported platform
            rmSync(FAKE_EXT_PATH, { recursive: true, force: true });
            return;
        }

        writeFileSync(join(binDir, binaryName), 'fake-binary');

        state.configValues.enabled = true;
        state.configValues.accessToken = 'tok-123';

        const ctx = makeContext();
        extension.activate(ctx);

        const provider = findProvider();
        const defs = await provider.provideMcpServerDefinitions();

        assert.equal(defs.length, 1);
        assert.equal(defs[0].label, 'CodeScene CodeHealth MCP');
        assert.equal(defs[0].command, join(binDir, binaryName));
        assert.deepEqual(defs[0].args, []);
        assert.equal(defs[0].env['CS_ACCESS_TOKEN'], 'tok-123');
        assert.equal(defs[0].version, '0.1.0');

        // Status bar should be active
        const sbi = state.statusBarItems[0];
        assert.ok(!sbi.text.includes('(off)'));

        rmSync(FAKE_EXT_PATH, { recursive: true, force: true });
    });
});

describe('resolveMcpServerDefinition', () => {
    beforeEach(() => { reset(); });

    it('returns non-matching servers unchanged', async () => {
        const ctx = makeContext();
        extension.activate(ctx);

        const provider = findProvider();
        const server = { label: 'Some Other Server' };
        const result = await provider.resolveMcpServerDefinition(server);

        assert.equal(result, server);
    });

    it('prompts for token when not configured', async () => {
        state.configValues.accessToken = '';
        state.warningResult = 'Continue Without';

        const ctx = makeContext();
        extension.activate(ctx);

        const provider = findProvider();
        const server = { label: 'CodeScene CodeHealth MCP' };
        const result = await provider.resolveMcpServerDefinition(server);

        assert.equal(result, server);
        assert.equal(state.shownWarnings.length, 1);
        assert.ok(state.shownWarnings[0].message.includes('No access token'));
    });

    it('skips prompt when token is already configured', async () => {
        state.configValues.accessToken = 'already-set';

        const ctx = makeContext();
        extension.activate(ctx);

        const provider = findProvider();
        const server = { label: 'CodeScene CodeHealth MCP' };
        const result = await provider.resolveMcpServerDefinition(server);

        assert.equal(result, server);
        assert.equal(state.shownWarnings.length, 0);
    });

    it('invokes configure command when user chooses Configure Now', async () => {
        state.configValues.accessToken = '';
        state.warningResult = 'Configure Now';
        state.inputBoxResult = 'new-token-456';

        const ctx = makeContext();
        extension.activate(ctx);

        const { McpStdioServerDefinition } = globalThis.__vscodeMock.vscode;
        const server = new McpStdioServerDefinition('CodeScene CodeHealth MCP', '/bin/test', [], {}, '1.0');

        const provider = findProvider();
        const result = await provider.resolveMcpServerDefinition(server);

        // After configure, the updated token should be on the server env
        assert.equal(result.env['CS_ACCESS_TOKEN'], 'new-token-456');
    });
});

describe('codescene.configure command', () => {
    beforeEach(() => { reset(); });

    it('saves token and shows confirmation message', async () => {
        state.inputBoxResult = 'my-secret-token';

        const ctx = makeContext();
        extension.activate(ctx);

        await findCommand('codescene.configure')();

        assert.equal(state.configUpdates.length, 1);
        assert.equal(state.configUpdates[0].key, 'accessToken');
        assert.equal(state.configUpdates[0].value, 'my-secret-token');
        assert.equal(state.shownInfoMessages.length, 1);
        assert.ok(state.shownInfoMessages[0].message.includes('Access token saved'));
    });

    it('does nothing when user cancels input', async () => {
        state.inputBoxResult = undefined;

        const ctx = makeContext();
        extension.activate(ctx);

        await findCommand('codescene.configure')();

        assert.equal(state.configUpdates.length, 0);
        assert.equal(state.shownInfoMessages.length, 0);
    });
});

describe('codescene.restart command', () => {
    beforeEach(() => { reset(); });

    it('shows restart message', () => {
        const ctx = makeContext();
        extension.activate(ctx);

        findCommand('codescene.restart')();

        assert.equal(state.shownInfoMessages.length, 1);
        assert.ok(state.shownInfoMessages[0].message.includes('restarting'));
    });
});

describe('codescene.showStatus command', () => {
    beforeEach(() => { reset(); });

    it('shows status with enabled and no token', () => {
        state.configValues.enabled = true;
        state.configValues.accessToken = '';

        const ctx = makeContext({ extensionPath: '/nonexistent' });
        extension.activate(ctx);

        findCommand('codescene.showStatus')();

        assert.equal(state.shownInfoMessages.length, 1);
        const msg = state.shownInfoMessages[0].message;
        assert.ok(msg.includes('Status: Enabled'));
        assert.ok(msg.includes('Access Token: Not set'));
        assert.ok(msg.includes('Binary: Not available'));
        assert.ok(msg.includes('Platform:'));
    });

    it('shows status with disabled', () => {
        state.configValues.enabled = false;

        const ctx = makeContext({ extensionPath: '/nonexistent' });
        extension.activate(ctx);

        findCommand('codescene.showStatus')();

        const msg = state.shownInfoMessages[0].message;
        assert.ok(msg.includes('Status: Disabled'));
    });

    it('includes binary path when binary exists', () => {
        const binDir = join(FAKE_EXT_PATH, 'bin');
        mkdirSync(binDir, { recursive: true });

        const { getBinaryName } = require('../out/config.js');
        const binaryName = getBinaryName(`${process.platform}-${process.arch}`);

        if (!binaryName) {
            rmSync(FAKE_EXT_PATH, { recursive: true, force: true });
            return;
        }

        writeFileSync(join(binDir, binaryName), 'fake');
        state.configValues.enabled = true;
        state.configValues.accessToken = 'tok';

        const ctx = makeContext();
        extension.activate(ctx);

        findCommand('codescene.showStatus')();

        const msg = state.shownInfoMessages[0].message;
        assert.ok(msg.includes('Binary: Found'));
        assert.ok(msg.includes('Binary path:'));

        rmSync(FAKE_EXT_PATH, { recursive: true, force: true });
    });
});

describe('configuration change watcher', () => {
    beforeEach(() => { reset(); });

    it('fires event emitter when codescene config changes', () => {
        const ctx = makeContext();
        extension.activate(ctx);

        // The provider should have onDidChangeMcpServerDefinitions
        const provider = findProvider();
        let fired = false;
        provider.onDidChangeMcpServerDefinitions(() => { fired = true; });

        // Simulate a config change affecting 'codescene'
        const listener = state.onDidChangeConfigListeners[0];
        listener({ affectsConfiguration: (section) => section === 'codescene' });

        assert.equal(fired, true);
    });

    it('does not fire for unrelated config changes', () => {
        const ctx = makeContext();
        extension.activate(ctx);

        const provider = findProvider();
        let fired = false;
        provider.onDidChangeMcpServerDefinitions(() => { fired = true; });

        const listener = state.onDidChangeConfigListeners[0];
        listener({ affectsConfiguration: (section) => section === 'other.setting' });

        assert.equal(fired, false);
    });
});
