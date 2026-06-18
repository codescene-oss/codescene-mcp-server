import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { mkdirSync, writeFileSync, rmSync, existsSync, readdirSync, renameSync, readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { TARGETS, getVersion, getDownloadUrl, isRedirect, downloadFile, downloadForTarget, extractZip, downloadCompressed, BIN_DIR } from '../scripts/download-binary.js';

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

describe('getVersion', () => {
    const origVersion = process.env.CS_MCP_VERSION;

    afterEach(() => {
        if (origVersion === undefined) {
            delete process.env.CS_MCP_VERSION;
        } else {
            process.env.CS_MCP_VERSION = origVersion;
        }
    });

    it('returns CS_MCP_VERSION env var when set', () => {
        process.env.CS_MCP_VERSION = '9.8.7';
        assert.equal(getVersion(), '9.8.7');
    });

    it('returns a valid semver-like string from env', () => {
        process.env.CS_MCP_VERSION = '1.2.3';
        const version = getVersion();
        assert.match(version, /^\d+\.\d+\.\d+/);
    });
});

describe('downloadForTarget', () => {
    let testServer;
    let baseUrl;
    let origBinDir;
    const originalBaseUrl = process.env.CS_MCP_DOWNLOAD_BASE_URL;
    const originalVersion = process.env.CS_MCP_VERSION;

    beforeEach(async () => {
        testServer = createTestServer();
        baseUrl = await testServer.listen();
        mkdirSync(TEST_TMP, { recursive: true });
        process.env.CS_MCP_VERSION = '99.0.0';
    });

    afterEach(async () => {
        await testServer.close();
        rmSync(TEST_TMP, { recursive: true, force: true });
        // Clean up any files created in BIN_DIR during tests
        if (existsSync(BIN_DIR)) {
            for (const f of readdirSync(BIN_DIR)) {
                if (f.startsWith('.test-')) {
                    rmSync(join(BIN_DIR, f), { force: true });
                }
            }
        }
        if (originalBaseUrl === undefined) {
            delete process.env.CS_MCP_DOWNLOAD_BASE_URL;
        } else {
            process.env.CS_MCP_DOWNLOAD_BASE_URL = originalBaseUrl;
        }
        if (originalVersion === undefined) {
            delete process.env.CS_MCP_VERSION;
        } else {
            process.env.CS_MCP_VERSION = originalVersion;
        }
    });

    it('skips download when binary already exists', async () => {
        // Create a fake binary in BIN_DIR
        mkdirSync(BIN_DIR, { recursive: true });
        const info = TARGETS['win32-x64']; // Uncompressed is simplest
        const binaryDest = join(BIN_DIR, info.binary);
        const existed = existsSync(binaryDest);

        if (!existed) {
            writeFileSync(binaryDest, 'existing-binary');
        }

        // Set up a server that should NOT be hit
        let serverHit = false;
        testServer.addRoute(`/MCP-${getVersion()}/${info.asset}`, (_req, res) => {
            serverHit = true;
            res.writeHead(200);
            res.end('data');
        });
        process.env.CS_MCP_DOWNLOAD_BASE_URL = baseUrl;

        await downloadForTarget('win32-x64');

        assert.equal(serverHit, false, 'Server should not be hit when binary exists');

        if (!existed) {
            rmSync(binaryDest, { force: true });
        }
    });

    it('downloads uncompressed binary for win32-x64', async () => {
        const info = TARGETS['win32-x64'];
        const binaryDest = join(BIN_DIR, info.binary);
        const existed = existsSync(binaryDest);

        // Temporarily rename existing binary if it exists
        if (existed) {
            renameSync(binaryDest, binaryDest + '.bak');
        }

        const content = Buffer.from('fake-windows-binary');
        testServer.addFileRoute(`/MCP-${getVersion()}/${info.asset}`, content);
        process.env.CS_MCP_DOWNLOAD_BASE_URL = baseUrl;

        try {
            await downloadForTarget('win32-x64');
            assert.ok(existsSync(binaryDest), 'Binary should be downloaded');
        } finally {
            rmSync(binaryDest, { force: true });
            if (existed) {
                renameSync(binaryDest + '.bak', binaryDest);
            }
        }
    });

    it('downloads compressed binary for linux-x64', async () => {
        const info = TARGETS['linux-x64'];
        const binaryDest = join(BIN_DIR, info.binary);
        const existed = existsSync(binaryDest);

        // Temporarily rename existing binary if it exists
        if (existed) {
            renameSync(binaryDest, binaryDest + '.bak');
        }

        // Create a zip containing a file named cs-mcp-linux-amd64
        const tmpSrc = join(TEST_TMP, info.binary);
        writeFileSync(tmpSrc, 'fake-linux-binary');
        const zipPath = join(TEST_TMP, info.asset);
        execFileSync('zip', ['-j', zipPath, tmpSrc], { stdio: 'pipe' });
        const zipContent = readFileSync(zipPath);

        testServer.addFileRoute(`/MCP-${getVersion()}/${info.asset}`, zipContent);
        process.env.CS_MCP_DOWNLOAD_BASE_URL = baseUrl;

        try {
            await downloadForTarget('linux-x64');
            assert.ok(existsSync(binaryDest), 'Binary should be downloaded and extracted');
        } finally {
            rmSync(binaryDest, { force: true });
            if (existed) {
                renameSync(binaryDest + '.bak', binaryDest);
            }
        }
    });
});

describe('extractZip', () => {
    beforeEach(() => {
        mkdirSync(TEST_TMP, { recursive: true });
    });

    afterEach(() => {
        rmSync(TEST_TMP, { recursive: true, force: true });
    });

    it('extracts a zip file to the destination directory', () => {
        // Create a test file and zip it
        const testFile = join(TEST_TMP, 'hello.txt');
        writeFileSync(testFile, 'hello world');

        const zipPath = join(TEST_TMP, 'test.zip');
        execFileSync('zip', ['-j', zipPath, testFile], { stdio: 'pipe' });

        const extractDir = join(TEST_TMP, 'extracted');
        mkdirSync(extractDir, { recursive: true });

        extractZip(zipPath, extractDir);

        assert.ok(existsSync(join(extractDir, 'hello.txt')));
        const content = readFileSync(join(extractDir, 'hello.txt'), 'utf8');
        assert.equal(content, 'hello world');
    });
});

describe('downloadCompressed', () => {
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
        // Clean up test artifacts from BIN_DIR
        for (const f of ['test-asset.zip', 'test-binary', 'cs-mcp-test-binary']) {
            rmSync(join(BIN_DIR, f), { force: true });
        }
    });

    it('downloads and extracts a compressed asset', async () => {
        // Create a zip containing a binary-like file
        const srcFile = join(TEST_TMP, 'cs-mcp-test-binary');
        writeFileSync(srcFile, 'compressed-binary-content');

        const zipPath = join(TEST_TMP, 'test-asset.zip');
        execFileSync('zip', ['-j', zipPath, srcFile], { stdio: 'pipe' });

        const zipContent = readFileSync(zipPath);
        testServer.addFileRoute('/asset.zip', zipContent);

        mkdirSync(BIN_DIR, { recursive: true });

        const info = { asset: 'test-asset.zip', binary: 'test-binary', compressed: true };
        const binaryDest = join(BIN_DIR, 'test-binary');

        await downloadCompressed(`${baseUrl}/asset.zip`, info, binaryDest);

        // The binary should be renamed from cs-mcp-test-binary to test-binary
        assert.ok(existsSync(binaryDest), 'Binary should be extracted and renamed');

        // Clean up
        rmSync(binaryDest, { force: true });
    });

    it('cleans up the zip file after extraction', async () => {
        const srcFile = join(TEST_TMP, 'cs-mcp-cleanup-test');
        writeFileSync(srcFile, 'data');

        const zipPath = join(TEST_TMP, 'cleanup.zip');
        execFileSync('zip', ['-j', zipPath, srcFile], { stdio: 'pipe' });

        const zipContent = readFileSync(zipPath);
        testServer.addFileRoute('/cleanup.zip', zipContent);

        mkdirSync(BIN_DIR, { recursive: true });

        const info = { asset: 'cleanup.zip', binary: 'cleanup-binary', compressed: true };
        const binaryDest = join(BIN_DIR, 'cleanup-binary');

        await downloadCompressed(`${baseUrl}/cleanup.zip`, info, binaryDest);

        // The zip should be cleaned up
        assert.ok(!existsSync(join(BIN_DIR, 'cleanup.zip')), 'Zip file should be cleaned up');

        // Clean up
        rmSync(binaryDest, { force: true });
        rmSync(join(BIN_DIR, 'cs-mcp-cleanup-test'), { force: true });
    });
});
