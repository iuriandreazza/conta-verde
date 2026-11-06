use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

/// The HMAC-SHA256 of `body` under `secret`, in lower case hex: what providers that sign a webhook put in
/// a header.
pub fn hmac_sha256_hex(secret: &[u8], body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC takes a key of any length");
    mac.update(body);
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Whether `signature` (hex, any case) is the HMAC-SHA256 of `body` under `secret`, compared in constant
/// time: how long the comparison takes must not tell how much of a forged signature was right.
pub fn is_valid_signature(secret: &[u8], body: &[u8], signature: &str) -> bool {
    let Some(expected) = decode_hex(signature.trim()) else {
        return false;
    };
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC takes a key of any length");
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_published_hmac_sha256_test_vector() {
        // RFC 4231, test case 2.
        assert_eq!(
            hmac_sha256_hex(b"Jefe", b"what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn accepts_the_right_signature_in_any_case_and_nothing_else() {
        let signature = hmac_sha256_hex(b"secret", b"body");
        assert!(is_valid_signature(b"secret", b"body", &signature));
        assert!(is_valid_signature(
            b"secret",
            b"body",
            &signature.to_uppercase()
        ));
        assert!(!is_valid_signature(b"other", b"body", &signature));
        assert!(!is_valid_signature(b"secret", b"bodies", &signature));
        for bad in ["", "zz", "abc", &signature[..62], "é"] {
            assert!(!is_valid_signature(b"secret", b"body", bad), "{bad}");
        }
    }
}
