use conta_verde_application::{SecretCipher, SecretError};
use conta_verde_domain::{CompanyId, SecretText};
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};

/// The first byte of what is stored: the version of the format, so it can change.
const VERSION: u8 = 1;
const KEY_BYTES: usize = 32;

/// AES-256-GCM over a key from the environment (`FISCAL_SECRETS_KEY`, 64 hexadecimal characters) ([ADR 0085]).
///
/// What is stored is `version (1) || nonce (12) || ciphertext || tag (16)`. The nonce is **fresh and random each time**, and the
/// company and the field are the **associated data**: a ciphertext opens only for the company and field it was sealed for.
/// **Losing the key makes what was sealed unrecoverable**; changing it is not built.
pub struct RingSecretCipher {
    key: LessSafeKey,
    random: SystemRandom,
}

impl RingSecretCipher {
    /// `hex` is the key: exactly 64 hexadecimal characters. Anything else is an error that names what is wrong, never the value.
    pub fn from_hex(hex: &str) -> Result<Self, SecretError> {
        let bytes = decode_hex(hex.trim())
            .filter(|bytes| bytes.len() == KEY_BYTES)
            .ok_or_else(|| {
                SecretError(
                    "the secrets key must be 64 hexadecimal characters (32 bytes)".to_owned(),
                )
            })?;
        let key = UnboundKey::new(&AES_256_GCM, &bytes)
            .map_err(|_| SecretError("the secrets key was refused".to_owned()))?;
        Ok(Self {
            key: LessSafeKey::new(key),
            random: SystemRandom::new(),
        })
    }
}

fn associated_data(company_id: CompanyId, field: &str) -> Vec<u8> {
    format!("{}|{field}|v{VERSION}", company_id.as_uuid()).into_bytes()
}

impl SecretCipher for RingSecretCipher {
    fn is_available(&self) -> bool {
        true
    }

    fn seal(
        &self,
        company_id: CompanyId,
        field: &str,
        plaintext: &SecretText,
    ) -> Result<Vec<u8>, SecretError> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        self.random
            .fill(&mut nonce_bytes)
            .map_err(|_| SecretError("no randomness to seal with".to_owned()))?;
        let mut in_out = plaintext.expose().as_bytes().to_vec();
        self.key
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce_bytes),
                Aad::from(associated_data(company_id, field)),
                &mut in_out,
            )
            .map_err(|_| SecretError("a secret could not be sealed".to_owned()))?;
        let mut sealed = Vec::with_capacity(1 + NONCE_LEN + in_out.len());
        sealed.push(VERSION);
        sealed.extend_from_slice(&nonce_bytes);
        sealed.extend_from_slice(&in_out);
        Ok(sealed)
    }

    fn open(
        &self,
        company_id: CompanyId,
        field: &str,
        sealed: &[u8],
    ) -> Result<SecretText, SecretError> {
        let unreadable = || SecretError("a stored secret cannot be opened".to_owned());
        let (version, rest) = sealed.split_first().ok_or_else(unreadable)?;
        if *version != VERSION || rest.len() <= NONCE_LEN {
            return Err(unreadable());
        }
        let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
        let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| unreadable())?;
        let mut in_out = ciphertext.to_vec();
        let plain = self
            .key
            .open_in_place(
                nonce,
                Aad::from(associated_data(company_id, field)),
                &mut in_out,
            )
            .map_err(|_| unreadable())?;
        let text = std::str::from_utf8(plain).map_err(|_| unreadable())?;
        SecretText::new(text).map_err(|_| unreadable())
    }
}

/// What stands in when the server has no key: nothing can be sealed and nothing sealed can be opened.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoSecretCipher;

impl SecretCipher for NoSecretCipher {
    fn is_available(&self) -> bool {
        false
    }

    fn seal(&self, _: CompanyId, _: &str, _: &SecretText) -> Result<Vec<u8>, SecretError> {
        Err(SecretError("the server has no secrets key".to_owned()))
    }

