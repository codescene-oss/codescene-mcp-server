import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import { execFile } from 'child_process';
import { buildEnvironment, getBinaryName, optionalIdString } from './config';

let statusBarItem: vscode.StatusBarItem;

const FIRST_RUN_KEY = 'codescene.firstRunCompleted';

export function activate(context: vscode.ExtensionContext) {
    const didChangeEmitter = new vscode.EventEmitter<void>();

    // Status bar indicator
    statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
    statusBarItem.command = 'codescene.showStatus';
    context.subscriptions.push(statusBarItem);
    updateStatusBar(true);

    // Register MCP server definition provider
    context.subscriptions.push(
        vscode.lm.registerMcpServerDefinitionProvider('codesceneMcp', {
            onDidChangeMcpServerDefinitions: didChangeEmitter.event,
            provideMcpServerDefinitions: async () => {
                const config = vscode.workspace.getConfiguration('codescene');
                const enabled = config.get<boolean>('enabled', true);

                if (!enabled) {
                    updateStatusBar(false);
                    return [];
                }

                const binaryPath = getBinaryPath(context);
                if (!binaryPath) {
                    vscode.window.showWarningMessage(
                        `CodeScene: No binary available for ${process.platform}/${process.arch}. ` +
                        'Please install a platform-specific version of this extension.'
                    );
                    updateStatusBar(false);
                    return [];
                }

                const env = buildEnvironment(config);
                updateStatusBar(true);

                return [
                    new vscode.McpStdioServerDefinition(
                        'CodeScene CodeHealth MCP',
                        binaryPath,
                        [],
                        env,
                        context.extension.packageJSON.version,
                    ),
                ];
            },
            resolveMcpServerDefinition: async (server: vscode.McpServerDefinition) => server,
        })
    );

    // Command: Sign in via OAuth (opens browser)
    context.subscriptions.push(
        vscode.commands.registerCommand('codescene.signIn', () =>
            runAuthCommand(context, didChangeEmitter, 'sign-in'))
    );

    // Command: Sign out of OAuth
    context.subscriptions.push(
        vscode.commands.registerCommand('codescene.signOut', () =>
            runAuthCommand(context, didChangeEmitter, 'sign-out'))
    );

    // Command: Configure access token (optional PAT / CI fallback)
    context.subscriptions.push(
        vscode.commands.registerCommand('codescene.configure', async () => {
            const token = await vscode.window.showInputBox({
                prompt: 'Enter a CodeScene access token (optional — prefer OAuth via Sign In)',
                password: true,
                placeHolder: 'Paste PAT or standalone token (leave empty to clear)...',
                ignoreFocusOut: true,
            });

            if (token !== undefined) {
                const config = vscode.workspace.getConfiguration('codescene');
                await config.update('accessToken', token, vscode.ConfigurationTarget.Global);
                vscode.window.showInformationMessage(
                    token
                        ? 'CodeScene: Access token saved. Note: a PAT blocks OAuth login until cleared.'
                        : 'CodeScene: Access token cleared. Use "CodeScene: Sign In" to authenticate.',
                );
                didChangeEmitter.fire();
            }
        })
    );

    // Command: Restart MCP server
    context.subscriptions.push(
        vscode.commands.registerCommand('codescene.restart', () => {
            didChangeEmitter.fire();
            vscode.window.showInformationMessage('CodeScene: MCP Server restarting...');
        })
    );

    // Command: Show status
    context.subscriptions.push(
        vscode.commands.registerCommand('codescene.showStatus', () => {
            const config = vscode.workspace.getConfiguration('codescene');
            const enabled = config.get<boolean>('enabled', true);
            const token = config.get<string>('accessToken', '');
            const accountIdStr = optionalIdString(config.get<string>('accountId', ''));
            const binaryPath = getBinaryPath(context);

            const items: string[] = [
                `Status: ${enabled ? 'Enabled' : 'Disabled'}`,
                `Auth: ${token ? 'PAT configured' : 'OAuth'}`,
                `Account ID: ${accountIdStr || 'Not set'}`,
                `Binary: ${binaryPath ? 'Found' : 'Not available'}`,
                `Platform: ${process.platform}/${process.arch}`,
            ];

            if (binaryPath) {
                items.push(`Binary path: ${binaryPath}`);
            }

            vscode.window.showInformationMessage(items.join(' | '));
        })
    );

    // Watch for configuration changes
    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration((e) => {
            if (e.affectsConfiguration('codescene')) {
                didChangeEmitter.fire();
            }
        })
    );

    // First-run prompt
    if (!context.globalState.get<boolean>(FIRST_RUN_KEY)) {
        showFirstRunPrompt(context, didChangeEmitter);
    }
}

