"""
Integration tests for SSL certificate handling with the CS CLI.

These tests verify that:
1. Custom CA certificates are properly converted to Java truststores
2. SSL arguments are correctly passed directly to the CLI command
3. The CS CLI can be invoked with the SSL configuration

Note: GraalVM native images (like the CS CLI) don't read _JAVA_OPTIONS,
so SSL arguments must be passed directly as CLI arguments.

To run these tests locally:
    cd src && python -m unittest utils.test_ssl_integration -v

Requirements:
    - CS CLI binary must be available (downloaded or in PATH)
    - cryptography package must be installed

These tests are skipped if the CS CLI is not available.
"""
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from .platform_details import (
    _create_truststore_from_pem,
    get_ssl_cli_args,
    get_platform_details,
)
from .code_health_analysis import cs_cli_path, run_local_tool, _is_cs_cli_command


# Test CA certificate for SSL testing
TEST_CA_CERT_PEM = b"""-----BEGIN CERTIFICATE-----
MIIDPzCCAiegAwIBAgIUdGj465l77xx7Je8KqOESIqx9zXYwDQYJKoZIhvcNAQEL
BQAwTzELMAkGA1UEBhMCVVMxDTALBgNVBAgMBFRlc3QxDTALBgNVBAcMBFRlc3Qx
EDAOBgNVBAoMB1Rlc3QgQ0ExEDAOBgNVBAMMB1Rlc3QgQ0EwHhcNMjYwMTE2MDky
OTQ5WhcNMjcwMTE2MDkyOTQ5WjBPMQswCQYDVQQGEwJVUzENMAsGA1UECAwEVGVz
dDENMAsGA1UEBwwEVGVzdDEQMA4GA1UECgwHVGVzdCBDQTEQMA4GA1UEAwwHVGVz
dCBDQTCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAMqoClSXXim/fiI9
Lc3X/4D4rHK6cWAnKVPA+CetSJiGrMrfeJZMSTWUv19M8aKlmbZsQxN4X4neycWE
UxH9y3XaqV9grmGvutTgw98t6fhawevGrjmcA+ygQ5S37reRQOHtc9ob51b8b9Rr
nyE8qIU2dkZ115VpFN+/woG2LG23iGj2dJ3AaZc/R8X0UQu5tQCDwTOeO/zMWPGG
xjzDpnFs4u7IAwPECEgEuxHH8PHapUoc0d+Aq/wBKM015qdohoaydrztzXp6DKJ5
RBv/cn+lTpFdvJQS0CceIo+hOUa46ONq63VM3SQhT7enOWToONBxrZpof18bITFd
2h4XxoMCAwEAAaMTMBEwDwYDVR0TAQH/BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOC
AQEAHDWTjJILOtrCBRFksVyvniUGFR8ioz2cE4R8xcKAFxNOPKLuxwm+ilbUBX3A
8VOCJjR6IimsLMhAUEi5FGDiVVhOwIp1+pULEigTG7r72yOCr2xnw8NrX9UbJNnx
rlyCjEN9URBpriiGGegixH6AoLVW0SjEsJ7CgfqmfWzKU+nsPIunvePtFhSw5jHC
mHwYTxYcxYW33TK9qQxs119A9+qG5Z+cJlDtYrfHirHwPZQeuQ25jhKE5FUUiuiq
iblIIstcPF4n6wQ0ieNajmj5nHXQEypkek8D/ANbwwhlVQ3u/hldcAyj4qD7G5oJ
sC0Nc9QdNQt5Tos5Je5S7CWL0w==
-----END CERTIFICATE-----"""


def get_cli_path() -> str | None:
    """Get the CS CLI path if available, otherwise return None."""
    # Check environment variable override first
    if os.getenv("CS_CLI_PATH"):
        env_path = os.getenv("CS_CLI_PATH")
        if env_path and Path(env_path).exists():
            return env_path

    # Check project root (where bundled binary lives during development)
    project_root = Path(__file__).parent.parent.parent
    platform = get_platform_details()
    binary_name = platform.get_cli_binary_name()
    project_root_cli = project_root / binary_name
    if project_root_cli.exists():
        return str(project_root_cli)

    # Fall back to standard cs_cli_path resolution
    try:
        cli_path = cs_cli_path(platform)
        if cli_path and Path(cli_path).exists():
            return cli_path
    except Exception:
        pass
    return None


