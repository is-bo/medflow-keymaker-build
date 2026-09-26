//! Activation keys: the signed payload, its text form, signing and verifying.
//!
//! ```text
//! MFK2.<base64url(payload JSON)>.<base64url(Ed25519 signature)>
//! ```
//!
//! The signature covers the ASCII bytes `MFK2.<payload part>` exactly as they
//! appear in the key (the JWS signing-input shape), so the prefix is
//! domain-separated and no re-serialisation is ever involved in verification.
//! A `.mflic` file holds the same string. Only `[A-Za-z0-9_.-]` appears, so it
//! survives WhatsApp, e-mail and SMS; whitespace or zero-width characters a
//! messenger inserts are stripped before parsing.

use crate::calendar::Term;
use crate::machine::MachineCode;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const KEY_PREFIX: &str = "MFK2";
/// Payload schema version (`v`). Bump when fields change; old verifiers then
/// report `Malformed(UnsupportedVersion)` instead of misreading a new key.
pub const PAYLOAD_VERSION: u32 = 1;
/// Keys whose expiry is at most this far away report `ExpiringSoon`.
pub const EXPIRY_WARNING_MS: i64 = 14 * crate::calendar::MS_PER_DAY;
/// Features a key may grant. Issuance refuses anything else so a typo in the
/// Key Maker can never mint a key that silently grants nothing.
pub const KNOWN_FEATURES: &[&str] = &["telegram"];
/// Longest key accepted by the parser — generous, but bounds the work done on
/// hostile input (a real key is ~400 characters).
const MAX_KEY_CHARS: usize = 4096;

/// The signed payload. Field names are the wire format (camelCase JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct License {
    pub v: u32,
    pub kid: String,
    pub license_id: String,
    pub licensee: String,
    /// Canonical `MF-XXXX-XXXX-XXXX-XXXX`.
    pub machine_code: String,
    pub major: u32,
    pub features: Vec<String>,
    /// Unix ms.
    pub issued_at: i64,
    /// Unix ms; `None` (JSON `null`) = lifetime.
    pub expires_at: Option<i64>,
}

impl License {
    pub fn term(&self) -> Term {
        Term::classify(self.issued_at, self.expires_at)
    }

    pub fn is_lifetime(&self) -> bool {
        self.expires_at.is_none()
    }

    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }
}

/// One trusted verifying key. Public keys are safe to embed and commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustedKey<'a> {
    pub kid: &'a str,
    pub public_key: [u8; 32],
}

/// What a verifier trusts: several keys by `kid` (rotation), plus removal
/// lists. A kid on `revoked_kids` is refused even if it is still in `keys`,
/// and a license id on `revoked_license_ids` is refused whatever signed it.
#[derive(Debug, Clone, Copy)]
pub struct TrustStore<'a> {
    pub keys: &'a [TrustedKey<'a>],
    pub revoked_kids: &'a [&'a str],
    pub revoked_license_ids: &'a [&'a str],
}

impl<'a> TrustStore<'a> {
    fn key_for(&self, kid: &str) -> Option<VerifyingKey> {
        if self.revoked_kids.contains(&kid) {
            return None;
        }
        self.keys
            .iter()
            .find(|k| k.kid == kid)
            .and_then(|k| VerifyingKey::from_bytes(&k.public_key).ok())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Malformed {
    /// Empty after stripping whitespace.
    Empty,
    TooLong,
    /// Not `MFK2.<payload>.<signature>`.
    Structure,
    /// A part is not base64url.
    Encoding,
    /// The payload is not the expected JSON object.
    Payload,
    /// `v` is newer (or older) than this verifier understands.
    UnsupportedVersion,
    /// Signed, but internally inconsistent (e.g. expires before it is issued,
    /// or the machine code inside is not a valid code). Only a buggy issuer
    /// produces this.
    Inconsistent,
}

impl Malformed {
    pub fn code(self) -> &'static str {
        match self {
            Malformed::Empty => "empty",
            Malformed::TooLong => "too_long",
            Malformed::Structure => "structure",
            Malformed::Encoding => "encoding",
            Malformed::Payload => "payload",
            Malformed::UnsupportedVersion => "unsupported_version",
            Malformed::Inconsistent => "inconsistent",
        }
    }
}

/// The verdict on a key for one machine at one instant.
///
/// Only `Valid` and `ExpiringSoon` permit new work. Every variant that carries
/// a `License` has passed signature verification, so its fields can be shown
/// to the user (e.g. "this key was issued for MF-…").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyStatus {
    Valid(License),
    /// Valid, and expires within `EXPIRY_WARNING_MS`.
    ExpiringSoon(License),
    Expired(License),
    WrongMachine(License),
    WrongMajor(License),
    /// Signed by a trusted key, but its license id is on the removal list.
    Revoked(License),
    /// The kid is not trusted (never was, or was removed / revoked).
    UnknownKid(String),
    BadSignature,
    Malformed(Malformed),
}

