//! One error type for every Key Maker operation. The UI switches on `code()`
//! (stable snake_case) and shows its own translated text; `Display` is the
//! English fallback for logs and tests.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// No vault on this phone yet — run the setup wizard.
    NoVault,
    /// A vault already exists; setup / restore would overwrite it.
    VaultExists,
    /// The session is locked (explicitly, idle timeout, or never unlocked).
    Locked,
    /// A setup step was called out of order (no draft, or a missing step).
    SetupOrder(&'static str),
    /// Password rules failed; the list names each rule (`too_short`, …).
    WeakPassword(Vec<&'static str>),
    /// The key id (kid) is not 3–32 of `a-z 0-9 -`.
    BadKid,
    /// Password, authenticator code or recovery code wrong. The wait is the
    /// delay before the next attempt is accepted (0 = try again now).
    WrongCredentials { wait_seconds: u64 },
    /// Too many failed attempts: nothing was checked, wait this long.
    TooManyAttempts { wait_seconds: u64 },
    /// The 6-digit code does not match the authenticator being enrolled.
    BadTotpCode,
    /// Recovery phrase unreadable (`word_count` / `unknown_word` / `checksum`).
    Phrase(String),
    /// The words typed during the phrase quiz do not match.
    PhraseMismatch,
    /// The file is not a Key Maker backup (or is damaged).
    BackupInvalid,
    /// The backup did not open with that password.
    BackupWrongPassword,
    /// The machine code failed to parse (`MachineCodeError::code()`).
    MachineCode(&'static str),
    /// A form field is invalid; the payload names the field.
    InvalidInput(&'static str),
    /// Signing failed (an `IssueError`, or the self-check after signing).
    Issue(String),
    /// Reading or writing the vault files failed.
    Storage(String),
    /// The vault file exists but cannot be read as a vault.
    Corrupt,
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Error::NoVault => "no_vault",
            Error::VaultExists => "vault_exists",
            Error::Locked => "locked",
            Error::SetupOrder(_) => "setup_order",
            Error::WeakPassword(_) => "weak_password",
            Error::BadKid => "bad_kid",
            Error::WrongCredentials { .. } => "wrong_credentials",
            Error::TooManyAttempts { .. } => "too_many_attempts",
            Error::BadTotpCode => "bad_totp_code",
            Error::Phrase(_) => "phrase_invalid",
            Error::PhraseMismatch => "phrase_mismatch",
            Error::BackupInvalid => "backup_invalid",
            Error::BackupWrongPassword => "backup_wrong_password",
            Error::MachineCode(_) => "machine_code_invalid",
            Error::InvalidInput(_) => "invalid_input",
            Error::Issue(_) => "issue_failed",
            Error::Storage(_) => "storage_failed",
            Error::Corrupt => "vault_corrupt",
        }
    }

    fn wait_seconds(&self) -> Option<u64> {
        match self {
            Error::WrongCredentials { wait_seconds } | Error::TooManyAttempts { wait_seconds } => {
                Some(*wait_seconds)
            }
            _ => None,
        }
    }

    fn detail(&self) -> Option<String> {
        match self {
            Error::SetupOrder(step) | Error::MachineCode(step) | Error::InvalidInput(step) => {
                Some((*step).to_string())
            }
            Error::WeakPassword(rules) => Some(rules.join(",")),
            Error::Phrase(detail) | Error::Issue(detail) | Error::Storage(detail) => {
                Some(detail.clone())
            }
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoVault => write!(f, "no vault on this device"),
            Error::VaultExists => write!(f, "a vault already exists on this device"),
            Error::Locked => write!(f, "locked"),
            Error::SetupOrder(step) => write!(f, "setup step out of order: {step}"),
            Error::WeakPassword(rules) => write!(f, "password rules failed: {}", rules.join(", ")),
            Error::BadKid => write!(f, "the key name must be 3-32 characters of a-z, 0-9 and -"),
            Error::WrongCredentials { wait_seconds } => {
                write!(f, "wrong password or code (next try in {wait_seconds}s)")
            }
            Error::TooManyAttempts { wait_seconds } => {
                write!(f, "too many attempts, wait {wait_seconds}s")
            }
            Error::BadTotpCode => write!(f, "the authenticator code does not match"),
            Error::Phrase(detail) => write!(f, "recovery phrase invalid: {detail}"),
            Error::PhraseMismatch => write!(f, "the words do not match the recovery phrase"),
            Error::BackupInvalid => write!(f, "not a MedFlow Key Maker backup file"),
            Error::BackupWrongPassword => write!(f, "the backup does not open with this password"),
            Error::MachineCode(code) => write!(f, "machine code invalid: {code}"),
            Error::InvalidInput(field) => write!(f, "invalid field: {field}"),
            Error::Issue(detail) => write!(f, "could not sign the key: {detail}"),
            Error::Storage(detail) => write!(f, "storage error: {detail}"),
            Error::Corrupt => write!(f, "the vault file is damaged"),
        }
    }
}

impl std::error::Error for Error {}

/// `{ code, message, waitSeconds?, detail? }` — what the webview receives.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("Error", 4)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.serialize_field("waitSeconds", &self.wait_seconds())?;
        s.serialize_field("detail", &self.detail())?;
        s.end()
    }
}

pub type Result<T> = std::result::Result<T, Error>;