def is_cli_available() -> bool:
    """Check if the CS CLI is available for testing."""
    return get_cli_path() is not None


def download_cli_for_platform() -> str | None:
    """
    Download the CS CLI for the current platform.
    
    Returns the path to the downloaded CLI, or None if download fails.
    This mirrors the download process used in the GitHub workflows.
    """
    import urllib.request
    import zipfile
    
    platform = sys.platform
    cli_urls = {
        'darwin': 'https://downloads.codescene.io/enterprise/cli/cs-macos-aarch64-latest.zip',
        'linux': 'https://downloads.codescene.io/enterprise/cli/cs-linux-amd64-latest.zip',
        'win32': 'https://downloads.codescene.io/enterprise/cli/cs-windows-amd64-latest.zip',
    }
    
    # Detect architecture for macOS
    if platform == 'darwin':
        import struct
        is_arm = struct.calcsize('P') * 8 == 64 and os.uname().machine == 'arm64'
        if not is_arm:
            cli_urls['darwin'] = 'https://downloads.codescene.io/enterprise/cli/cs-macos-amd64-latest.zip'
    
    url = cli_urls.get(platform)
    if not url:
        return None
    
    try:
        # Download to temp directory
        temp_dir = tempfile.mkdtemp(prefix='cs-cli-test-')
        zip_path = os.path.join(temp_dir, 'cs-cli.zip')
        
        urllib.request.urlretrieve(url, zip_path)
        
        # Extract
        with zipfile.ZipFile(zip_path, 'r') as zip_ref:
            zip_ref.extractall(temp_dir)
        
        # Find the CLI binary
        binary_name = 'cs.exe' if platform == 'win32' else 'cs'
        cli_path = os.path.join(temp_dir, binary_name)
        
        if os.path.exists(cli_path):
            os.chmod(cli_path, 0o755)
            return cli_path
        
        return None
    except Exception as e:
        print(f"Failed to download CLI: {e}")
        return None