impl KeyStatus {
    /// Stable snake_case code for UIs and logs.
    pub fn code(&self) -> &'static str {
        match self {
            KeyStatus::Valid(_) => "valid",
            KeyStatus::ExpiringSoon(_) => "expiring_soon",
            KeyStatus::Expired(_) => "expired",
            KeyStatus::WrongMachine(_) => "wrong_machine",
            KeyStatus::WrongMajor(_) => "wrong_major",
            KeyStatus::Revoked(_) => "revoked",
            KeyStatus::UnknownKid(_) => "unknown_kid",
            KeyStatus::BadSignature => "bad_signature",
            KeyStatus::Malformed(_) => "malformed",
        }
    }

    /// The license, for every verdict reached after the signature verified.
    pub fn license(&self) -> Option<&License> {
        match self {
            KeyStatus::Valid(l)
            | KeyStatus::ExpiringSoon(l)
            | KeyStatus::Expired(l)
            | KeyStatus::WrongMachine(l)
            | KeyStatus::WrongMajor(l)
            | KeyStatus::Revoked(l) => Some(l),
            _ => None,
        }
    }

    /// May the holder create and edit records?
    pub fn permits_work(&self) -> bool {
        matches!(self, KeyStatus::Valid(_) | KeyStatus::ExpiringSoon(_))
    }

    /// Is this a genuine key for this machine and major version (expired or
    /// not)? Only such keys are candidates for storage and renewal ranking.
    pub fn is_for_this_machine(&self) -> bool {
        matches!(
            self,
            KeyStatus::Valid(_) | KeyStatus::ExpiringSoon(_) | KeyStatus::Expired(_)
        )
    }
}

/// Strip what messengers and editors add around a pasted key: whitespace and
/// line breaks anywhere, zero-width characters, and wrapping quotes/backticks.
pub fn normalize_key_text(input: &str) -> String {
    input
        .chars()
        .filter(|&c| {
            !c.is_whitespace()
                && !matches!(
                    c,
                    '\u{200B}'..='\u{200D}' | '\u{2060}' | '\u{FEFF}' | '"' | '\'' | '`' | '\u{201C}' | '\u{201D}'
                )
        })
        .collect()
}

struct ParsedKey<'k> {
    signing_input: &'k str,
    payload: License,
    signature: Signature,
}

