/// SHA-256 truncated hashing — mirrors Python's `hashing.py`.
///
/// Produces a 16-character hex digest used for non-PII analytics properties
/// (e.g., hashed file paths).

use sha2::{Digest, Sha256};

/// Return the first 16 hex characters of the SHA-256 digest of `input`.
pub fn truncated_sha256(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    hex_encode_truncated(&result, 16)
}

fn hex_encode_truncated(bytes: &[u8], max_hex_chars: usize) -> String {
    let byte_count = (max_hex_chars + 1) / 2;
    let mut out = String::with_capacity(max_hex_chars);
    for &b in bytes.iter().take(byte_count) {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out.truncate(max_hex_chars);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_16_char_hex() {
        let hash = truncated_sha256("hello");
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn deterministic() {
        assert_eq!(truncated_sha256("test"), truncated_sha256("test"));
    }

    #[test]
    fn different_inputs_differ() {
        assert_ne!(truncated_sha256("a"), truncated_sha256("b"));
    }
}