@unittest.skipUnless(is_cli_available(), "CS CLI not available - run TestSSLIntegrationWithDownload instead")
class TestSSLIntegration(unittest.TestCase):
    """
    Integration tests for SSL certificate handling with the CS CLI.
    
    These tests require the CS CLI to be available locally.
    If the CLI is not available, use TestSSLIntegrationWithDownload instead,
    which will download the CLI automatically.
    """
    
    def setUp(self):
        """Save original environment and set up test certificate."""
        self.original_env = os.environ.copy()
        # Clear SSL-related env vars
        for key in ['REQUESTS_CA_BUNDLE', 'SSL_CERT_FILE', 'CURL_CA_BUNDLE', '_JAVA_OPTIONS']:
            os.environ.pop(key, None)
    
    def tearDown(self):
        """Restore original environment."""
        os.environ.clear()
        os.environ.update(self.original_env)
    
    def test_truststore_is_valid_pkcs12(self):
        """Verify that the created truststore is a valid PKCS12 file."""
        with tempfile.NamedTemporaryFile(suffix='.pem', delete=False) as f:
            f.write(TEST_CA_CERT_PEM)
            pem_path = f.name
        
        try:
            truststore_path = _create_truststore_from_pem(pem_path)
            self.assertIsNotNone(truststore_path)
            assert truststore_path is not None
            
            # Verify the file exists and has content
            self.assertTrue(os.path.exists(truststore_path))
            self.assertGreater(os.path.getsize(truststore_path), 0)
            
            # Verify it's a valid PKCS12 by trying to load it
            from cryptography.hazmat.primitives.serialization import pkcs12
            with open(truststore_path, 'rb') as f:
                p12_data = f.read()
            
            # This will raise an exception if the PKCS12 is invalid
            pkcs12.load_pkcs12(p12_data, b"changeit")
            
            # Cleanup
            os.unlink(truststore_path)
        finally:
            os.unlink(pem_path)
    
    def test_java_options_include_truststore_path(self):
        """Verify that SSL args include the truststore configuration."""
        with tempfile.NamedTemporaryFile(suffix='.pem', delete=False) as f:
            f.write(TEST_CA_CERT_PEM)
            pem_path = f.name
        
        try:
            os.environ['REQUESTS_CA_BUNDLE'] = pem_path
            
            args = get_ssl_cli_args()
            
            self.assertEqual(len(args), 3)
            self.assertTrue(any('-Djavax.net.ssl.trustStore=' in arg for arg in args))
            self.assertIn('-Djavax.net.ssl.trustStoreType=PKCS12', args)
            self.assertIn('-Djavax.net.ssl.trustStorePassword=changeit', args)
            
            # Extract truststore path and verify it exists
            for arg in args:
                if arg.startswith('-Djavax.net.ssl.trustStore='):
                    truststore_path = arg.split('=', 1)[1]
                    self.assertTrue(os.path.exists(truststore_path))
                    # Cleanup
                    os.unlink(truststore_path)
                    break
        finally:
            os.unlink(pem_path)
    
    def test_cli_receives_ssl_args_in_command(self):
        """Verify that the CLI is invoked with SSL args passed directly in command."""
        with tempfile.NamedTemporaryFile(suffix='.pem', delete=False) as f:
            f.write(TEST_CA_CERT_PEM)
            pem_path = f.name
        
        try:
            os.environ['REQUESTS_CA_BUNDLE'] = pem_path
            
            captured_command = []
            original_run = subprocess.run
            
            def mock_run(command, **kwargs):
                captured_command.extend(command)
                # Return a mock result for --version
                result = mock.MagicMock()
                result.returncode = 0
                result.stdout = '{"version": "test"}'
                result.stderr = ''
                return result
            
            cli_path = get_cli_path()
            assert cli_path is not None
            
            with mock.patch('utils.code_health_analysis.subprocess.run', side_effect=mock_run):
                try:
                    run_local_tool([cli_path, '--version'])
                except Exception:
                    pass  # We just want to capture the command
            
            # Verify SSL args are in the command
            command_str = ' '.join(captured_command)
            self.assertIn('-Djavax.net.ssl.trustStore=', command_str)
            self.assertIn('-Djavax.net.ssl.trustStoreType=PKCS12', command_str)
            
            # Verify SSL args come after CLI path but before subcommand
            cli_idx = captured_command.index(cli_path)
            version_idx = captured_command.index('--version')
            ssl_arg_idx = next(i for i, arg in enumerate(captured_command) if '-Djavax.net.ssl.trustStore=' in arg)
            self.assertLess(cli_idx, ssl_arg_idx)
            self.assertLess(ssl_arg_idx, version_idx)
        finally:
            os.unlink(pem_path)
    
    def test_is_cs_cli_command_detection(self):
        """Verify that CS CLI command detection works correctly."""
        # Should be detected as CS CLI
        self.assertTrue(_is_cs_cli_command('/path/to/cs'))
        self.assertTrue(_is_cs_cli_command('/path/to/cs.exe'))
        self.assertTrue(_is_cs_cli_command('cs'))
        self.assertTrue(_is_cs_cli_command('cs.exe'))
        self.assertTrue(_is_cs_cli_command('/root/.local/bin/cs'))
        self.assertTrue(_is_cs_cli_command('C:\\Program Files\\cs.exe'))
        
        # Should NOT be detected as CS CLI
        self.assertFalse(_is_cs_cli_command('git'))
        self.assertFalse(_is_cs_cli_command('/usr/bin/python'))
        self.assertFalse(_is_cs_cli_command(''))
        self.assertFalse(_is_cs_cli_command(None))
    
    def test_cli_help_works_with_ssl_config(self):
        """Verify that the CLI can be invoked with SSL configuration."""
        with tempfile.NamedTemporaryFile(suffix='.pem', delete=False) as f:
            f.write(TEST_CA_CERT_PEM)
            pem_path = f.name
        
        try:
            os.environ['REQUESTS_CA_BUNDLE'] = pem_path
            
            cli_path = get_cli_path()
            assert cli_path is not None
            
            # Get SSL args
            ssl_args = get_ssl_cli_args()
            
            # Run the CLI with --help to verify it starts correctly
            # This tests that Java can read our truststore without errors
            result = subprocess.run(
                [cli_path] + ssl_args + ['--help'],
                capture_output=True,
                text=True,
                encoding="utf-8",
                env={
                    **os.environ,
                    'CS_CONTEXT': 'mcp-server',
                },
                timeout=30,
            )
            
            # The CLI should start and show help (exit code 0)
            # or show an error about missing args (but NOT a Java/SSL error)
            self.assertNotIn('trustStore', result.stderr.lower())
            self.assertNotIn('pkcs12', result.stderr.lower())
            self.assertNotIn('keystore', result.stderr.lower())
        finally:
            os.unlink(pem_path)


