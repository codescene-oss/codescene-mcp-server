use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64URL;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::errors::LicenseError;

const PUBLIC_KEY_BYTES: &[u8; 32] = &[
    0x22, 0xb5, 0x08, 0x1e, 0xf1, 0x1f, 0x83, 0xa2, 0x63, 0x07, 0x41, 0x2f, 0x02, 0x3f, 0x3e, 0x2b,
    0xdd, 0x14, 0xb8, 0x24, 0x01, 0xd0, 0xdd, 0x8a, 0xa6, 0x90, 0x88, 0x07, 0xcf, 0x16, 0x17, 0x7a,
];

pub fn is_standalone_license(token: &str) -> bool {
    verify_jwt(token).is_ok()
}

fn verify_jwt(token: &str) -> Result<(), LicenseError> {
    let token = token.trim();

    // JWT must have exactly three dot-separated parts.
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(LicenseError::InvalidFormat);
    }

    // The Ed25519 signature is over the ASCII bytes of "header.payload".
    let sig_input_end = parts[0].len() + 1 + parts[1].len();
    let signing_input = &token.as_bytes()[..sig_input_end];

    // Decode the signature (base64url, no padding).
    let sig_bytes = BASE64URL
        .decode(parts[2])
        .map_err(|_| LicenseError::InvalidFormat)?;

    let signature = Signature::from_bytes(
        sig_bytes
            .as_slice()
            .try_into()
            .map_err(|_| LicenseError::InvalidFormat)?,
    );

    let verifying_key =
        VerifyingKey::from_bytes(PUBLIC_KEY_BYTES).map_err(|_| LicenseError::InvalidSignature)?;

    verifying_key
        .verify(signing_input, &signature)
        .map_err(|_| LicenseError::InvalidSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Valid JWT signed with the production private key.
    const VALID_JWT: &str = concat!(
        "eyJhbGciOiJFZERTQSIsImtpZCI6ImNzbWNwIiwidHlwIjoiSldTIn0.",
        "eyJpc3MiOiJjb2Rlc2NlbmUtbWNwIiwiYXVkIjoiY29kZXNjZW5lLWNsaSIs",
        "ImlhdCI6MTc3MTk0NTM1NSwiZXhwIjoxNzcyMjgxNjUzLCJzdWIiOiIyYTM5",
        "NDAyNS1kYjg2LTQwMDAtYWE0NS1lODY2Yjk5YmJhMzcifQ.",
        "V0UxjlS1ZK-hcF1M7edu6GfvMAjv1XukFe8m6iHzS9guh_4rqu4HGbRTzl21",
        "7qMemCjwyHtAG9pO6NUu3SWbCQ",
    );

    #[test]
    fn accepts_valid_standalone_jwt() {
        assert!(is_standalone_license(VALID_JWT));
    }

    #[test]
    fn rejects_empty_token() {
        assert!(!is_standalone_license(""));
    }

    #[test]
    fn rejects_pat_prefix() {
        assert!(!is_standalone_license("cst_abc123def456"));
    }

    #[test]
    fn rejects_random_string() {
        assert!(!is_standalone_license("not-a-real-token"));
    }

    #[test]
    fn rejects_wrong_part_count() {
        assert!(!is_standalone_license("only.one"));
        assert!(!is_standalone_license("a.b.c.d.e"));
    }

    #[test]
    fn rejects_invalid_signature() {
        assert!(!is_standalone_license("aaa.bbb.ccc"));
    }
}