fn parse(normalized: &str) -> Result<ParsedKey<'_>, Malformed> {
    if normalized.is_empty() {
        return Err(Malformed::Empty);
    }
    if normalized.len() > MAX_KEY_CHARS {
        return Err(Malformed::TooLong);
    }
    let mut parts = normalized.split('.');
    let (Some(prefix), Some(payload_b64), Some(signature_b64), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(Malformed::Structure);
    };
    if prefix != KEY_PREFIX || payload_b64.is_empty() || signature_b64.is_empty() {
        return Err(Malformed::Structure);
    }
    let body = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_| Malformed::Encoding)?;
    let signature_bytes = URL_SAFE_NO_PAD
        .decode(signature_b64)
        .map_err(|_| Malformed::Encoding)?;
    let signature = Signature::from_slice(&signature_bytes).map_err(|_| Malformed::Encoding)?;
    // Probe the version before the strict parse so a newer key format reports
    // itself as such rather than as garbage.
    let probe: serde_json::Value = serde_json::from_slice(&body).map_err(|_| Malformed::Payload)?;
    match probe.get("v").and_then(|v| v.as_u64()) {
        Some(v) if v == u64::from(PAYLOAD_VERSION) => {}
        Some(_) => return Err(Malformed::UnsupportedVersion),
        None => return Err(Malformed::Payload),
    }
    let payload: License = serde_json::from_value(probe).map_err(|_| Malformed::Payload)?;
    let signing_input = &normalized[..prefix.len() + 1 + payload_b64.len()];
    Ok(ParsedKey {
        signing_input,
        payload,
        signature,
    })
}

/// Verify `key` for `machine`, a build of major version `app_major`, at
/// `now_ms`. Checks run in this order, and nothing in the payload is trusted
/// until the signature has verified:
///
/// 1. shape / encoding / payload version → `Malformed`
/// 2. `kid` trusted and not removed → `UnknownKid`
/// 3. Ed25519 signature over `MFK2.<payload>` → `BadSignature`
/// 4. license id not on the removal list → `Revoked`
/// 5. `major` equals `app_major` → `WrongMajor`
/// 6. `machineCode` equals `machine` → `WrongMachine`
/// 7. expiry → `Expired` / `ExpiringSoon` (≤ 14 days) / `Valid`
///
/// Clock trust is NOT this function's job: it believes `now_ms`. The desktop
/// wraps it with a rollback guard.
pub fn verify(
    trust: &TrustStore<'_>,
    key: &str,
    machine: &MachineCode,
    app_major: u32,
    now_ms: i64,
) -> KeyStatus {
    let normalized = normalize_key_text(key);
    let parsed = match parse(&normalized) {
        Ok(parsed) => parsed,
        Err(malformed) => return KeyStatus::Malformed(malformed),
    };
    let Some(verifying_key) = trust.key_for(&parsed.payload.kid) else {
        return KeyStatus::UnknownKid(parsed.payload.kid);
    };
    if verifying_key
        .verify_strict(parsed.signing_input.as_bytes(), &parsed.signature)
        .is_err()
    {
        return KeyStatus::BadSignature;
    }
    let license = parsed.payload;
    // Signed by us, so these only fail on an issuer bug — but never trust a
    // payload that contradicts itself.
    let Ok(licensed_machine) = MachineCode::parse(&license.machine_code) else {
        return KeyStatus::Malformed(Malformed::Inconsistent);
    };
    if matches!(license.expires_at, Some(expires) if expires <= license.issued_at) {
        return KeyStatus::Malformed(Malformed::Inconsistent);
    }
    if trust
        .revoked_license_ids
        .iter()
        .any(|id| id.eq_ignore_ascii_case(&license.license_id))
    {
        return KeyStatus::Revoked(license);
    }
    if license.major != app_major {
        return KeyStatus::WrongMajor(license);
    }
    if &licensed_machine != machine {
        return KeyStatus::WrongMachine(license);
    }
    match license.expires_at {
        Some(expires) if now_ms >= expires => KeyStatus::Expired(license),
        Some(expires) if expires - now_ms <= EXPIRY_WARNING_MS => KeyStatus::ExpiringSoon(license),
        _ => KeyStatus::Valid(license),
    }
}

/// Read the payload WITHOUT verifying it — for display only (e.g. showing
/// which machine a rejected key names). Never base a decision on this.
pub fn read_unverified(key: &str) -> Option<License> {
    let normalized = normalize_key_text(key);
    parse(&normalized).ok().map(|p| p.payload)
}

// ── Issuing (Key Maker / dev CLI) ────────────────────────────────────────────