class TestSSLIntegrationWithDownload(unittest.TestCase):
    """
    Integration tests that download the CLI if not available.
    
    These tests are slower as they may download the CLI binary.
    Run with: python -m unittest utils.test_ssl_integration.TestSSLIntegrationWithDownload -v
    """
    
    @classmethod
    def setUpClass(cls):
        """Download CLI if not available."""
        cls.downloaded_cli = None
        if not is_cli_available():
            print("CS CLI not found locally, attempting download...")
            cls.downloaded_cli = download_cli_for_platform()
            if cls.downloaded_cli:
                # Set env var so cs_cli_path finds it
                os.environ['CS_CLI_PATH'] = cls.downloaded_cli
                print(f"Downloaded CLI to: {cls.downloaded_cli}")
            else:
                print("Failed to download CLI, some tests will be skipped")
    
    @classmethod
    def tearDownClass(cls):
        """Clean up downloaded CLI."""
        if cls.downloaded_cli:
            try:
                temp_dir = os.path.dirname(cls.downloaded_cli)
                import shutil
                shutil.rmtree(temp_dir, ignore_errors=True)
            except Exception:
                pass
            os.environ.pop('CS_CLI_PATH', None)
    
    def setUp(self):
        """Save original environment."""
        self.original_env = os.environ.copy()
        for key in ['REQUESTS_CA_BUNDLE', 'SSL_CERT_FILE', 'CURL_CA_BUNDLE']:
            os.environ.pop(key, None)
    
    def tearDown(self):
        """Restore original environment."""
        # Preserve CS_CLI_PATH if we downloaded the CLI
        cli_path = self.__class__.downloaded_cli
        os.environ.clear()
        os.environ.update(self.original_env)
        if cli_path:
            os.environ['CS_CLI_PATH'] = cli_path
    
    @unittest.skipUnless(is_cli_available() or download_cli_for_platform(), "Cannot get CS CLI")
    def test_cli_version_with_custom_truststore(self):
        """Test that CLI can run with custom SSL truststore configured."""
        with tempfile.NamedTemporaryFile(suffix='.pem', delete=False) as f:
            f.write(TEST_CA_CERT_PEM)
            pem_path = f.name
        
        try:
            os.environ['REQUESTS_CA_BUNDLE'] = pem_path
            ssl_args = get_ssl_cli_args()
            
            cli_path = get_cli_path()
            if not cli_path:
                self.skipTest("CLI not available")
            
            # Run CLI with our SSL config (args passed directly, not via env)
            env = os.environ.copy()
            env['CS_CONTEXT'] = 'mcp-server'
            
            result = subprocess.run(
                [cli_path] + ssl_args + ['--help'],
                capture_output=True,
                text=True,
                encoding="utf-8",
                env=env,
                timeout=60,
            )
            
            # Should not have Java SSL errors
            combined_output = result.stdout + result.stderr
            self.assertNotIn('PKCS12', combined_output)
            self.assertNotIn('trustStore', combined_output)
            self.assertNotIn('KeyStoreException', combined_output)
        finally:
            os.unlink(pem_path)


if __name__ == '__main__':
    unittest.main()
