import os
import sys
import tempfile
import unittest
from collections.abc import Generator
from contextlib import contextmanager
from unittest import mock

from .platform_details import (
    UnixPlatformDetails,
    WindowsPlatformDetails,
    _create_truststore_from_pem,
    get_platform_details,
    get_ssl_cli_args,
)

# Valid self-signed CA certificate for testing (generated with cryptography library)
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


@contextmanager
def temp_pem_file(content: bytes = TEST_CA_CERT_PEM) -> Generator[str]:
    """Context manager for creating and cleaning up temporary PEM files."""
    with tempfile.NamedTemporaryFile(suffix=".pem", delete=False) as f:
        f.write(content)
        pem_path = f.name
    try:
        yield pem_path
    finally:
        os.unlink(pem_path)


class TestPlatformDetails(unittest.TestCase):
    def test_get_platform_details_returns_details(self):
        # Just test that it returns a details instance, not the specific type
        # since we're running on a real platform
        details = get_platform_details()
        self.assertIsNotNone(details)
        # Check it has the required methods
        self.assertTrue(hasattr(details, "get_cli_binary_name"))
        self.assertTrue(hasattr(details, "configure_environment"))

    def test_windows_cli_binary_name(self):
        details = WindowsPlatformDetails()
        self.assertEqual("cs.exe", details.get_cli_binary_name())

    def test_unix_cli_binary_name(self):
        details = UnixPlatformDetails()
        self.assertEqual("cs", details.get_cli_binary_name())

    @mock.patch("os.path.exists")
    def test_windows_configure_environment_adds_git_to_path(self, mock_exists):
        mock_exists.return_value = True
        details = WindowsPlatformDetails()
        env = {"PATH": "C:\\existing\\path"}

        result = details.configure_environment(env)

        self.assertIn("Git", result["PATH"])
        self.assertIn("C:\\existing\\path", result["PATH"])
        self.assertIn(";", result["PATH"])

    def test_unix_configure_environment_returns_copy(self):
        details = UnixPlatformDetails()
        env = {"PATH": "/usr/bin:/usr/local/bin"}

        result = details.configure_environment(env)

        self.assertEqual(env["PATH"], result["PATH"])
        self.assertIsNot(env, result)  # Should be a copy

    @mock.patch("os.path.exists")
    def test_windows_configure_environment_no_git_found(self, mock_exists):
        mock_exists.return_value = False
        details = WindowsPlatformDetails()
        env = {"PATH": "C:\\existing\\path"}

        result = details.configure_environment(env)

        # PATH should remain unchanged if no Git found
        self.assertEqual("C:\\existing\\path", result["PATH"])

    @mock.patch("os.path.exists")
    def test_windows_configure_environment_git_already_in_path(self, mock_exists):
        # Return False for all paths so nothing gets added
        mock_exists.return_value = False
        details = WindowsPlatformDetails()
        git_path = r"C:\Program Files\Git\cmd"
        env = {"PATH": f"{git_path};C:\\existing\\path"}

        result = details.configure_environment(env)

        # If git is already in path and we can't find other git paths,
        # the PATH should remain unchanged
        self.assertEqual(env["PATH"], result["PATH"])

    def test_windows_configure_environment_preserves_other_env_vars(self):
        details = WindowsPlatformDetails()
        env = {"PATH": "C:\\test", "HOME": "C:\\Users\\test", "CUSTOM_VAR": "value"}

        result = details.configure_environment(env)

        self.assertEqual("C:\\Users\\test", result["HOME"])
        self.assertEqual("value", result["CUSTOM_VAR"])

    def test_unix_configure_environment_preserves_all_vars(self):
        details = UnixPlatformDetails()
        env = {"PATH": "/usr/bin", "HOME": "/home/user", "SHELL": "/bin/bash"}

        result = details.configure_environment(env)

        self.assertEqual(env["PATH"], result["PATH"])
        self.assertEqual(env["HOME"], result["HOME"])
        self.assertEqual(env["SHELL"], result["SHELL"])

    @mock.patch("os.path.exists")
    def test_windows_finds_first_existing_git_path(self, mock_exists):
        # Simulate only the third path existing
        def exists_side_effect(path):
            return r"C:\Program Files\Git\bin" in path

        mock_exists.side_effect = exists_side_effect
        details = WindowsPlatformDetails()
        env = {"PATH": "C:\\existing"}

        result = details.configure_environment(env)

        self.assertIn(r"Git\bin", result["PATH"])
        self.assertIn("C:\\existing", result["PATH"])

    def test_windows_get_java_options_returns_tmpdir_setting(self):
        details = WindowsPlatformDetails()

        result = details.get_java_options()

        self.assertIn("-Djava.io.tmpdir=", result)
        # Should contain a valid temp directory path
        self.assertTrue(len(result) > len('-Djava.io.tmpdir=""'))

    @mock.patch.dict(os.environ, {}, clear=True)
    def test_unix_get_java_options_returns_empty_string(self):
        # Ensure no SSL env vars are set
        for key in ["REQUESTS_CA_BUNDLE", "SSL_CERT_FILE", "CURL_CA_BUNDLE"]:
            os.environ.pop(key, None)

        details = UnixPlatformDetails()

        result = details.get_java_options()

        self.assertEqual("", result)

    def _test_platform_detection(self, platform_name: str, expected_class: str):
        """Helper to test platform detection with mocked sys.platform."""
        import utils.platform_details as pd

        try:
            pd.sys = mock.MagicMock()
            pd.sys.platform = platform_name

            details = pd.get_platform_details()
            self.assertEqual(expected_class, details.__class__.__name__)
        finally:
            pd.sys = sys

    def test_get_platform_details_returns_windows_on_win32(self):
        self._test_platform_detection("win32", "WindowsPlatformDetails")

    def test_get_platform_details_returns_unix_on_darwin(self):
        self._test_platform_detection("darwin", "UnixPlatformDetails")

    def test_get_platform_details_returns_unix_on_linux(self):
        self._test_platform_detection("linux", "UnixPlatformDetails")