/// Everything the Key Maker form collects.
#[derive(Debug, Clone)]
pub struct KeyRequest {
    pub licensee: String,
    pub machine_code: MachineCode,
    pub major: u32,
    pub features: Vec<String>,
    pub issued_at: i64,
    pub term: Term,
    /// `None` → a fresh random UUID.
    pub license_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueError {
    EmptyKid,
    EmptyLicensee,
    UnknownFeature(String),
    ZeroLengthTerm,
    Randomness,
    Encoding,
}

impl fmt::Display for IssueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IssueError::EmptyKid => write!(f, "the key id (kid) is empty"),
            IssueError::EmptyLicensee => write!(f, "the licensee name is empty"),
            IssueError::UnknownFeature(name) => write!(
                f,
                "unknown feature '{name}' (known: {})",
                KNOWN_FEATURES.join(", ")
            ),
            IssueError::ZeroLengthTerm => write!(f, "the duration must be at least 1"),
            IssueError::Randomness => write!(f, "the system random generator failed"),
            IssueError::Encoding => write!(f, "could not encode the payload"),
        }
    }
}

impl std::error::Error for IssueError {}

/// Ed25519 public key for a 32-byte seed.
pub fn public_key_from_seed(seed: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(seed).verifying_key().to_bytes()
}

/// A fresh 32-byte signing seed from the OS CSPRNG.
pub fn generate_seed() -> Result<[u8; 32], IssueError> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|_| IssueError::Randomness)?;
    Ok(seed)
}

/// A random (version 4) UUID for `licenseId`.
pub fn new_license_id() -> Result<String, IssueError> {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).map_err(|_| IssueError::Randomness)?;
    b[6] = (b[6] & 0x0F) | 0x40;
    b[8] = (b[8] & 0x3F) | 0x80;
    let h = hex::encode(b);
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    ))
}

/// Sign an already-built payload. `license.kid` is overwritten with `kid` so a
/// key can never claim a signer other than the one that signed it.
pub fn sign_license(seed: &[u8; 32], kid: &str, license: &License) -> Result<String, IssueError> {
    if kid.trim().is_empty() {
        return Err(IssueError::EmptyKid);
    }
    let mut payload = license.clone();
    payload.kid = kid.to_string();
    payload.v = PAYLOAD_VERSION;
    let body = serde_json::to_vec(&payload).map_err(|_| IssueError::Encoding)?;
    let signing_input = format!("{KEY_PREFIX}.{}", URL_SAFE_NO_PAD.encode(body));
    let signature = SigningKey::from_bytes(seed).sign(signing_input.as_bytes());
    Ok(format!(
        "{signing_input}.{}",
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    ))
}

/// Build and sign a key from a form. Returns the key string and its payload.
pub fn issue_key(
    seed: &[u8; 32],
    kid: &str,
    request: &KeyRequest,
) -> Result<(String, License), IssueError> {
    let licensee = request.licensee.trim();
    if licensee.is_empty() {
        return Err(IssueError::EmptyLicensee);
    }
    if request.term.count() == Some(0) {
        return Err(IssueError::ZeroLengthTerm);
    }
    let mut features: Vec<String> = Vec::new();
    for feature in &request.features {
        let feature = feature.trim().to_ascii_lowercase();
        if !KNOWN_FEATURES.contains(&feature.as_str()) {
            return Err(IssueError::UnknownFeature(feature));
        }
        if !features.contains(&feature) {
            features.push(feature);
        }
    }
    features.sort();
    let license = License {
        v: PAYLOAD_VERSION,
        kid: kid.to_string(),
        license_id: match &request.license_id {
            Some(id) => id.clone(),
            None => new_license_id()?,
        },
        licensee: licensee.to_string(),
        machine_code: request.machine_code.as_str().to_string(),
        major: request.major,
        features,
        issued_at: request.issued_at,
        expires_at: request.term.expires_at(request.issued_at),
    };
    let key = sign_license(seed, kid, &license)?;
    Ok((key, license))
}

