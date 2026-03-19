"""Privacy-preserving hashing for analytics event properties.

Produces truncated SHA-256 hashes so the analytics backend can correlate
repeated uses of the same file or branch without seeing the actual values.
"""

import hashlib

_HASH_PREFIX_LENGTH = 16


def hash_value(value: str) -> str:
    """Return a 16-character hex prefix of the SHA-256 hash of *value*."""
    return hashlib.sha256(value.encode()).hexdigest()[:_HASH_PREFIX_LENGTH]
