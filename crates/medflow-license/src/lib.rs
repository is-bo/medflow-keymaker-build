//! MedFlow offline license keys — one implementation shared by the desktop
//! verifier (apps/desktop/src-tauri) and the MedFlow Key Maker (Android).
//!
//! No Tauri, no I/O, no clock: every function takes `now` and returns a value.
//!
//! Key Maker flow:
//! 1. First setup: [`generate_seed`] → show [`phrase::seed_to_phrase`] once →
//!    show [`trusted_key_rust_row`] (kid + [`public_key_from_seed`]) for
//!    embedding in the desktop's trusted-key table.
//! 2. Restore: [`phrase::phrase_to_seed`].
//! 3. Issue: [`MachineCode::parse`] the doctor's code (typos are caught by its
//!    checksum), then [`issue_key`] with a [`KeyRequest`] → key string, shared
//!    as text or saved as a `.mflic` file (same string).
//!
//! Desktop flow: [`MachineCode::derive`] from the OS machine id, then
//! [`verify`] against its [`TrustStore`] → [`KeyStatus`].
//!
//! Format and threat model: docs/licensing.md.

pub mod calendar;
pub mod key;
pub mod machine;

#[cfg(feature = "host")]
pub mod host;
#[cfg(feature = "phrase")]
pub mod phrase;

pub use calendar::Term;
pub use key::{
    generate_seed, issue_key, new_license_id, normalize_key_text, public_key_from_seed,
    read_unverified, sign_license, trusted_key_rust_row, verify, IssueError, KeyRequest,
    KeyStatus, License, Malformed, TrustStore, TrustedKey, EXPIRY_WARNING_MS, KEY_PREFIX,
    KNOWN_FEATURES, PAYLOAD_VERSION,
};
pub use machine::{MachineCode, MachineCodeError, MACHINE_CODE_PREFIX};
