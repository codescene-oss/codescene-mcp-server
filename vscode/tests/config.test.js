import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { BINARY_MAP, buildEnvironment, getBinaryName } from '../out/config.js';

/** Helper to create a mock config object. */
function mockConfig(values = {}) {
    return {
        get(key, defaultValue) {
            return key in values ? values[key] : defaultValue;
        },
    };
}

describe('BINARY_MAP', () => {
    it('has entries for all supported platforms', () => {
        const expected = ['darwin-arm64', 'darwin-x64', 'linux-arm64', 'linux-x64', 'win32-x64'];
        assert.deepEqual(Object.keys(BINARY_MAP).sort(), expected.sort());
    });

    it('windows binary has .exe extension', () => {
        assert.ok(BINARY_MAP['win32-x64'].endsWith('.exe'));
    });

    it('non-windows binaries do not have .exe extension', () => {
        for (const [key, value] of Object.entries(BINARY_MAP)) {
            if (!key.startsWith('win32')) {
                assert.ok(!value.endsWith('.exe'), `${key} should not have .exe`);
            }
        }
    });
});

describe('getBinaryName', () => {
    it('returns correct name for linux-x64', () => {
        assert.equal(getBinaryName('linux-x64'), 'cs-mcp-linux-amd64');
    });

    it('returns correct name for darwin-arm64', () => {
        assert.equal(getBinaryName('darwin-arm64'), 'cs-mcp-macos-aarch64');
    });

    it('returns undefined for unsupported platform', () => {
        assert.equal(getBinaryName('freebsd-x64'), undefined);
    });
});

describe('buildEnvironment', () => {
    it('returns empty object when all settings are defaults', () => {
        const config = mockConfig({});
        const env = buildEnvironment(config);
        assert.deepEqual(env, {});
    });

    it('sets CS_ACCESS_TOKEN when accessToken is provided', () => {
        const config = mockConfig({ accessToken: 'my-token' });
        const env = buildEnvironment(config);
        assert.equal(env['CS_ACCESS_TOKEN'], 'my-token');
    });

    it('sets CS_ONPREM_URL when onpremUrl is provided', () => {
        const config = mockConfig({ onpremUrl: 'https://cs.example.com' });
        const env = buildEnvironment(config);
        assert.equal(env['CS_ONPREM_URL'], 'https://cs.example.com');
    });

    it('sets CS_DEFAULT_PROJECT_ID when defaultProjectId is provided', () => {
        const config = mockConfig({ defaultProjectId: '42' });
        const env = buildEnvironment(config);
        assert.equal(env['CS_DEFAULT_PROJECT_ID'], '42');
    });

    it('sets CS_ENABLED_TOOLS when enabledTools is provided', () => {
        const config = mockConfig({ enabledTools: 'code_health_score,code_health_review' });
        const env = buildEnvironment(config);
        assert.equal(env['CS_ENABLED_TOOLS'], 'code_health_score,code_health_review');
    });

    it('sets CS_DISABLE_VERSION_CHECK to "1" when disableVersionCheck is true', () => {
        const config = mockConfig({ disableVersionCheck: true });
        const env = buildEnvironment(config);
        assert.equal(env['CS_DISABLE_VERSION_CHECK'], '1');
    });

    it('does not set CS_DISABLE_VERSION_CHECK when disableVersionCheck is false', () => {
        const config = mockConfig({ disableVersionCheck: false });
        const env = buildEnvironment(config);
        assert.ok(!('CS_DISABLE_VERSION_CHECK' in env));
    });

    it('sets REQUESTS_CA_BUNDLE when caBundlePath is provided', () => {
        const config = mockConfig({ caBundlePath: '/path/to/ca.pem' });
        const env = buildEnvironment(config);
        assert.equal(env['REQUESTS_CA_BUNDLE'], '/path/to/ca.pem');
    });

    it('does not include empty string values', () => {
        const config = mockConfig({ accessToken: '', onpremUrl: '', defaultProjectId: '' });
        const env = buildEnvironment(config);
        assert.deepEqual(env, {});
    });

    it('sets all env vars when all settings are provided', () => {
        const config = mockConfig({
            accessToken: 'token-123',
            onpremUrl: 'https://cs.corp.com',
            defaultProjectId: '7',
            enabledTools: 'code_health_score',
            disableVersionCheck: true,
            caBundlePath: '/certs/ca.pem',
        });
        const env = buildEnvironment(config);
        assert.equal(env['CS_ACCESS_TOKEN'], 'token-123');
        assert.equal(env['CS_ONPREM_URL'], 'https://cs.corp.com');
        assert.equal(env['CS_DEFAULT_PROJECT_ID'], '7');
        assert.equal(env['CS_ENABLED_TOOLS'], 'code_health_score');
        assert.equal(env['CS_DISABLE_VERSION_CHECK'], '1');
        assert.equal(env['REQUESTS_CA_BUNDLE'], '/certs/ca.pem');
    });
});
