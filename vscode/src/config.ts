/** Platform-specific binary names bundled in the extension's bin/ directory. */
export const BINARY_MAP: Record<string, string> = {
    'darwin-arm64': 'cs-mcp-macos-aarch64',
    'darwin-x64': 'cs-mcp-macos-amd64',
    'linux-arm64': 'cs-mcp-linux-aarch64',
    'linux-x64': 'cs-mcp-linux-amd64',
    'win32-x64': 'cs-mcp-windows-amd64.exe',
};

export interface ConfigLike {
    get<T>(key: string, defaultValue: T): T;
}

/**
 * Builds the environment variables to pass to the MCP server process.
 */
export function buildEnvironment(config: ConfigLike): Record<string, string> {
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
 * Resolves the binary name for the given platform key.
 */
export function getBinaryName(platformKey: string): string | undefined {
    return BINARY_MAP[platformKey];
}
