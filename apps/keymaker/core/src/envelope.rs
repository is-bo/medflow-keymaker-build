//! Password-sealed files: Argon2id → AES-256-GCM.
//!
//! Used for both the on-phone vault and the exported backup (different
//! `format` tags, so one can never be mistaken for the other). The file is a
//! small JSON document carrying everything needed to open it except the
//! password: KDF parameters, salt, nonce, ciphertext. The header (format,
//! version, KDF parameters, salt) is bound into the GCM tag as associated data,
//! so weakening the stored parameters makes decryption fail instead of
//! silently lowering the cost of a guess.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

pub const VAULT_FORMAT: &str = "medflow-keymaker-vault";
pub const BACKUP_FORMAT: &str = "medflow-keymaker-backup";
const ENVELOPE_VERSION: u32 = 1;
const KDF_ALG: &str = "argon2id";
const CIPHER: &str = "aes-256-gcm";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

/// Argon2id cost. Opening refuses parameters outside these bounds so a hostile
/// "backup" cannot make the phone allocate gigabytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
}

/// 64 MiB, 3 passes: about 0.5–1.5 s on a mid-range phone, per guess.
pub const PHONE_KDF: KdfParams = KdfParams {
    m_kib: 64 * 1024,
    t: 3,
    p: 1,
};

/// Cheap parameters for unit tests only.
pub const TEST_KDF: KdfParams = KdfParams {
    m_kib: 64,
    t: 1,
    p: 1,
};

const MIN_M_KIB: u32 = 8;
const MAX_M_KIB: u32 = 256 * 1024;
const MAX_T: u32 = 10;
const MAX_P: u32 = 4;

impl KdfParams {
    fn in_bounds(self) -> bool {
        (MIN_M_KIB..=MAX_M_KIB).contains(&self.m_kib)
            && (1..=MAX_T).contains(&self.t)
            && (1..=MAX_P).contains(&self.p)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KdfHeader {
    alg: String,
    m_kib: u32,
    t: u32,
    p: u32,
    salt: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    format: String,
    version: u32,
    kdf: KdfHeader,
    cipher: String,
    nonce: String,
    ciphertext: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenError {
    /// Not JSON / not an envelope / bad base64.
    NotEnvelope,
    /// An envelope of another kind (a vault offered as a backup, …).
    WrongFormat,
    /// Unknown algorithm or out-of-bounds cost parameters.
    BadParams,
    /// Wrong password, or the file was modified. GCM cannot tell them apart.
    WrongPasswordOrDamaged,
}

/// A derived 256-bit key plus the salt and parameters it came from, kept for
/// the unlocked session so the vault can be re-sealed after every change
/// without re-running Argon2.
pub struct SealKey {
    key: Zeroizing<[u8; 32]>,
    salt: [u8; SALT_LEN],
    params: KdfParams,
}

fn derive(password: &str, salt: &[u8], params: KdfParams) -> Zeroizing<[u8; 32]> {
    let argon = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(params.m_kib, params.t, params.p, Some(32)).expect("bounded params are valid"),
    );
    let mut out = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(password.as_bytes(), salt, out.as_mut())
        .expect("argon2 with valid params and salt");
    out
}

fn aad(format: &str, params: KdfParams, salt_b64: &str) -> Vec<u8> {
    format!(
        "{format}|{ENVELOPE_VERSION}|{KDF_ALG}|{}|{}|{}|{salt_b64}|{CIPHER}",
        params.m_kib, params.t, params.p
    )
    .into_bytes()
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("the OS random generator failed");
    b
}

impl SealKey {
    /// A fresh random salt — use for every new file and every password change.
    pub fn new_random(password: &str, params: KdfParams) -> Self {
        let salt = random_bytes::<SALT_LEN>();
        SealKey {
            key: derive(password, &salt, params),
            salt,
            params,
        }
    }

    /// Does `password` derive this same key? Constant-time comparison.
    pub fn matches_password(&self, password: &str) -> bool {
        let candidate = derive(password, &self.salt, self.params);
        bool::from(candidate.as_ref().ct_eq(self.key.as_ref()))
    }

    /// Encrypt `plaintext` into a JSON envelope tagged `format`.
    pub fn seal(&self, format: &str, plaintext: &[u8]) -> String {
        let salt_b64 = STANDARD.encode(self.salt);
        let nonce = random_bytes::<NONCE_LEN>();
        let cipher = Aes256Gcm::new_from_slice(self.key.as_ref()).expect("32-byte key");
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: &aad(format, self.params, &salt_b64),
                },
            )
            .expect("AES-GCM encryption cannot fail for in-memory input");
        let envelope = Envelope {
            format: format.to_string(),
            version: ENVELOPE_VERSION,
            kdf: KdfHeader {
                alg: KDF_ALG.to_string(),
                m_kib: self.params.m_kib,
                t: self.params.t,
                p: self.params.p,
                salt: salt_b64,
            },
            cipher: CIPHER.to_string(),
            nonce: STANDARD.encode(nonce),
            ciphertext: STANDARD.encode(ciphertext),
        };
        serde_json::to_string_pretty(&envelope).expect("envelope serializes")
    }
}

