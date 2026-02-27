import os
import unittest
from unittest import mock

from authlib.jose import JsonWebToken
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

from .license import _PUBLIC_KEY_PEM, is_standalone_license, is_standalone_token

# A valid JWT signed with the production private key (signature-only check,
# expiry is not enforced by is_standalone_license).
_VALID_STANDALONE_JWT = (
    "eyJhbGciOiJFZERTQSIsImtpZCI6ImNzbWNwIiwidHlwIjoiSldTIn0."
    "eyJpc3MiOiJjb2Rlc2NlbmUtbWNwIiwiYXVkIjoiY29kZXNjZW5lLWNsaSIs"
    "ImlhdCI6MTc3MTk0NTM1NSwiZXhwIjoxNzcyMjgxNjUzLCJzdWIiOiIyYTM5"
    "NDAyNS1kYjg2LTQwMDAtYWE0NS1lODY2Yjk5YmJhMzcifQ."
    "V0UxjlS1ZK-hcF1M7edu6GfvMAjv1XukFe8m6iHzS9guh_4rqu4HGbRTzl21"
    "7qMemCjwyHtAG9pO6NUu3SWbCQ"
)


def _sign_jwt_with_random_key(claims: dict) -> str:
    """Create an EdDSA-signed JWT using a freshly generated key (wrong key)."""
    private_key = Ed25519PrivateKey.generate()
    private_pem = private_key.private_bytes(
        serialization.Encoding.PEM,
        serialization.PrivateFormat.PKCS8,
        serialization.NoEncryption(),
    )
    jwt_instance = JsonWebToken(["EdDSA"])
    return jwt_instance.encode({"alg": "EdDSA"}, claims, private_pem).decode()


class TestIsStandaloneLicense(unittest.TestCase):
    """Tests for is_standalone_license() against the production public key."""

    def test_valid_standalone_jwt(self):
        self.assertTrue(is_standalone_license(_VALID_STANDALONE_JWT))

    def test_pat_returns_false(self):
        self.assertFalse(is_standalone_license("cst_abc123def456"))

    def test_none_returns_false(self):
        self.assertFalse(is_standalone_license(None))

    def test_empty_string_returns_false(self):
        self.assertFalse(is_standalone_license(""))

    def test_jwt_signed_by_wrong_key_returns_false(self):
        token = _sign_jwt_with_random_key({"sub": "attacker"})
        self.assertFalse(is_standalone_license(token))

    def test_malformed_jwt_returns_false(self):
        self.assertFalse(is_standalone_license("not.a.jwt"))

    def test_base64_garbage_with_dots_returns_false(self):
        self.assertFalse(is_standalone_license("aaa.bbb.ccc"))

    def test_single_dot_returns_false(self):
        self.assertFalse(is_standalone_license("only.one"))

    def test_four_dots_returns_false(self):
        self.assertFalse(is_standalone_license("a.b.c.d.e"))


class TestIsStandaloneToken(unittest.TestCase):
    """Tests for is_standalone_token() which reads CS_ACCESS_TOKEN from env."""

    @mock.patch.dict(os.environ, {"CS_ACCESS_TOKEN": _VALID_STANDALONE_JWT})
    def test_standalone_jwt_in_env(self):
        self.assertTrue(is_standalone_token())

    @mock.patch.dict(os.environ, {"CS_ACCESS_TOKEN": "cst_regular_pat_token"})
    def test_pat_in_env(self):
        self.assertFalse(is_standalone_token())

    @mock.patch.dict(os.environ, {}, clear=True)
    def test_no_token_in_env(self):
        self.assertFalse(is_standalone_token())


class TestPublicKeyIsLoadable(unittest.TestCase):
    """Verify the embedded production public key is a valid Ed25519 PEM."""

    def test_production_public_key_is_valid_ed25519(self):
        from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
        from cryptography.hazmat.primitives.serialization import load_pem_public_key

        key = load_pem_public_key(_PUBLIC_KEY_PEM)
        self.assertIsInstance(key, Ed25519PublicKey)