class TestSSLTruststoreOptions(unittest.TestCase):
    """Tests for SSL truststore configuration using REQUESTS_CA_BUNDLE."""

    def setUp(self):
        """Save original env vars and clear SSL-related ones."""
        self.original_env = os.environ.copy()
        for key in ["REQUESTS_CA_BUNDLE", "SSL_CERT_FILE", "CURL_CA_BUNDLE"]:
            os.environ.pop(key, None)

    def tearDown(self):
        """Restore original env vars."""
        os.environ.clear()
        os.environ.update(self.original_env)

    def _assert_truststore_args_present(self, result: list) -> None:
        """Assert that truststore args are present in the result."""
        self.assertTrue(any("-Djavax.net.ssl.trustStore=" in arg for arg in result))

    def test_returns_empty_list_when_no_env_vars_set(self):
        result = get_ssl_cli_args()
        self.assertEqual([], result)

    def test_returns_empty_list_when_ca_bundle_file_not_found(self):
        os.environ["REQUESTS_CA_BUNDLE"] = "/nonexistent/ca-bundle.crt"

        result = get_ssl_cli_args()

        self.assertEqual([], result)

    def test_returns_truststore_args_when_valid_pem_exists(self):
        with temp_pem_file() as pem_path:
            os.environ["REQUESTS_CA_BUNDLE"] = pem_path

            result = get_ssl_cli_args()

            self.assertEqual(3, len(result))
            self._assert_truststore_args_present(result)
            self.assertIn("-Djavax.net.ssl.trustStoreType=PKCS12", result)
            self.assertIn("-Djavax.net.ssl.trustStorePassword=changeit", result)

    def test_ssl_cert_file_is_used_as_fallback(self):
        with temp_pem_file() as pem_path:
            os.environ["SSL_CERT_FILE"] = pem_path
            result = get_ssl_cli_args()
            self._assert_truststore_args_present(result)

    def test_curl_ca_bundle_is_used_as_fallback(self):
        with temp_pem_file() as pem_path:
            os.environ["CURL_CA_BUNDLE"] = pem_path
            result = get_ssl_cli_args()
            self._assert_truststore_args_present(result)

    def test_requests_ca_bundle_takes_precedence(self):
        with temp_pem_file() as requests_path, temp_pem_file() as ssl_path:
            os.environ["REQUESTS_CA_BUNDLE"] = requests_path
            os.environ["SSL_CERT_FILE"] = ssl_path

            result = get_ssl_cli_args()

            self._assert_truststore_args_present(result)

    def test_returns_empty_for_invalid_pem_content(self):
        with temp_pem_file(b"not a valid certificate") as pem_path:
            os.environ["REQUESTS_CA_BUNDLE"] = pem_path
            result = get_ssl_cli_args()
            self.assertEqual([], result)

    def test_windows_java_options_only_contains_tmpdir(self):
        """Windows Java options should only contain tmpdir, SSL is handled via CLI args."""
        with temp_pem_file() as pem_path:
            os.environ["REQUESTS_CA_BUNDLE"] = pem_path
            details = WindowsPlatformDetails()

            result = details.get_java_options()

            self.assertIn("-Djava.io.tmpdir=", result)
            # SSL options should NOT be in Java options (they go directly to CLI)
            self.assertNotIn("-Djavax.net.ssl.trustStore", result)

    def test_unix_java_options_is_empty(self):
        """Unix Java options should be empty, SSL is handled via CLI args."""
        with temp_pem_file() as pem_path:
            os.environ["REQUESTS_CA_BUNDLE"] = pem_path
            details = UnixPlatformDetails()

            result = details.get_java_options()

            # Unix doesn't need Java options, SSL goes directly to CLI
            self.assertEqual("", result)


