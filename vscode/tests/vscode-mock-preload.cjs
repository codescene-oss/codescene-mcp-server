/**
 * Preload script that registers a mock 'vscode' module in the require cache.
 * Used with: node --require ./tests/vscode-mock-preload.cjs --test tests/extension.test.js
 *
 * The mock is intentionally minimal — just enough to exercise extension.ts code paths.
 * Each test can reconfigure the mock via the exported `__mock` helper.
 */

'use strict';

// ── Helpers ────────────────────────────────────────────────────────────────

/** Tiny EventEmitter stand-in that records fire() calls. */
function createEventEmitter() {
    const listeners = [];
    return {
        event: (listener) => { listeners.push(listener); },
        fire: (...args) => { listeners.forEach(fn => fn(...args)); },
        _listeners: listeners,
    };
}

// ── State that tests can inspect / reconfigure ─────────────────────────────

const state = {
    configValues: {},                 // key → value
    registeredProviders: [],          // { id, provider }
    registeredCommands: {},           // command → handler
    statusBarItems: [],               // [{ text, tooltip, command, show(), dispose() }]
    shownWarnings: [],                // [{ message, items, result }]
    shownInfoMessages: [],            // [{ message }]
    shownErrors: [],                  // [{ message }]
    shownInputBoxes: [],              // [{ options, result }]
    shownQuickPicks: [],              // [{ items, options }]
    configUpdates: [],                // [{ key, value, target }]
    onDidChangeConfigListeners: [],   // [(e) => void]
    executedCommands: [],             // [{ command, args }]

    // Pre-configured results for UI interactions
    warningResult: undefined,         // return value of showWarningMessage
    infoMessageResult: undefined,     // return value of showInformationMessage
    inputBoxResult: undefined,        // return value of showInputBox
    quickPickResult: undefined,       // return value of showQuickPick, or index into items
    quickPickIndex: undefined,
};

function reset() {
    state.configValues = {};
    state.registeredProviders = [];
    state.registeredCommands = {};
    state.statusBarItems = [];
    state.shownWarnings = [];
    state.shownInfoMessages = [];
    state.shownErrors = [];
    state.shownInputBoxes = [];
    state.shownQuickPicks = [];
    state.configUpdates = [];
    state.onDidChangeConfigListeners = [];
    state.executedCommands = [];
    state.warningResult = undefined;
    state.infoMessageResult = undefined;
    state.inputBoxResult = undefined;
    state.quickPickResult = undefined;
    state.quickPickIndex = undefined;
    state.execFileResult = null;
    state.execFileResults = [];
    state.execFileCalls = [];
}

// ── Mock vscode namespace ──────────────────────────────────────────────────

const StatusBarAlignment = { Left: 1, Right: 2 };
const ConfigurationTarget = { Global: 1, Workspace: 2, WorkspaceFolder: 3 };
const ProgressLocation = { Notification: 15, SourceControl: 1, Window: 10 };

class McpStdioServerDefinition {
    constructor(label, command, args, env, version) {
        this.label = label;
        this.command = command;
        this.args = args;
        this.env = env;
        this.version = version;
    }
}

const vscode = {
    StatusBarAlignment,
    ConfigurationTarget,
    ProgressLocation,
    EventEmitter: function () { return createEventEmitter(); },
    McpStdioServerDefinition,

    window: {
        createStatusBarItem(_alignment, _priority) {
            const item = {
                text: '', tooltip: '', command: '',
                show() { },
                hide() { },
                dispose() { item._disposed = true; },
                _disposed: false,
            };
            state.statusBarItems.push(item);
            return item;
        },
        showWarningMessage(message, ...items) {
            state.shownWarnings.push({ message, items });
            return Promise.resolve(state.warningResult);
        },
        showInformationMessage(message, ...items) {
            state.shownInfoMessages.push({ message, items });
            return Promise.resolve(state.infoMessageResult);
        },
        showErrorMessage(message) {
            state.shownErrors.push({ message });
            return Promise.resolve();
        },
        showInputBox(options) {
            state.shownInputBoxes.push({ options });
            return Promise.resolve(state.inputBoxResult);
        },
        showQuickPick(items, options) {
            state.shownQuickPicks.push({ items, options });
            if (state.quickPickIndex !== undefined) {
                return Promise.resolve(items[state.quickPickIndex]);
            }
            return Promise.resolve(state.quickPickResult);
        },
        withProgress(_options, task) {
            return task({ report() {} });
        },
    },

    workspace: {
        getConfiguration(_section) {
            return {
                get(key, defaultValue) {
                    return key in state.configValues ? state.configValues[key] : defaultValue;
                },
                update(key, value, target) {
                    state.configUpdates.push({ key, value, target });
                    state.configValues[key] = value;
                    return Promise.resolve();
                },
            };
        },
        onDidChangeConfiguration(listener) {
            state.onDidChangeConfigListeners.push(listener);
            return { dispose() { } };
        },
    },

    commands: {
        registerCommand(command, handler) {
            state.registeredCommands[command] = handler;
            return { dispose() { } };
        },
        executeCommand(command, ...args) {
            state.executedCommands.push({ command, args });
            if (state.registeredCommands[command]) {
                return Promise.resolve(state.registeredCommands[command](...args));
            }
            return Promise.resolve();
        },
    },

    lm: {
        registerMcpServerDefinitionProvider(id, provider) {
            state.registeredProviders.push({ id, provider });
            return { dispose() { } };
        },
    },
};

// ── Register in require cache ──────────────────────────────────────────────

const Module = require('module');
const originalResolveFilename = Module._resolveFilename;
Module._resolveFilename = function (request, parent, isMain, options) {
    if (request === 'vscode') {
        return 'vscode';  // Return a virtual module id
    }
    return originalResolveFilename.call(this, request, parent, isMain, options);
};

require.cache['vscode'] = {
    id: 'vscode',
    filename: 'vscode',
    loaded: true,
    exports: vscode,
};

// ── Mock child_process.execFile to avoid spawning real processes ────────────

const childProcess = require('child_process');
const originalExecFile = childProcess.execFile;

// Tests can set state.execFileResult = { error, stdout, stderr }
state.execFileResult = null;
state.execFileResults = [];
state.execFileCalls = [];

childProcess.execFile = function (file, args, options, callback) {
    state.execFileCalls.push({ file, args, options });
    let result = null;
    if (Array.isArray(state.execFileResults) && state.execFileResults.length > 0) {
        result = state.execFileResults.shift();
    } else {
        result = state.execFileResult;
    }
    if (result) {
        const { error, stdout, stderr } = result;
        if (callback) callback(error || null, stdout, stderr);
    } else {
        // Default: call original (tests that set a binary will need this)
        return originalExecFile.call(childProcess, file, args, options, callback);
    }
};

// ── Expose for tests ───────────────────────────────────────────────────────

globalThis.__vscodeMock = { state, reset, vscode };
