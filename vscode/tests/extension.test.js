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

function makeContext({ extensionPath, version, firstRun } = {}) {
    const globalStateStore = { 'codescene.firstRunCompleted': firstRun === true ? undefined : true };
    return {
        extensionPath: extensionPath ?? FAKE_EXT_PATH,
        extension: { packageJSON: { version: version ?? '0.1.0' } },
        subscriptions: [],
        globalState: {
            get(key) { return globalStateStore[key]; },
            update(key, value) { globalStateStore[key] = value; return Promise.resolve(); },
        },
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
        assert.equal(ctx.subscriptions.length, 7);
    });

    it('registers MCP server definition provider with id codesceneMcp', () => {
        const ctx = makeContext();
        extension.activate(ctx);

        assert.equal(state.registeredProviders.length, 1);
        assert.equal(state.registeredProviders[0].id, 'codesceneMcp');
    });

    it('registers four commands', () => {
        const ctx = makeContext();
        extension.activate(ctx);

        assert.ok(state.registeredCommands['codescene.signIn']);
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

    it('returns server definition without prompting for a token', async () => {
        state.configValues.accessToken = '';

        const ctx = makeContext();
        extension.activate(ctx);

        const provider = findProvider();
        const server = { label: 'CodeScene CodeHealth MCP' };
        const result = await provider.resolveMcpServerDefinition(server);

        assert.equal(result, server);
        assert.equal(state.shownWarnings.length, 0);
    });

    it('returns non-matching servers unchanged', async () => {
        const ctx = makeContext();
        extension.activate(ctx);

        const provider = findProvider();
        const server = { label: 'Some Other Server' };
        const result = await provider.resolveMcpServerDefinition(server);

        assert.equal(result, server);
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
        assert.ok(state.shownInfoMessages[0].message.includes('blocks OAuth'));
    });

    it('clears token when user submits empty string', async () => {
        state.inputBoxResult = '';

        const ctx = makeContext();
        extension.activate(ctx);

        await findCommand('codescene.configure')();

        assert.equal(state.configUpdates.length, 1);
        assert.equal(state.configUpdates[0].value, '');
        assert.ok(state.shownInfoMessages[0].message.includes('Access token cleared'));
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
        state.configValues.accountId = '';

        const ctx = makeContext({ extensionPath: '/nonexistent' });
        extension.activate(ctx);

        findCommand('codescene.showStatus')();

        assert.equal(state.shownInfoMessages.length, 1);
        const msg = state.shownInfoMessages[0].message;
        assert.ok(msg.includes('Status: Enabled'));
        assert.ok(msg.includes('Auth: OAuth'));
        assert.ok(msg.includes('Account ID: Not set'));
        assert.ok(msg.includes('Binary: Not available'));
        assert.ok(msg.includes('Platform:'));
    });

    it('shows PAT status and account id when configured', () => {
        state.configValues.enabled = true;
        state.configValues.accessToken = 'tok';
        state.configValues.accountId = 42;

        const ctx = makeContext({ extensionPath: '/nonexistent' });
        extension.activate(ctx);

        findCommand('codescene.showStatus')();

        const msg = state.shownInfoMessages[0].message;
        assert.ok(msg.includes('Auth: PAT configured'));
        assert.ok(msg.includes('Account ID: 42'));
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

describe('first-run prompt', () => {
    beforeEach(() => { reset(); });

    it('shows welcome message on first run', async () => {
        state.infoMessageResult = 'Skip';
        const ctx = makeContext({ firstRun: true });
        extension.activate(ctx);

        // Allow the async showInformationMessage to resolve
        await new Promise(r => setTimeout(r, 10));

        const welcome = state.shownInfoMessages.find(m => m.message.includes('Welcome'));
        assert.ok(welcome, 'expected welcome message');
    });

    it('does not show welcome message on subsequent runs', () => {
        const ctx = makeContext({ firstRun: false });
        extension.activate(ctx);

        const welcome = state.shownInfoMessages.find(m => m.message.includes('Welcome'));
        assert.equal(welcome, undefined);
    });

    it('triggers sign-in when user clicks Sign In', async () => {
        state.infoMessageResult = 'Sign In to CodeScene';
        // inputBoxResult = undefined simulates cancellation in the sign-in flow
        state.inputBoxResult = undefined;
        const ctx = makeContext({ firstRun: true });
        extension.activate(ctx);

        await new Promise(r => setTimeout(r, 10));

        // The sign-in flow should have prompted for on-prem URL
        const inputBox = state.shownInputBoxes.find(i =>
            i.options.prompt?.includes('CodeScene instance URL')
        );
        assert.ok(inputBox, 'expected on-prem URL prompt from sign-in flow');
    });
});

describe('codescene.signIn command', () => {
    beforeEach(() => { reset(); });
    afterEach(() => {
        if (existsSync(FAKE_EXT_PATH)) {
            rmSync(FAKE_EXT_PATH, { recursive: true, force: true });
        }
    });

    function setupBinary(ctx) {
        const binDir = join(ctx.extensionPath, 'bin');
        mkdirSync(binDir, { recursive: true });
        const binaryName = process.platform === 'darwin' && process.arch === 'arm64'
            ? 'cs-mcp-macos-aarch64'
            : process.platform === 'linux' ? 'cs-mcp-linux-amd64' : 'cs-mcp-windows-amd64.exe';
        writeFileSync(join(binDir, binaryName), '#!/bin/sh\necho {}');
    }

    it('prompts for on-prem URL when not configured', async () => {
        state.inputBoxResult = '';
        state.execFileResult = { error: null, stdout: '{"status":"signed_in"}', stderr: '' };
        const ctx = makeContext();
        setupBinary(ctx);

        extension.activate(ctx);
        const handler = findCommand('codescene.signIn');
        await handler();

        const inputBox = state.shownInputBoxes.find(i =>
            i.options.prompt?.includes('CodeScene instance URL')
        );
        assert.ok(inputBox, 'expected on-prem URL prompt');
    });

    it('does nothing when user cancels on-prem URL prompt', async () => {
        state.inputBoxResult = undefined;
        const ctx = makeContext();
        extension.activate(ctx);

        const handler = findCommand('codescene.signIn');
        await handler();

        assert.equal(state.shownErrors.length, 0);
        assert.equal(state.execFileCalls.length, 0);
    });

    it('skips on-prem URL prompt when already configured', async () => {
        state.configValues['onpremUrl'] = 'https://codescene.myco.com';
        state.execFileResult = { error: null, stdout: '{"status":"signed_in"}', stderr: '' };
        const ctx = makeContext();
        setupBinary(ctx);

        extension.activate(ctx);
        const handler = findCommand('codescene.signIn');
        await handler();

        const inputBox = state.shownInputBoxes.find(i =>
            i.options.prompt?.includes('CodeScene instance URL')
        );
        assert.equal(inputBox, undefined);
    });

    it('shows error when binary is not available', async () => {
        state.inputBoxResult = '';
        const ctx = makeContext({ extensionPath: '/nonexistent' });
        extension.activate(ctx);

        const handler = findCommand('codescene.signIn');
        await handler();

        const errorMsg = state.shownErrors.find(m => m.message.includes('Binary not available'));
        assert.ok(errorMsg, 'expected binary not available error');
    });

    it('shows success message on signed_in result', async () => {
        state.inputBoxResult = '';
        state.execFileResult = { error: null, stdout: '{"status":"signed_in"}', stderr: '' };
        const ctx = makeContext();
        setupBinary(ctx);

        extension.activate(ctx);
        const handler = findCommand('codescene.signIn');
        await handler();

        const msg = state.shownInfoMessages.find(m => m.message.includes('Successfully signed in'));
        assert.ok(msg, 'expected success message');
    });

    it('shows success message on already_signed_in result', async () => {
        state.inputBoxResult = '';
        state.execFileResult = { error: null, stdout: '{"status":"already_signed_in"}', stderr: '' };
        const ctx = makeContext();
        setupBinary(ctx);

        extension.activate(ctx);
        const handler = findCommand('codescene.signIn');
        await handler();

        const msg = state.shownInfoMessages.find(m => m.message.includes('Successfully signed in'));
        assert.ok(msg, 'expected success message');
    });

    it('shows warning on incomplete login', async () => {
        state.inputBoxResult = '';
        state.execFileResult = { error: null, stdout: '{"status":"expired","error":"session expired"}', stderr: '' };
        const ctx = makeContext();
        setupBinary(ctx);

        extension.activate(ctx);
        const handler = findCommand('codescene.signIn');
        await handler();

        const msg = state.shownWarnings.find(m => m.message.includes('Sign in incomplete'));
        assert.ok(msg, 'expected warning message');
    });

    it('shows error message on execFile failure', async () => {
        state.inputBoxResult = '';
        state.execFileResult = { error: new Error('spawn failed'), stdout: '', stderr: 'timeout' };
        const ctx = makeContext();
        setupBinary(ctx);

        extension.activate(ctx);
        const handler = findCommand('codescene.signIn');
        await handler();

        const msg = state.shownErrors.find(m => m.message.includes('Sign in failed'));
        assert.ok(msg, 'expected error message');
        assert.ok(msg.message.includes('timeout'), 'expected stderr in message');
    });

    it('handles unparseable stdout gracefully', async () => {
        state.inputBoxResult = '';
        state.execFileResult = { error: null, stdout: 'not json', stderr: '' };
        const ctx = makeContext();
        setupBinary(ctx);

        extension.activate(ctx);
        const handler = findCommand('codescene.signIn');
        await handler();

        const msg = state.shownInfoMessages.find(m => m.message.includes('Sign in completed'));
        assert.ok(msg, 'expected fallback success message');
    });

    it('saves on-prem URL when user provides one', async () => {
        state.inputBoxResult = 'https://cs.internal.io';
        state.execFileResult = { error: null, stdout: '{"status":"signed_in"}', stderr: '' };
        const ctx = makeContext();
        setupBinary(ctx);

        extension.activate(ctx);
        const handler = findCommand('codescene.signIn');
        await handler();

        const update = state.configUpdates.find(u => u.key === 'onpremUrl');
        assert.ok(update, 'expected onpremUrl config update');
        assert.equal(update.value, 'https://cs.internal.io');
    });

    it('passes account ID to auth subprocess environment', async () => {
        state.configValues['onpremUrl'] = 'https://cs.co';
        state.configValues['accountId'] = '42';
        state.execFileResult = { error: null, stdout: '{"status":"signed_in"}', stderr: '' };
        const ctx = makeContext();
        setupBinary(ctx);

        extension.activate(ctx);
        const handler = findCommand('codescene.signIn');
        await handler();

        assert.equal(state.execFileCalls.length, 1);
        const { options } = state.execFileCalls[0];
        assert.equal(options.env['CS_ONPREM_URL'], 'https://cs.co');
        assert.equal(options.env['CS_ACCOUNT_ID'], '42');
    });
});