    fn open(&self, _: CompanyId, _: &str, _: &[u8]) -> Result<SecretText, SecretError> {
        Err(SecretError("the server has no secrets key".to_owned()))
    }
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

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn cipher() -> RingSecretCipher {
        RingSecretCipher::from_hex(KEY).unwrap()
    }

    fn secret(text: &str) -> SecretText {
        SecretText::new(text).unwrap()
    }

    #[test]
    fn a_secret_opens_for_the_company_and_field_it_was_sealed_for_and_for_nothing_else() {
        let (company, other) = (CompanyId::generate(), CompanyId::generate());
        let sealed = cipher()
            .seal(
                company,
                "focus_token_production",
                &secret("tok_live_1234567890"),
            )
            .unwrap();
        assert_eq!(
            cipher()
                .open(company, "focus_token_production", &sealed)
                .unwrap(),
            secret("tok_live_1234567890")
        );
        assert!(
            cipher()
                .open(other, "focus_token_production", &sealed)
                .is_err()
        );
        assert!(
            cipher()
                .open(company, "focus_token_homologation", &sealed)
                .is_err()
        );
    }

    #[test]
    fn what_is_stored_holds_nothing_readable_and_differs_every_time() {
        let company = CompanyId::generate();
        let value = secret("tok_live_1234567890");
        let first = cipher().seal(company, "webhook_secret", &value).unwrap();
        let second = cipher().seal(company, "webhook_secret", &value).unwrap();
        assert_ne!(first, second, "a fresh nonce each time");
        assert_eq!(first[0], 1, "the version of the format");
        assert!(!String::from_utf8_lossy(&first).contains("tok_live"));
        assert_eq!(first.len(), 1 + 12 + "tok_live_1234567890".len() + 16);
    }

    #[test]
    fn a_changed_byte_a_cut_value_and_another_key_all_refuse_to_open() {
        let company = CompanyId::generate();
        let sealed = cipher()
            .seal(company, "nfeio_api_key", &secret("key_1234567890"))
            .unwrap();
        for at in [0, 1, 13, sealed.len() - 1] {
            let mut bad = sealed.clone();
            bad[at] ^= 1;
            assert!(
                cipher().open(company, "nfeio_api_key", &bad).is_err(),
                "byte {at}"
            );
        }
        assert!(
            cipher()
                .open(company, "nfeio_api_key", &sealed[..sealed.len() - 1])
                .is_err()
        );
        assert!(cipher().open(company, "nfeio_api_key", &[]).is_err());
        assert!(cipher().open(company, "nfeio_api_key", &[1; 10]).is_err());
        let other_key = RingSecretCipher::from_hex(
            "ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100",
        )
        .unwrap();
        assert!(other_key.open(company, "nfeio_api_key", &sealed).is_err());
    }

    #[test]
    fn the_key_is_exactly_64_hexadecimal_characters_and_its_text_never_leaks() {
        for bad in [
            "",
            "abcd",
            &KEY[..62],
            &format!("{KEY}00"),
            &"zz".repeat(32),
            "é".repeat(32).as_str(),
        ] {
            let error = RingSecretCipher::from_hex(bad).err().expect("refused");
            assert!(
                !error.0.contains(bad) || bad.is_empty(),
                "the message holds the key"
            );
        }
        assert!(RingSecretCipher::from_hex(&format!(" {KEY}\n")).is_ok());
        assert!(RingSecretCipher::from_hex(&KEY.to_uppercase()).is_ok());
    }

    #[test]
    fn without_a_key_nothing_is_sealed_or_opened() {
        let none = NoSecretCipher;
        assert!(!none.is_available());
        assert!(
            none.seal(CompanyId::generate(), "x", &secret("tok_live_1234567890"))
                .is_err()
        );
        assert!(none.open(CompanyId::generate(), "x", &[1; 40]).is_err());
    }
}