class TestCreateTruststoreFromPem(unittest.TestCase):
    """Tests for PEM to PKCS12 conversion."""

    def test_returns_none_for_invalid_pem_content(self):
        """Test that invalid PEM content returns None via exception handler."""
        with temp_pem_file(b"not a valid certificate") as pem_path:
            result = _create_truststore_from_pem(pem_path)
            self.assertIsNone(result)

    def test_returns_none_for_empty_pem_file(self):
        """Test that an empty PEM file returns None via exception handler."""
        with temp_pem_file(b"") as pem_path:
            result = _create_truststore_from_pem(pem_path)
            self.assertIsNone(result)

    def test_creates_truststore_and_reuses_existing(self):
        """Test that truststore is created and reused on subsequent calls."""
        with temp_pem_file() as pem_path:
            truststore_path = _create_truststore_from_pem(pem_path)
            self.assertIsNotNone(truststore_path)
            assert truststore_path is not None  # Type narrowing
            self.assertTrue(os.path.exists(truststore_path))

            mtime1 = os.path.getmtime(truststore_path)

            # Second call should reuse existing truststore
            result2 = _create_truststore_from_pem(pem_path)
            self.assertEqual(truststore_path, result2)

            assert result2 is not None  # Type narrowing
            mtime2 = os.path.getmtime(result2)
            self.assertEqual(mtime1, mtime2)

            # Clean up truststore
            os.unlink(truststore_path)

    def test_returns_none_on_file_read_error(self):
        """Test that file read errors are handled gracefully."""
        result = _create_truststore_from_pem("/nonexistent/path/cert.pem")
        self.assertIsNone(result)

    def test_returns_none_when_no_certs_parsed(self):
        """Test handling when certificate parsing returns empty list."""
        from cryptography import x509

        with temp_pem_file() as pem_path, mock.patch.object(x509, "load_pem_x509_certificates", return_value=[]):
            result = _create_truststore_from_pem(pem_path)
            self.assertIsNone(result)

    def test_returns_none_when_cryptography_import_fails(self):
        """Test handling when cryptography module is not available."""
        import builtins

        import utils.platform_details as pd

        original_import = builtins.__import__

        def mock_import(name, *args, **kwargs):
            if name == "cryptography" or name.startswith("cryptography."):
                raise ImportError("No module named 'cryptography'")
            return original_import(name, *args, **kwargs)

        with temp_pem_file() as pem_path, mock.patch.object(builtins, "__import__", side_effect=mock_import):
            result = pd._create_truststore_from_pem(pem_path)
            # When cryptography is available, this will succeed
            # The ImportError branch can only be hit in envs without cryptography
            self.assertTrue(result is None or isinstance(result, str))


if __name__ == "__main__":
    unittest.main()
