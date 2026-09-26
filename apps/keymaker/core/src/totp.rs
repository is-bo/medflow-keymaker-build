//! TOTP (RFC 6238) over HOTP (RFC 4226): HMAC-SHA1, 30-second steps, 6 digits
//! — the defaults every authenticator app (Google, Microsoft, Aegis, …) uses
//! when it scans an `otpauth://` QR code.

use hmac::{Hmac, Mac};
use sha1::Sha1;
use subtle::ConstantTimeEq;

pub const STEP_SECONDS: i64 = 30;
pub const DIGITS: u32 = 6;
/// Accept the previous and next step too (phone clocks drift a little).
pub const WINDOW: i64 = 1;
pub const ISSUER: &str = "MedFlow Key Maker";
const SECRET_BYTES: usize = 20;
const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// RFC 4648 base32, no padding (the form otpauth URIs use).
pub fn base32_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for &b in bytes {
        buffer = (buffer << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(BASE32[((buffer >> bits) & 0x1F) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(BASE32[((buffer << (5 - bits)) & 0x1F) as usize] as char);
    }
    out
}

/// Lenient decode: case, spaces and `=` padding are ignored.
pub fn base32_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for c in text.chars().filter(|c| !c.is_whitespace() && *c != '=' && *c != '-') {
        let v = BASE32.iter().position(|&s| s as char == c.to_ascii_uppercase())? as u32;
        buffer = (buffer << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

/// A new random 160-bit secret, base32 encoded.
pub fn generate_secret() -> String {
    base32_encode(&crate::envelope::random_bytes::<SECRET_BYTES>())
}

/// RFC 4226 HOTP with a configurable digit count.
pub fn hotp(secret: &[u8], counter: u64, digits: u32) -> u32 {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[19] & 0x0F) as usize;
    let code = (u32::from(digest[offset] & 0x7F) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);
    code % 10u32.pow(digits)
}

pub fn step_at(unix_seconds: i64) -> u64 {
    (unix_seconds.max(0) / STEP_SECONDS) as u64
}

/// The code for `unix_seconds`, zero-padded.
pub fn totp_at(secret: &[u8], unix_seconds: i64, digits: u32) -> String {
    format!(
        "{:0width$}",
        hotp(secret, step_at(unix_seconds), digits),
        width = digits as usize
    )
}

/// Check a typed 6-digit code at `now_ms`, ±`WINDOW` steps. Returns the
/// matched step so the caller can refuse to accept the same code twice
/// (`last_used_step`): a code seen over the shoulder cannot be replayed.
pub fn verify(secret_b32: &str, code: &str, now_ms: i64, last_used_step: Option<u64>) -> Option<u64> {
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let secret = base32_decode(secret_b32)?;
    let now_step = step_at(now_ms.div_euclid(1000)) as i64;
    let mut matched = None;
    // Check every candidate (no early exit) so timing does not reveal which step matched.
    for delta in -WINDOW..=WINDOW {
        let step = now_step + delta;
        if step < 0 {
            continue;
        }
        let expected = format!("{:06}", hotp(&secret, step as u64, DIGITS));
        if bool::from(expected.as_bytes().ct_eq(code.as_bytes())) {
            matched = Some(step as u64);
        }
    }
    match (matched, last_used_step) {
        (Some(step), Some(last)) if step <= last => None,
        (m, _) => m,
    }
}

fn uri_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// `otpauth://totp/MedFlow%20Key%20Maker:<account>?secret=…&issuer=…`
pub fn otpauth_uri(account: &str, secret_b32: &str) -> String {
    format!(
        "otpauth://totp/{}:{}?secret={secret_b32}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECONDS}",
        uri_encode(ISSUER),
        uri_encode(account),
        uri_encode(ISSUER)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238 Appendix B, SHA1 rows (8 digits, ASCII secret "12345678901234567890").
    const RFC_SECRET: &[u8] = b"12345678901234567890";
    const RFC_VECTORS: &[(i64, &str)] = &[
        (59, "94287082"),
        (1_111_111_109, "07081804"),
        (1_111_111_111, "14050471"),
        (1_234_567_890, "89005924"),
        (2_000_000_000, "69279037"),
        (20_000_000_000, "65353130"),
    ];

    #[test]
    fn rfc6238_sha1_vectors() {
        for &(t, expected) in RFC_VECTORS {
            assert_eq!(totp_at(RFC_SECRET, t, 8), expected, "t={t}");
            // 6-digit codes are the last six digits of the same value.
            assert_eq!(totp_at(RFC_SECRET, t, 6), &expected[2..], "t={t}");
        }
    }

    #[test]
    fn rfc4226_hotp_vectors() {
        let expected = [755224, 287082, 359152, 969429, 338314, 254676, 287922, 162583, 399871, 520489];
        for (counter, &code) in expected.iter().enumerate() {
            assert_eq!(hotp(RFC_SECRET, counter as u64, 6), code);
        }
    }

    #[test]
    fn base32_round_trip_and_known_value() {
        assert_eq!(base32_encode(RFC_SECRET), "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        assert_eq!(base32_decode("gezd gnbv gy3t qojq gezd gnbv gy3t qojq").unwrap(), RFC_SECRET);
        let secret = generate_secret();
        assert_eq!(secret.len(), 32);
        assert_eq!(base32_decode(&secret).unwrap().len(), 20);
        assert!(base32_decode("not base32!").is_none());
    }

    #[test]
    fn verify_accepts_one_step_of_drift_and_refuses_replay() {
        let b32 = base32_encode(RFC_SECRET);
        let now_ms = 1_111_111_111_000;
        let now = totp_at(RFC_SECRET, 1_111_111_111, 6);
        let prev = totp_at(RFC_SECRET, 1_111_111_111 - 30, 6);
        let next = totp_at(RFC_SECRET, 1_111_111_111 + 30, 6);
        let far = totp_at(RFC_SECRET, 1_111_111_111 - 90, 6);
        let step = verify(&b32, &now, now_ms, None).expect("current code");
        assert_eq!(step, step_at(1_111_111_111));
        assert!(verify(&b32, &prev, now_ms, None).is_some());
        assert!(verify(&b32, &next, now_ms, None).is_some());
        assert!(verify(&b32, &far, now_ms, None).is_none());
        // Replay: the same (or an older) step is refused once used.
        assert!(verify(&b32, &now, now_ms, Some(step)).is_none());
        assert!(verify(&b32, &prev, now_ms, Some(step)).is_none());
        assert!(verify(&b32, &next, now_ms, Some(step)).is_some());
        // Shape checks.
        assert!(verify(&b32, "12345", now_ms, None).is_none());
        assert!(verify(&b32, "abcdef", now_ms, None).is_none());
        assert!(verify(&b32, &format!(" {} ", &now[..3]).replace(' ', "") , now_ms, None).is_none());
    }

    #[test]
    fn otpauth_uri_shape() {
        let uri = otpauth_uri("prod-2026-09", "ABCDEF");
        assert_eq!(
            uri,
            "otpauth://totp/MedFlow%20Key%20Maker:prod-2026-09?secret=ABCDEF&issuer=MedFlow%20Key%20Maker&algorithm=SHA1&digits=6&period=30"
        );
    }
}