export function deactivate() {
    if (statusBarItem) {
        statusBarItem.dispose();
    }
}

/**
 * Shows a first-run welcome prompt offering to sign in.
 */
async function showFirstRunPrompt(
    context: vscode.ExtensionContext,
    didChangeEmitter: vscode.EventEmitter<void>,
) {
    const signIn = 'Sign In to CodeScene';
    const skip = 'Skip';

    const choice = await vscode.window.showInformationMessage(
        'Welcome to CodeScene CodeHealth MCP! Sign in to enable full Code Health analysis with your CodeScene account.',
        signIn,
        skip,
    );

    await context.globalState.update(FIRST_RUN_KEY, true);

    if (choice === signIn) {
        await runAuthCommand(context, didChangeEmitter, 'sign-in');
    }
}

/**
 * Resolves the on-prem URL, prompting the user if not already configured.
 * Returns undefined if the user cancels, or the URL string (empty for cloud).
 */
async function resolveOnpremUrl(config: vscode.WorkspaceConfiguration): Promise<string | undefined> {
    const existing = config.get<string>('onpremUrl', '');
    if (existing) {
        return existing;
    }

    const input = await vscode.window.showInputBox({
        prompt: 'CodeScene instance URL (leave empty for CodeScene Cloud)',
        placeHolder: 'https://codescene.mycompany.com',
        ignoreFocusOut: true,
    });

    if (input === undefined) {
        return undefined; // User cancelled
    }

    if (input) {
        await config.update('onpremUrl', input, vscode.ConfigurationTarget.Global);
    }
    return input;
}

/**
 * Builds the environment for the auth subprocess.
 */
function buildAuthEnv(onpremUrl: string, accountId: string): Record<string, string> {
    const env: Record<string, string> = { ...process.env as Record<string, string> };
    if (onpremUrl) {
        env['CS_ONPREM_URL'] = onpremUrl;
    }
    if (accountId) {
        env['CS_ACCOUNT_ID'] = accountId;
    }
    return env;
}

type AuthSubprocessKind = 'sign-in' | 'sign-out';

interface AuthFlowOptions {
    kind: AuthSubprocessKind;
    args: string[];
    /** Return `undefined` to abort (e.g. user cancelled the on-prem prompt). */
    resolveOnpremUrl: (config: vscode.WorkspaceConfiguration) => Promise<string | undefined>;
    /** When true, always refresh MCP definitions after the subprocess finishes. */
    alwaysRefresh: boolean;
    successStatuses: string[];
}

function authFlowLabel(kind: AuthSubprocessKind): string {
    return kind === 'sign-in' ? 'Sign in' : 'Sign out';
}

/**
 * Handles JSON stdout from `cs-mcp auth` / `cs-mcp auth logout`.
 */
function handleAuthSubprocessResult(
    stdout: string,
    didChangeEmitter: vscode.EventEmitter<void>,
    options: Pick<AuthFlowOptions, 'kind' | 'successStatuses' | 'alwaysRefresh'>,
): void {
    const result = JSON.parse(stdout.trim());
    if (options.successStatuses.includes(result.status)) {
        const successVerb = options.kind === 'sign-in' ? 'signed in' : 'signed out';
        vscode.window.showInformationMessage(`CodeScene: Successfully ${successVerb}!`);
        didChangeEmitter.fire();
        return;
    }
    vscode.window.showWarningMessage(
        `CodeScene: ${authFlowLabel(options.kind)} incomplete — ${result.error || result.status}`,
    );
    if (options.alwaysRefresh) {
        didChangeEmitter.fire();
    }
}

