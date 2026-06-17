import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { mkdirSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { TARGETS, getDownloadUrl, isRedirect, downloadFile, BIN_DIR } from '../scripts/download-binary.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const TEST_TMP = join(__dirname, '..', '.test-tmp');

function createTestServer() {
    const routes = new Map();
    const server = http.createServer((req, res) => {
        const handler = routes.get(req.url);
        if (handler) {
            handler(req, res);
        } else {
            res.writeHead(404);
            res.end('Not Found');
        }
    });

    return {
        server,
        addRoute(path, handler) { routes.set(path, handler); },
        addFileRoute(path, content) {
            routes.set(path, (_req, res) => {
                res.writeHead(200, { 'Content-Length': content.length });
                res.end(content);
            });
        },
        addRedirect(fromPath, toPath) {
            routes.set(fromPath, (_req, res) => {
                res.writeHead(302, { Location: toPath });
                res.end();
            });
        },
        listen() {
            return new Promise((resolve) => {
                server.listen(0, '127.0.0.1', () => {
                    const addr = server.address();
                    resolve(`http://127.0.0.1:${addr.port}`);
                });
            });
        },
        close() {
            return new Promise((resolve) => server.close(resolve));
        },
    };
}

describe('TARGETS', () => {
    it('has entries for all supported platforms', () => {
        const expected = ['darwin-arm64', 'darwin-x64', 'linux-arm64', 'linux-x64', 'win32-x64'];
        assert.deepEqual(Object.keys(TARGETS).sort(), expected.sort());
    });

    it('all entries have asset, binary, and compressed fields', () => {
        for (const [key, value] of Object.entries(TARGETS)) {
            assert.ok(value.asset, `${key} missing asset`);
            assert.ok(value.binary, `${key} missing binary`);
            assert.equal(typeof value.compressed, 'boolean', `${key} compressed should be boolean`);
        }
    });

    it('windows target is not compressed', () => {
        assert.equal(TARGETS['win32-x64'].compressed, false);
    });

    it('non-windows targets are compressed', () => {
        for (const [key, value] of Object.entries(TARGETS)) {
            if (!key.startsWith('win32')) {
                assert.equal(value.compressed, true, `${key} should be compressed`);
            }
        }
    });
});

describe('getDownloadUrl', () => {
    const originalEnv = process.env.CS_MCP_DOWNLOAD_BASE_URL;

    afterEach(() => {
        if (originalEnv === undefined) {
            delete process.env.CS_MCP_DOWNLOAD_BASE_URL;
        } else {
            process.env.CS_MCP_DOWNLOAD_BASE_URL = originalEnv;
        }
    });

    it('constructs URL with version tag and asset', () => {
        delete process.env.CS_MCP_DOWNLOAD_BASE_URL;
        const url = getDownloadUrl('1.2.3', 'cs-mcp-linux-amd64.zip');
        assert.equal(url, 'https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-1.2.3/cs-mcp-linux-amd64.zip');
    });

    it('uses custom base URL from environment', () => {
        process.env.CS_MCP_DOWNLOAD_BASE_URL = 'http://localhost:9999/downloads';
        const url = getDownloadUrl('0.5.0', 'cs-mcp-macos-aarch64.zip');
        assert.equal(url, 'http://localhost:9999/downloads/MCP-0.5.0/cs-mcp-macos-aarch64.zip');
    });
});

describe('isRedirect', () => {
    it('returns truthy for 302 with location header', () => {
        const response = { statusCode: 302, headers: { location: 'http://example.com' } };
        assert.ok(isRedirect(response));
    });

    it('returns truthy for 301 with location header', () => {
        const response = { statusCode: 301, headers: { location: 'http://example.com' } };
        assert.ok(isRedirect(response));
    });

    it('returns false for 200', () => {
        const response = { statusCode: 200, headers: {} };
        assert.ok(!isRedirect(response));
    });

    it('returns false for 302 without location header', () => {
        const response = { statusCode: 302, headers: {} };
        assert.ok(!isRedirect(response));
    });

    it('returns false for 400+', () => {
        const response = { statusCode: 404, headers: { location: 'http://example.com' } };
        assert.ok(!isRedirect(response));
    });
});

describe('downloadFile', () => {
    let testServer;
    let baseUrl;

    beforeEach(async () => {
        testServer = createTestServer();
        baseUrl = await testServer.listen();
        mkdirSync(TEST_TMP, { recursive: true });
    });

    afterEach(async () => {
        await testServer.close();
        rmSync(TEST_TMP, { recursive: true, force: true });
    });

    it('downloads a file successfully', async () => {
        const content = Buffer.from('hello binary');
        testServer.addFileRoute('/file.bin', content);

        const dest = join(TEST_TMP, 'file.bin');
        await downloadFile(`${baseUrl}/file.bin`, dest);
        assert.ok(existsSync(dest));
    });

    it('follows redirects', async () => {
        const content = Buffer.from('redirected content');
        testServer.addRedirect('/old', `${baseUrl}/new`);
        testServer.addFileRoute('/new', content);

        const dest = join(TEST_TMP, 'redirected.bin');
        await downloadFile(`${baseUrl}/old`, dest);
        assert.ok(existsSync(dest));
    });

    it('rejects on HTTP error', async () => {
        const dest = join(TEST_TMP, 'nope.bin');
        await assert.rejects(
            () => downloadFile(`${baseUrl}/nonexistent`, dest),
            /Download failed: HTTP 404/
        );
    });
});
