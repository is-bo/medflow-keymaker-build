//! Ten single-use recovery codes: each replaces the authenticator code once
//! (the password is still required — it is what decrypts the vault).
//!
//! Codes are 10 Crockford base32 symbols (50 bits) shown as `XXXXX-XXXXX`.
//! Only salted SHA-256 hashes are kept, inside the encrypted vault.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub const CODE_COUNT: usize = 10;
const SYMBOLS: usize = 10;
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCode {
    pub hash_hex: String,
    pub used_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCodes {
    pub salt_hex: String,
    pub codes: Vec<RecoveryCode>,
}

/// Uppercase, drop separators, O→0 and I/L→1 (what people misread on paper).
pub fn normalize(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect()
}

fn hash(salt: &[u8], normalized: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"medflow-keymaker/recovery-code/v1\0");
    h.update(salt);
    h.update(normalized.as_bytes());
    h.finalize().into()
}

impl RecoveryCodes {
    /// New set; returns the stored hashes and the plain codes to show once.
    pub fn generate() -> (RecoveryCodes, Vec<String>) {
        let salt = crate::envelope::random_bytes::<16>();
        let mut plain = Vec::with_capacity(CODE_COUNT);
        let mut codes = Vec::with_capacity(CODE_COUNT);
        while plain.len() < CODE_COUNT {
            let raw = crate::envelope::random_bytes::<SYMBOLS>();
            let symbols: String = raw.iter().map(|b| ALPHABET[(b & 0x1F) as usize] as char).collect();
            if plain.iter().any(|p: &String| normalize(p) == symbols) {
                continue;
            }
            codes.push(RecoveryCode {
                hash_hex: hex::encode(hash(&salt, &symbols)),
                used_at: None,
            });
            plain.push(format!("{}-{}", &symbols[..5], &symbols[5..]));
        }
        (
            RecoveryCodes {
                salt_hex: hex::encode(salt),
                codes,
            },
            plain,
        )
    }

    pub fn remaining(&self) -> usize {
        self.codes.iter().filter(|c| c.used_at.is_none()).count()
    }

    /// If `input` is an unused code, mark it used and return true.
    pub fn consume(&mut self, input: &str, now_ms: i64) -> bool {
        let normalized = normalize(input);
        if normalized.len() != SYMBOLS {
            return false;
        }
        let Ok(salt) = hex::decode(&self.salt_hex) else {
            return false;
        };
        let candidate = hash(&salt, &normalized);
        let mut hit = None;
        for (i, code) in self.codes.iter().enumerate() {
            let Ok(stored) = hex::decode(&code.hash_hex) else {
                continue;
            };
            if code.used_at.is_none() && bool::from(stored.as_slice().ct_eq(&candidate)) {
                hit = Some(i);
            }
        }
        match hit {
            Some(i) => {
                self.codes[i].used_at = Some(now_ms);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_single_use_and_forgiving() {
        let (mut set, plain) = RecoveryCodes::generate();
        assert_eq!(plain.len(), CODE_COUNT);
        assert_eq!(set.remaining(), CODE_COUNT);
        let first = plain[0].clone();
        assert_eq!(first.len(), 11);
        // Lower case, spaces instead of the dash: still accepted.
        let messy = first.to_lowercase().replace('-', " ");
        assert!(set.consume(&messy, 1));
        assert_eq!(set.remaining(), CODE_COUNT - 1);
        // Second use of the same code fails.
        assert!(!set.consume(&first, 2));
        assert!(!set.consume("AAAAA-AAAAA", 3));
        assert!(!set.consume("", 3));
        assert!(set.consume(&plain[9], 4));
        assert_eq!(set.remaining(), CODE_COUNT - 2);
        // No plain code is stored.
        let stored = serde_json::to_string(&set).unwrap();
        for p in &plain {
            assert!(!stored.contains(&normalize(p)));
        }
    }
}