/// The Rust source line to paste into the desktop's trusted-key table
/// (`TRUSTED_KEYS` in apps/desktop/src-tauri/src/license.rs).
pub fn trusted_key_rust_row(kid: &str, public_key: &[u8; 32]) -> String {
    let bytes: Vec<String> = public_key.iter().map(|b| b.to_string()).collect();
    format!(
        "TrustedKey {{ kid: \"{kid}\", public_key: [{}] }},",
        bytes.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::MS_PER_DAY;

    const SEED: [u8; 32] = [42u8; 32];
    const OTHER_SEED: [u8; 32] = [43u8; 32];
    const NOW: i64 = 1_790_000_000_000;

    fn machine() -> MachineCode {
        MachineCode::derive("test-machine")
    }

    fn trusted() -> Vec<TrustedKey<'static>> {
        vec![TrustedKey {
            kid: "test-1",
            public_key: public_key_from_seed(&SEED),
        }]
    }

    fn request(term: Term) -> KeyRequest {
        KeyRequest {
            licensee: "Dr Amina Benali".into(),
            machine_code: machine(),
            major: 2,
            features: vec!["telegram".into()],
            issued_at: NOW,
            term,
            license_id: Some("0b6f2c1e-9d7a-4e55-8c3f-1a2b3c4d5e6f".into()),
        }
    }

    fn issue(term: Term) -> String {
        issue_key(&SEED, "test-1", &request(term)).unwrap().0
    }

    fn check(key: &str, now: i64) -> KeyStatus {
        let keys = trusted();
        let trust = TrustStore {
            keys: &keys,
            revoked_kids: &[],
            revoked_license_ids: &[],
        };
        verify(&trust, key, &machine(), 2, now)
    }

    /// Re-encode a key with `edit` applied to its payload JSON but the
    /// ORIGINAL signature kept — what a tamperer can produce without the seed.
    fn tamper(key: &str, edit: impl FnOnce(&mut serde_json::Value)) -> String {
        let mut parts = key.split('.');
        let (prefix, payload, signature) = (
            parts.next().unwrap(),
            parts.next().unwrap(),
            parts.next().unwrap(),
        );
        let mut json: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap();
        edit(&mut json);
        format!(
            "{prefix}.{}.{signature}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&json).unwrap())
        )
    }

    #[test]
    fn issued_keys_verify_and_carry_their_fields() {
        let key = issue(Term::Months(1));
        assert!(key.starts_with("MFK2."));
        assert!(key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')));
        match check(&key, NOW) {
            KeyStatus::Valid(license) => {
                assert_eq!(license.licensee, "Dr Amina Benali");
                assert_eq!(license.machine_code, machine().as_str());
                assert_eq!(license.features, vec!["telegram".to_string()]);
                assert_eq!(license.term(), Term::Months(1));
                assert_eq!(license.major, 2);
            }
            other => panic!("expected valid, got {other:?}"),
        }
        let lifetime = issue(Term::Lifetime);
        match check(&lifetime, NOW + 80 * 365 * MS_PER_DAY) {
            KeyStatus::Valid(license) => assert!(license.is_lifetime()),
            other => panic!("expected valid lifetime, got {other:?}"),
        }
    }

    #[test]
    fn expiry_boundaries_are_exact() {
        let key = issue(Term::Days(30));
        let expires = NOW + 30 * MS_PER_DAY;
        assert_eq!(check(&key, expires - EXPIRY_WARNING_MS - 1).code(), "valid");
        assert_eq!(check(&key, expires - EXPIRY_WARNING_MS).code(), "expiring_soon");
        assert_eq!(check(&key, expires - 1).code(), "expiring_soon");
        assert_eq!(check(&key, expires).code(), "expired");
    }

    #[test]
    fn pasted_keys_survive_messenger_noise() {
        let key = issue(Term::Years(1));
        let noisy = format!(
            "  \"{}\n{}\u{200B}\"\r\n",
            &key[..60],
            &key[60..]
        );
        assert_eq!(check(&noisy, NOW).code(), "valid");
    }

    #[test]
    fn flipping_any_payload_byte_breaks_the_signature() {
        let key = issue(Term::Years(1));
        let dot = key.find('.').unwrap() + 1;
        let end = key.rfind('.').unwrap();
        for index in [dot, dot + 10, (dot + end) / 2, end - 1] {
            let mut bytes = key.clone().into_bytes();
            bytes[index] = if bytes[index] == b'A' { b'B' } else { b'A' };
            let tampered = String::from_utf8(bytes).unwrap();
            let status = check(&tampered, NOW);
            assert!(
                matches!(
                    status,
                    KeyStatus::BadSignature | KeyStatus::Malformed(_) | KeyStatus::UnknownKid(_)
                ),
                "index {index}: {status:?}"
            );
            assert!(!status.permits_work());
        }
    }

    #[test]
    fn edited_fields_fail_signature_verification() {
        let key = issue(Term::Months(1));
        let other_machine = MachineCode::derive("someone-else");
        type Edit = Box<dyn Fn(&mut serde_json::Value)>;
        let edits: Vec<Edit> = vec![
            Box::new(|j| j["expiresAt"] = serde_json::Value::Null),
            Box::new(|j| j["expiresAt"] = serde_json::json!(NOW + 3650 * MS_PER_DAY)),
            Box::new(move |j| j["machineCode"] = serde_json::json!(other_machine.as_str())),
            Box::new(|j| j["features"] = serde_json::json!(["telegram", "extra"])),
            Box::new(|j| j["licensee"] = serde_json::json!("Someone Else")),
            Box::new(|j| j["major"] = serde_json::json!(3)),
        ];
        for edit in edits {
            let tampered = tamper(&key, |json| edit(json));
            assert_eq!(check(&tampered, NOW), KeyStatus::BadSignature);
        }
    }

    #[test]
    fn signature_does_not_cover_a_swapped_prefix() {
        let key = issue(Term::Months(1));
        let swapped = key.replacen("MFK2", "MFK3", 1);
        assert_eq!(check(&swapped, NOW), KeyStatus::Malformed(Malformed::Structure));
    }

    #[test]
    fn key_for_another_machine_is_wrong_machine() {
        let mut req = request(Term::Months(1));
        req.machine_code = MachineCode::derive("someone-else");
        let (key, _) = issue_key(&SEED, "test-1", &req).unwrap();
        let status = check(&key, NOW);
        assert_eq!(status.code(), "wrong_machine");
        assert!(!status.permits_work());
        assert!(!status.is_for_this_machine());
        assert_eq!(
            status.license().unwrap().machine_code,
            req.machine_code.as_str()
        );
    }

    #[test]
    fn unknown_and_revoked_kids_are_refused() {
        let (key, _) = issue_key(&OTHER_SEED, "rogue", &request(Term::Lifetime)).unwrap();
        assert_eq!(check(&key, NOW), KeyStatus::UnknownKid("rogue".into()));

        // Signed by a key we do not hold but claiming our kid → bad signature.
        let (forged, _) = issue_key(&OTHER_SEED, "test-1", &request(Term::Lifetime)).unwrap();
        assert_eq!(check(&forged, NOW), KeyStatus::BadSignature);

        let keys = trusted();
        let revoked = TrustStore {
            keys: &keys,
            revoked_kids: &["test-1"],
            revoked_license_ids: &[],
        };
        let good = issue(Term::Lifetime);
        assert_eq!(
            verify(&revoked, &good, &machine(), 2, NOW),
            KeyStatus::UnknownKid("test-1".into())
        );
    }

    #[test]
    fn revoked_license_ids_are_refused() {
        let keys = trusted();
        let trust = TrustStore {
            keys: &keys,
            revoked_kids: &[],
            revoked_license_ids: &["0B6F2C1E-9D7A-4E55-8C3F-1A2B3C4D5E6F"],
        };
        let status = verify(&trust, &issue(Term::Lifetime), &machine(), 2, NOW);
        assert_eq!(status.code(), "revoked");
    }

    #[test]
    fn wrong_major_is_reported_after_the_signature() {
        let key = issue(Term::Lifetime);
        let keys = trusted();
        let trust = TrustStore {
            keys: &keys,
            revoked_kids: &[],
            revoked_license_ids: &[],
        };
        let status = verify(&trust, &key, &machine(), 3, NOW);
        assert_eq!(status.code(), "wrong_major");
        assert_eq!(status.license().unwrap().major, 2);
    }

    #[test]
    fn malformed_inputs_are_classified() {
        assert_eq!(check("", NOW), KeyStatus::Malformed(Malformed::Empty));
        assert_eq!(check("hello", NOW), KeyStatus::Malformed(Malformed::Structure));
        assert_eq!(check("MFK2.a.b.c", NOW), KeyStatus::Malformed(Malformed::Structure));
        assert_eq!(check("MFL1.abc.def", NOW), KeyStatus::Malformed(Malformed::Structure));
        assert_eq!(check("MFK2.@@@.def", NOW), KeyStatus::Malformed(Malformed::Encoding));
        let not_json = format!(
            "MFK2.{}.{}",
            URL_SAFE_NO_PAD.encode(b"not json"),
            URL_SAFE_NO_PAD.encode([0u8; 64])
        );
        assert_eq!(check(&not_json, NOW), KeyStatus::Malformed(Malformed::Payload));
        let future = tamper(&issue(Term::Lifetime), |j| j["v"] = serde_json::json!(2));
        assert_eq!(
            check(&future, NOW),
            KeyStatus::Malformed(Malformed::UnsupportedVersion)
        );
        let smuggled = tamper(&issue(Term::Lifetime), |j| j["admin"] = serde_json::json!(true));
        assert_eq!(check(&smuggled, NOW), KeyStatus::Malformed(Malformed::Payload));
        assert_eq!(
            check(&"A".repeat(MAX_KEY_CHARS + 1), NOW),
            KeyStatus::Malformed(Malformed::TooLong)
        );
    }

    #[test]
    fn signed_but_inconsistent_payloads_are_refused() {
        let mut license = issue_key(&SEED, "test-1", &request(Term::Months(1))).unwrap().1;
        license.expires_at = Some(license.issued_at);
        let key = sign_license(&SEED, "test-1", &license).unwrap();
        assert_eq!(check(&key, NOW), KeyStatus::Malformed(Malformed::Inconsistent));
    }

    #[test]
    fn issuance_validates_the_form() {
        let mut req = request(Term::Months(1));
        req.features = vec!["telgram".into()];
        assert_eq!(
            issue_key(&SEED, "test-1", &req).unwrap_err(),
            IssueError::UnknownFeature("telgram".into())
        );
        req.features = vec![" Telegram ".into(), "telegram".into()];
        assert_eq!(
            issue_key(&SEED, "test-1", &req).unwrap().1.features,
            vec!["telegram".to_string()]
        );
        req.licensee = "  ".into();
        assert_eq!(
            issue_key(&SEED, "test-1", &req).unwrap_err(),
            IssueError::EmptyLicensee
        );
        let mut zero = request(Term::Months(0));
        zero.license_id = None;
        assert_eq!(
            issue_key(&SEED, "test-1", &zero).unwrap_err(),
            IssueError::ZeroLengthTerm
        );
        assert_eq!(
            issue_key(&SEED, " ", &request(Term::Lifetime)).unwrap_err(),
            IssueError::EmptyKid
        );
    }

    #[test]
    fn license_ids_are_random_v4_uuids() {
        let a = new_license_id().unwrap();
        let b = new_license_id().unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
        assert!(matches!(&a[19..20], "8" | "9" | "a" | "b"));
    }

    #[test]
    fn rust_row_embeds_the_public_key() {
        let row = trusted_key_rust_row("prod-2026", &public_key_from_seed(&SEED));
        assert!(row.starts_with("TrustedKey { kid: \"prod-2026\", public_key: ["));
        // kid, 31 separators between the 32 bytes, and the trailing comma.
        assert_eq!(row.matches(',').count(), 33);
    }
}