/**
 * Resolves on-prem URL / account env, then spawns a bundled auth subcommand.
 */
async function runAuthFlow(
    context: vscode.ExtensionContext,
    didChangeEmitter: vscode.EventEmitter<void>,
    options: AuthFlowOptions,
): Promise<void> {
    const config = vscode.workspace.getConfiguration('codescene');
    const onpremUrl = await options.resolveOnpremUrl(config);
    if (onpremUrl === undefined) {
        return;
    }

    const binaryPath = getBinaryPath(context);
    if (!binaryPath) {
        vscode.window.showErrorMessage('CodeScene: Binary not available for this platform.');
        return;
    }

    const accountId = optionalIdString(config.get<string>('accountId', ''));
    const env = buildAuthEnv(onpremUrl, accountId);
    const label = authFlowLabel(options.kind);
    const progressTitle = `CodeScene: ${options.kind === 'sign-in' ? 'Signing in' : 'Signing out'}...`;
    const completedMessage = `CodeScene: ${label} completed.`;

    await vscode.window.withProgress(
        {
            location: vscode.ProgressLocation.Notification,
            title: progressTitle,
            cancellable: false,
        },
        () => new Promise<void>((resolve) => {
            execFile(binaryPath, options.args, { env }, (error, stdout, stderr) => {
                if (error) {
                    vscode.window.showErrorMessage(
                        `CodeScene: ${label} failed — ${stderr?.trim() || stdout?.trim() || error.message}`,
                    );
                    if (options.alwaysRefresh) {
                        didChangeEmitter.fire();
                    }
                } else {
                    try {
                        handleAuthSubprocessResult(stdout, didChangeEmitter, options);
                    } catch {
                        vscode.window.showInformationMessage(completedMessage);
                        didChangeEmitter.fire();
                    }
                }
                resolve();
            });
        }),
    );
}

const AUTH_FLOWS: Record<AuthSubprocessKind, Omit<AuthFlowOptions, 'kind'>> = {
    'sign-in': {
        args: ['auth'],
        resolveOnpremUrl,
        alwaysRefresh: false,
        successStatuses: ['signed_in', 'already_signed_in'],
    },
    'sign-out': {
        // Mirrors `cs auth logout` via the bundled binary: `cs-mcp auth logout`.
        args: ['auth', 'logout'],
        resolveOnpremUrl: async (config) => (config.get<string>('onpremUrl', '') || '').trim(),
        alwaysRefresh: true,
        successStatuses: ['signed_out'],
    },
};

/** Runs `cs-mcp auth` or `cs-mcp auth logout` for the given flow kind. */
async function runAuthCommand(
    context: vscode.ExtensionContext,
    didChangeEmitter: vscode.EventEmitter<void>,
    kind: AuthSubprocessKind,
) {
    await runAuthFlow(context, didChangeEmitter, { kind, ...AUTH_FLOWS[kind] });
}

/**
 * Resolves the path to the bundled cs-mcp binary for the current platform.
 */
function getBinaryPath(context: vscode.ExtensionContext): string | undefined {
    const key = `${process.platform}-${process.arch}`;
    const binaryName = getBinaryName(key);

    if (!binaryName) {
        return undefined;
    }

    const binaryPath = path.join(context.extensionPath, 'bin', binaryName);

    if (!fs.existsSync(binaryPath)) {
        return undefined;
    }

    return binaryPath;
}

/**
 * Updates the status bar item.
 */
function updateStatusBar(active: boolean) {
    if (active) {
        statusBarItem.text = '$(shield) CodeScene';
        statusBarItem.tooltip = 'CodeScene CodeHealth MCP — Active';
        statusBarItem.show();
    } else {
        statusBarItem.text = '$(shield) CodeScene (off)';
        statusBarItem.tooltip = 'CodeScene CodeHealth MCP — Disabled';
        statusBarItem.show();
    }
}
