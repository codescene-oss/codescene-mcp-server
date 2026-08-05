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

function setIfNonEmpty(env: Record<string, string>, key: string, value: string): void {
    if (value) {
        env[key] = value;
    }
}

/** Normalize optional string/number settings (e.g. accountId) to a trimmed string. */
export function optionalIdString(value: string | number | null | undefined): string {
    if (value == null) {
        return '';
    }
    if (value === '') {
        return '';
    }
    return String(value).trim();
}

/**
 * Builds the environment variables to pass to the MCP server process.
 */
export function buildEnvironment(config: ConfigLike): Record<string, string> {
    const env: Record<string, string> = {};

    setIfNonEmpty(env, 'CS_ACCESS_TOKEN', config.get<string>('accessToken', ''));
    setIfNonEmpty(env, 'CS_ONPREM_URL', config.get<string>('onpremUrl', ''));
    setIfNonEmpty(env, 'CS_ACCOUNT_ID', optionalIdString(config.get<string | number>('accountId', '')));
    setIfNonEmpty(env, 'CS_DEFAULT_PROJECT_ID', config.get<string>('defaultProjectId', ''));
    setIfNonEmpty(env, 'CS_ENABLED_TOOLS', config.get<string>('enabledTools', ''));
    setIfNonEmpty(env, 'REQUESTS_CA_BUNDLE', config.get<string>('caBundlePath', ''));

    if (config.get<boolean>('disableVersionCheck', false)) {
        env['CS_DISABLE_VERSION_CHECK'] = '1';
    }

    return env;
}

/**
 * Resolves the binary name for the given platform key.
 */
export function getBinaryName(platformKey: string): string | undefined {
    return BINARY_MAP[platformKey];
}
