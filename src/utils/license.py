import os

from authlib.jose import JsonWebToken
from authlib.jose.errors import BadSignatureError, DecodeError

_PUBLIC_KEY_PEM = b"""-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAIrUIHvEfg6JjB0EvAj8+K90UuCQB0N2KppCIB88WF3o=
-----END PUBLIC KEY-----"""


def is_standalone_license(token: str | None) -> bool:
    """Check if a token is a standalone MCP license (Ed25519-signed JWT).

    Returns True only when the token is a valid JWT whose signature can be
    verified with the embedded Ed25519 public key.  Returns False for PATs,
    empty/None values, malformed tokens, and JWTs signed by other keys.
    """
    if not token:
        return False

    if token.count(".") != 2:
        return False

    try:
        jwt = JsonWebToken(["EdDSA"])
        jwt.decode(token, _PUBLIC_KEY_PEM)
        return True
    except (DecodeError, BadSignatureError):
        return False
    except Exception:
        # Fail-safe: unknown errors default to non-standalone (register all tools).
        return False


def is_standalone_token() -> bool:
    """Check if the CS_ACCESS_TOKEN environment variable is a standalone MCP JWT."""
    return is_standalone_license(os.getenv("CS_ACCESS_TOKEN"))
