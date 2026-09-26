//! MedFlow Key Maker security core.
//!
//! Everything that protects the signing key lives here, with no Tauri and no
//! platform code, so it is unit-tested on any computer:
//!
//! - [`envelope`] — Argon2id → AES-256-GCM sealed files (vault and backup)
//! - [`totp`] — RFC 6238 authenticator codes
//! - [`lockout`] — escalating, persisted delays after failed attempts
//! - [`recovery`] — ten single-use recovery codes
//! - [`history`] — issued keys: status, search, CSV
//! - [`engine`] — the setup / unlock / issue / backup state machine
//!
//! Key format and signing come from `medflow-license`, the same crate the
//! desktop verifier uses. Threat model: apps/keymaker/README.md.

pub mod engine;
pub mod envelope;
pub mod error;
pub mod history;
pub mod lockout;
pub mod recovery;
pub mod rules;
pub mod storage;
pub mod totp;
pub mod vault;

pub use engine::{Clock, Engine, IssueForm, SecondFactor, SystemClock, TermInput};
pub use error::{Error, Result};
