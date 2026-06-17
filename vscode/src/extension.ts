import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';

/** Platform-specific binary names bundled in the extension's bin/ directory. */
const BINARY_MAP: Record<string, string> = {
    'darwin-arm64': 'cs-mcp-macos-aarch64',
    'darwin-x64': 'cs-mcp-macos-amd64',
    'linux-arm64': 'cs-mcp-linux-aarch64',
    'linux-x64': 'cs-mcp-linux-amd64',
    'win32-x64': 'cs-mcp-windows-amd64.exe',
};

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
                        'CodeScene Code Health',
                        binaryPath,
                        [],
                        env,
                        context.extension.packageJSON.version,
                    ),
                ];
            },
            resolveMcpServerDefinition: async (server: vscode.McpServerDefinition) => {
                if (server.label !== 'CodeScene Code Health') {
                    return server;
                }

                // Check if access token is configured
                const config = vscode.workspace.getConfiguration('codescene');
                const token = config.get<string>('accessToken', '');

                if (!token) {
                    const action = await vscode.window.showWarningMessage(
                        'CodeScene: No access token configured. Some features require an access token.',
                        'Configure Now',
                        'Continue Without'
                    );

                    if (action === 'Configure Now') {
                        await vscode.commands.executeCommand('codescene.configure');
                        // Re-read config after user input
                        const updatedConfig = vscode.workspace.getConfiguration('codescene');
                        const updatedToken = updatedConfig.get<string>('accessToken', '');
                        if (updatedToken && server instanceof vscode.McpStdioServerDefinition) {
                            (server as any).env = {
                                ...(server as any).env,
                                CS_ACCESS_TOKEN: updatedToken,
                            };
                        }
                    }
                }

                return server;
            },
        })
    );

    // Command: Configure access token
    context.subscriptions.push(
        vscode.commands.registerCommand('codescene.configure', async () => {
            const token = await vscode.window.showInputBox({
                prompt: 'Enter your CodeScene access token',
                password: true,
                placeHolder: 'Paste your CodeScene access token here...',
                ignoreFocusOut: true,
            });

            if (token !== undefined) {
                const config = vscode.workspace.getConfiguration('codescene');
                await config.update('accessToken', token, vscode.ConfigurationTarget.Global);
                vscode.window.showInformationMessage('CodeScene: Access token saved.');
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
            const binaryPath = getBinaryPath(context);

            const items: string[] = [
                `Status: ${enabled ? 'Enabled' : 'Disabled'}`,
                `Access Token: ${token ? 'Configured' : 'Not set'}`,
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
    const binaryName = BINARY_MAP[key];

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
 * Builds the environment variables to pass to the MCP server process.
 */
function buildEnvironment(config: vscode.WorkspaceConfiguration): Record<string, string> {
    const env: Record<string, string> = {};

    const token = config.get<string>('accessToken', '');
    if (token) {
        env['CS_ACCESS_TOKEN'] = token;
    }

    const onpremUrl = config.get<string>('onpremUrl', '');
    if (onpremUrl) {
        env['CS_ONPREM_URL'] = onpremUrl;
    }

    const projectId = config.get<string>('defaultProjectId', '');
    if (projectId) {
        env['CS_DEFAULT_PROJECT_ID'] = projectId;
    }

    const enabledTools = config.get<string>('enabledTools', '');
    if (enabledTools) {
        env['CS_ENABLED_TOOLS'] = enabledTools;
    }

    const disableVersionCheck = config.get<boolean>('disableVersionCheck', false);
    if (disableVersionCheck) {
        env['CS_DISABLE_VERSION_CHECK'] = '1';
    }

    const caBundlePath = config.get<string>('caBundlePath', '');
    if (caBundlePath) {
        env['REQUESTS_CA_BUNDLE'] = caBundlePath;
    }

    return env;
}

/**
 * Updates the status bar item.
 */
function updateStatusBar(active: boolean) {
    if (active) {
        statusBarItem.text = '$(shield) CodeScene';
        statusBarItem.tooltip = 'CodeScene Code Health MCP — Active';
        statusBarItem.show();
    } else {
        statusBarItem.text = '$(shield) CodeScene (off)';
        statusBarItem.tooltip = 'CodeScene Code Health MCP — Disabled';
        statusBarItem.show();
    }
}