/// Decrypt an envelope. On success also returns the `SealKey`, so the caller
/// can re-seal with the same salt (vault) without another Argon2 run.
pub fn open(
    format: &str,
    text: &str,
    password: &str,
) -> Result<(Zeroizing<Vec<u8>>, SealKey), OpenError> {
    let envelope: Envelope =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|_| OpenError::NotEnvelope)?;
    if envelope.format != format {
        return Err(OpenError::WrongFormat);
    }
    let params = KdfParams {
        m_kib: envelope.kdf.m_kib,
        t: envelope.kdf.t,
        p: envelope.kdf.p,
    };
    if envelope.version != ENVELOPE_VERSION
        || envelope.kdf.alg != KDF_ALG
        || envelope.cipher != CIPHER
        || !params.in_bounds()
    {
        return Err(OpenError::BadParams);
    }
    let salt_vec = STANDARD
        .decode(&envelope.kdf.salt)
        .map_err(|_| OpenError::NotEnvelope)?;
    let nonce = STANDARD
        .decode(&envelope.nonce)
        .map_err(|_| OpenError::NotEnvelope)?;
    let ciphertext = STANDARD
        .decode(&envelope.ciphertext)
        .map_err(|_| OpenError::NotEnvelope)?;
    let salt: [u8; SALT_LEN] = salt_vec.try_into().map_err(|_| OpenError::NotEnvelope)?;
    if nonce.len() != NONCE_LEN {
        return Err(OpenError::NotEnvelope);
    }
    let key = derive(password, &salt, params);
    let cipher = Aes256Gcm::new_from_slice(key.as_ref()).expect("32-byte key");
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad(format, params, &envelope.kdf.salt),
            },
        )
        .map_err(|_| OpenError::WrongPasswordOrDamaged)?;
    Ok((Zeroizing::new(plaintext), SealKey { key, salt, params }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_then_open_round_trips() {
        let key = SealKey::new_random("correct horse 42", TEST_KDF);
        let text = key.seal(VAULT_FORMAT, b"secret payload");
        let (plain, reopened) = open(VAULT_FORMAT, &text, "correct horse 42").unwrap();
        assert_eq!(plain.as_slice(), b"secret payload");
        assert!(reopened.matches_password("correct horse 42"));
        assert!(!reopened.matches_password("correct horse 43"));
        // Nothing readable leaks into the file.
        assert!(!text.contains("secret"));
    }

    #[test]
    fn wrong_password_fails() {
        let key = SealKey::new_random("correct horse 42", TEST_KDF);
        let text = key.seal(VAULT_FORMAT, b"x");
        assert_eq!(
            open(VAULT_FORMAT, &text, "wrong horse 42").err(),
            Some(OpenError::WrongPasswordOrDamaged)
        );
    }

    #[test]
    fn format_tags_are_not_interchangeable() {
        let key = SealKey::new_random("pw123456789", TEST_KDF);
        let vault = key.seal(VAULT_FORMAT, b"x");
        assert_eq!(open(BACKUP_FORMAT, &vault, "pw123456789").err(), Some(OpenError::WrongFormat));
        // Relabelling the format inside the JSON breaks the tag (AAD-bound).
        let relabelled = vault.replace(VAULT_FORMAT, BACKUP_FORMAT);
        assert_eq!(
            open(BACKUP_FORMAT, &relabelled, "pw123456789").err(),
            Some(OpenError::WrongPasswordOrDamaged)
        );
    }

    #[test]
    fn tampered_params_or_ciphertext_fail() {
        let key = SealKey::new_random("pw123456789", KdfParams { m_kib: 128, t: 2, p: 1 });
        let text = key.seal(VAULT_FORMAT, b"payload");
        // Lowering the stored cost is detected (header is authenticated).
        let weaker = text.replace("\"t\": 2", "\"t\": 1");
        assert_ne!(weaker, text);
        assert!(open(VAULT_FORMAT, &weaker, "pw123456789").is_err());
        // Absurd cost is refused before any work is done.
        let huge = text.replace("\"mKib\": 128", "\"mKib\": 99999999");
        assert_eq!(open(VAULT_FORMAT, &huge, "pw123456789").err(), Some(OpenError::BadParams));
        assert_eq!(open(VAULT_FORMAT, "not json", "pw").err(), Some(OpenError::NotEnvelope));
    }

    #[test]
    fn every_seal_uses_a_fresh_nonce() {
        let key = SealKey::new_random("pw123456789", TEST_KDF);
        assert_ne!(key.seal(VAULT_FORMAT, b"same"), key.seal(VAULT_FORMAT, b"same"));
    }
}
