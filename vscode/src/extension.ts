import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import { buildEnvironment, getBinaryName, optionalIdString } from './config';

let statusBarItem: vscode.StatusBarItem;

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
            // OAuth-first: do not gate on a PAT. Agents call the `login` tool.
            resolveMcpServerDefinition: async (server: vscode.McpServerDefinition) => server,
        })
    );

    // Command: Configure access token (optional PAT / CI fallback)
    context.subscriptions.push(
        vscode.commands.registerCommand('codescene.configure', async () => {
            const token = await vscode.window.showInputBox({
                prompt: 'Enter a CodeScene access token (optional — prefer OAuth via agent login)',
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
                        : 'CodeScene: Access token cleared. Ask the agent to log in with OAuth.',
                );
                didChangeEmitter.fire(); // Trigger MCP server restart
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
            const accountIdStr = optionalIdString(config.get<string | number>('accountId', ''));
            const binaryPath = getBinaryPath(context);

            const items: string[] = [
                `Status: ${enabled ? 'Enabled' : 'Disabled'}`,
                `Auth: ${token ? 'PAT configured (blocks OAuth)' : 'OAuth via agent login'}`,
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
}

export function deactivate() {
    if (statusBarItem) {
        statusBarItem.dispose();
    }
}

/**
 * Resolves the path to the bundled cs-mcp binary for the current platform.
 * The binary is expected to be in the extension's `bin/` directory.
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
