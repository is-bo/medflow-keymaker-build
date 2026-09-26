//! The Key Maker state machine: setup wizard → locked ⇄ unlocked.
//!
//! Every screen action is one method here; the Tauri layer only forwards
//! them. Nothing secret is ever written outside the encrypted vault, and the
//! decrypted vault only lives in memory while unlocked (zeroized on lock).

use crate::envelope::{self, KdfParams, OpenError, SealKey, BACKUP_FORMAT, VAULT_FORMAT};
use crate::error::{Error, Result};
use crate::history::{self, HistoryItem, IssuedKey};
use crate::lockout::{seconds_ceil, Lockout};
use crate::recovery::RecoveryCodes;
use crate::vault::{TotpConfig, VaultData, VAULT_SCHEMA};
use crate::{rules, storage, totp};
use medflow_license::phrase::{phrase_to_seed, seed_to_phrase, PhraseError};
use medflow_license::{
    generate_seed, issue_key, public_key_from_seed, trusted_key_rust_row, verify, KeyRequest,
    MachineCode, MachineCodeError, Term, TrustStore, TrustedKey,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

/// MedFlow major version the keys unlock (MedFlow 2.x).
pub const APP_MAJOR: u32 = 2;
/// Auto-lock after this long without any action.
pub const IDLE_MS: i64 = 2 * 60_000;
/// While the wizard is unfinished (copying 24 words and 10 codes by hand takes
/// longer than 2 minutes) the idle limit is longer. Leaving the app still
/// locks at once (the webview calls `lock` on background).
pub const SETUP_IDLE_MS: i64 = 15 * 60_000;
/// An unfinished setup draft (unsaved seed in memory) is dropped after this.
pub const DRAFT_TTL_MS: i64 = 30 * 60_000;
pub const VAULT_FILE: &str = "keymaker.vault";
pub const LOCKOUT_FILE: &str = "lockout.json";
pub const BACKUP_EXTENSION: &str = "mfkbackup";

pub trait Clock: Send {
    /// Wall clock, unix ms (key dates, TOTP, lockout deadlines).
    fn now_ms(&self) -> i64;
    /// Monotonic ms (idle timeout that a clock change cannot stretch).
    fn mono_ms(&self) -> i64;
}

pub struct SystemClock {
    start: Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        SystemClock { start: Instant::now() }
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
    fn mono_ms(&self) -> i64 {
        self.start.elapsed().as_millis() as i64
    }
}

// ── Values returned to the UI ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// `setup` (no vault) / `locked` / `unlocked`.
    pub phase: &'static str,
    /// Seconds before the next unlock attempt is accepted.
    pub wait_seconds: u64,
    pub setup_finished: bool,
    pub kid: Option<String>,
    pub history_count: usize,
    pub keys_since_backup: usize,
    pub last_backup_at: Option<i64>,
    pub recovery_codes_left: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInfo {
    pub kid: String,
    /// The line the developer pastes into `TRUSTED_KEYS`.
    pub row: String,
    /// Short hash of the public key, to compare over the phone.
    pub fingerprint: String,
    pub public_key_hex: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStarted {
    pub phrase: Vec<String>,
    pub key: KeyInfo,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub key: KeyInfo,
    pub history_count: usize,
    pub created_at: i64,
    pub last_backup_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpEnrollment {
    /// Base32 secret in groups of 4, for typing into an authenticator.
    pub secret: String,
    pub uri: String,
    pub account: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockResult {
    pub used_recovery_code: bool,
    pub recovery_codes_left: usize,
    pub setup_finished: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverySheet {
    pub key: KeyInfo,
    pub phrase: Vec<String>,
    pub codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedBackup {
    pub file_name: String,
    pub contents: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineCheck {
    pub ok: bool,
    pub canonical: Option<String>,
    /// `empty` / `wrong_length` / `invalid_character` / `checksum_mismatch`.
    pub error: Option<&'static str>,
    pub symbols: Option<usize>,
    pub position: Option<usize>,
    pub character: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhraseCheck {
    pub word_count: usize,
    /// 1-based positions of words not on the BIP39 list.
    pub unknown: Vec<usize>,
    pub valid: bool,
    /// `word_count` / `unknown_word` / `checksum`.
    pub error: Option<&'static str>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermInput {
    /// `months` / `years` / `days` / `lifetime`.
    pub kind: String,
    pub count: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueForm {
    pub licensee: String,
    #[serde(default)]
    pub phone: String,
    pub machine_code: String,
    pub term: TermInput,
    #[serde(default)]
    pub telegram: bool,
    #[serde(default)]
    pub renewal_of: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "code")]
pub enum SecondFactor {
    Totp(String),
    RecoveryCode(String),
}

// ── Internal state ───────────────────────────────────────────────────────────

struct Session {
    key: SealKey,
    data: VaultData,
    last_mono: i64,
    last_wall: i64,
    /// Plain recovery codes, only between their generation and the end of setup.
    fresh_codes: Option<Vec<String>>,
    /// Secret being enrolled from Settings → New authenticator.
    pending_totp: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DraftSource {
    New,
    Phrase,
    Backup,
}

struct Draft {
    source: DraftSource,
    key: SealKey,
    seed: Zeroizing<[u8; 32]>,
    kid: String,
    created_at: i64,
    started_mono: i64,
    history: Vec<IssuedKey>,
    last_backup_at: Option<i64>,
    keys_at_last_backup: Option<usize>,
    restored_totp: Option<TotpConfig>,
    restored_recovery: Option<RecoveryCodes>,
    phrase_verified: bool,
    pending_totp: Option<String>,
    totp: Option<TotpConfig>,
    keep_recovery: bool,
}

pub struct Engine {
    dir: PathBuf,
    clock: Box<dyn Clock>,
    kdf: KdfParams,
    session: Option<Session>,
    draft: Option<Draft>,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

pub fn key_info(kid: &str, seed: &[u8; 32]) -> KeyInfo {
    let pk = public_key_from_seed(seed);
    let digest = Sha256::digest(pk);
    let hex = hex::encode(&digest[..8]).to_uppercase();
    let fingerprint = hex
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).expect("hex is ascii"))
        .collect::<Vec<_>>()
        .join(" ");
    KeyInfo {
        kid: kid.to_string(),
        row: trusted_key_rust_row(kid, &pk),
        fingerprint,
        public_key_hex: hex::encode(pk),
    }
}

fn group4(secret: &str) -> String {
    secret
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).expect("base32 is ascii"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn enrollment(kid: &str, secret: &str) -> TotpEnrollment {
    TotpEnrollment {
        secret: group4(secret),
        uri: totp::otpauth_uri(kid, secret),
        account: kid.to_string(),
    }
}

pub fn check_machine_code(input: &str) -> MachineCheck {
    let mut check = MachineCheck {
        ok: false,
        canonical: None,
        error: None,
        symbols: None,
        position: None,
        character: None,
    };
    match MachineCode::parse(input) {
        Ok(code) => {
            check.ok = true;
            check.canonical = Some(code.as_str().to_string());
        }
        Err(err) => {
            check.error = Some(err.code());
            match err {
                MachineCodeError::WrongLength { symbols } => check.symbols = Some(symbols),
                MachineCodeError::InvalidCharacter { character, position } => {
                    check.position = Some(position);
                    check.character = Some(character.to_string());
                }
                _ => {}
            }
        }
    }
    check
}

fn phrase_error_code(err: &PhraseError) -> &'static str {
    match err {
        PhraseError::WordCount(_) => "word_count",
        PhraseError::UnknownWord { .. } => "unknown_word",
        PhraseError::BadChecksum => "checksum",
    }
}

pub fn check_phrase(input: &str) -> PhraseCheck {
    let words: Vec<String> = input.split_whitespace().map(str::to_lowercase).collect();
    let list = bip39_words();
    let unknown: Vec<usize> = words
        .iter()
        .enumerate()
        .filter(|(_, w)| !list.contains(&w.as_str()))
        .map(|(i, _)| i + 1)
        .collect();
    let result = phrase_to_seed(input);
    PhraseCheck {
        word_count: words.len(),
        unknown,
        valid: result.is_ok(),
        error: result.as_ref().err().map(phrase_error_code),
    }
}

fn bip39_words() -> &'static [&'static str; 2048] {
    bip39::Language::English.word_list()
}

fn parse_term(input: &TermInput) -> Result<Term> {
    let count = input.count;
    let bounded = |max: u32| match count {
        Some(n) if (1..=max).contains(&n) => Ok(n),
        _ => Err(Error::InvalidInput("term")),
    };
    match input.kind.as_str() {
        "months" => Ok(Term::Months(bounded(120)?)),
        "years" => Ok(Term::Years(bounded(10)?)),
        "days" => Ok(Term::Days(bounded(3650)?)),
        "lifetime" => Ok(Term::Lifetime),
        _ => Err(Error::InvalidInput("term")),
    }
}

pub fn preview_expiry(term: &TermInput, now_ms: i64) -> Result<Option<i64>> {
    Ok(parse_term(term)?.expires_at(now_ms))
}

fn storage_err(e: std::io::Error) -> Error {
    Error::Storage(e.to_string())
}

// ── Engine ───────────────────────────────────────────────────────────────────

impl Engine {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self::with(dir, Box::new(SystemClock::default()), envelope::PHONE_KDF)
    }

    /// Injectable clock and KDF cost (tests).
    pub fn with(dir: impl Into<PathBuf>, clock: Box<dyn Clock>, kdf: KdfParams) -> Self {
        Engine {
            dir: dir.into(),
            clock,
            kdf,
            session: None,
            draft: None,
        }
    }

    fn vault_path(&self) -> PathBuf {
        self.dir.join(VAULT_FILE)
    }

    fn lockout_path(&self) -> PathBuf {
        self.dir.join(LOCKOUT_FILE)
    }

    pub fn vault_exists(&self) -> bool {
        self.vault_path().exists()
    }

    fn now(&self) -> i64 {
        self.clock.now_ms()
    }

    fn expire_idle(&mut self) {
        let (mono, wall) = (self.clock.mono_ms(), self.clock.now_ms());
        if let Some(s) = &self.session {
            let limit = if s.data.setup_finished { IDLE_MS } else { SETUP_IDLE_MS };
            if mono - s.last_mono > limit || wall - s.last_wall > limit {
                self.session = None;
            }
        }
        if let Some(d) = &self.draft {
            if mono - d.started_mono > DRAFT_TTL_MS {
                self.draft = None;
            }
        }
    }

    /// The unlocked session, refreshing its idle timer; `Locked` if none.
    fn session(&mut self) -> Result<&mut Session> {
        self.expire_idle();
        let (mono, wall) = (self.clock.mono_ms(), self.clock.now_ms());
        let s = self.session.as_mut().ok_or(Error::Locked)?;
        s.last_mono = mono;
        s.last_wall = wall;
        Ok(s)
    }

    fn draft(&mut self) -> Result<&mut Draft> {
        self.expire_idle();
        self.draft.as_mut().ok_or(Error::SetupOrder("no_draft"))
    }

    fn save_vault(dir: &Path, session: &Session) -> Result<()> {
        let json = Zeroizing::new(serde_json::to_vec(&session.data).expect("vault serializes"));
        let text = session.key.seal(VAULT_FORMAT, &json);
        storage::write_atomic(&dir.join(VAULT_FILE), text.as_bytes()).map_err(storage_err)
    }

    fn save(&mut self) -> Result<()> {
        let dir = self.dir.clone();
        let session = self.session.as_ref().ok_or(Error::Locked)?;
        Self::save_vault(&dir, session)
    }

    pub fn status(&mut self) -> Status {
        self.expire_idle();
        let wait = Lockout::load(&self.lockout_path()).wait_ms(self.now());
        let exists = self.vault_exists();
        match &self.session {
            Some(s) => Status {
                phase: "unlocked",
                wait_seconds: 0,
                setup_finished: s.data.setup_finished,
                kid: Some(s.data.kid.clone()),
                history_count: s.data.history.len(),
                keys_since_backup: s.data.keys_since_backup(),
                last_backup_at: s.data.last_backup_at,
                recovery_codes_left: Some(s.data.recovery.remaining()),
            },
            None => Status {
                phase: if exists { "locked" } else { "setup" },
                wait_seconds: if exists { seconds_ceil(wait) } else { 0 },
                setup_finished: exists,
                kid: None,
                history_count: 0,
                keys_since_backup: 0,
                last_backup_at: None,
                recovery_codes_left: None,
            },
        }
    }

    // ── Setup: new key, phrase restore, backup restore ───────────────────────

    fn require_no_vault(&self) -> Result<()> {
        if self.vault_exists() {
            Err(Error::VaultExists)
        } else {
            Ok(())
        }
    }

    fn check_new_password(password: &str) -> Result<()> {
        let problems = rules::password_problems(password);
        if problems.is_empty() {
            Ok(())
        } else {
            Err(Error::WeakPassword(problems))
        }
    }

    fn new_draft(&self, source: DraftSource, key: SealKey, seed: Zeroizing<[u8; 32]>, kid: String) -> Draft {
        Draft {
            source,
            key,
            seed,
            kid,
            created_at: self.now(),
            started_mono: self.clock.mono_ms(),
            history: Vec::new(),
            last_backup_at: None,
            keys_at_last_backup: None,
            restored_totp: None,
            restored_recovery: None,
            phrase_verified: source == DraftSource::Phrase,
            pending_totp: None,
            totp: None,
            keep_recovery: false,
        }
    }

    /// Step 1 of a new setup: generate the signing seed on this device.
    pub fn setup_begin(&mut self, password: &str, kid: &str) -> Result<SetupStarted> {
        self.require_no_vault()?;
        Self::check_new_password(password)?;
        if !rules::kid_is_valid(kid) {
            return Err(Error::BadKid);
        }
        let seed = Zeroizing::new(generate_seed().map_err(|e| Error::Issue(e.to_string()))?);
        let phrase = seed_to_phrase(&seed);
        let key = SealKey::new_random(password, self.kdf);
        let info = key_info(kid, &seed);
        self.draft = Some(self.new_draft(DraftSource::New, key, seed, kid.to_string()));
        Ok(SetupStarted {
            phrase: phrase.split(' ').map(str::to_string).collect(),
            key: info,
        })
    }

    /// Restore a lost phone from the 24 words. History starts empty and a new
    /// authenticator must be enrolled.
    pub fn setup_restore_phrase(&mut self, phrase: &str, password: &str, kid: &str) -> Result<SetupStarted> {
        self.require_no_vault()?;
        let seed = Zeroizing::new(
            phrase_to_seed(phrase).map_err(|e| Error::Phrase(phrase_error_code(&e).to_string()))?,
        );
        Self::check_new_password(password)?;
        if !rules::kid_is_valid(kid) {
            return Err(Error::BadKid);
        }
        let key = SealKey::new_random(password, self.kdf);
        let info = key_info(kid, &seed);
        let words = seed_to_phrase(&seed).split(' ').map(str::to_string).collect();
        self.draft = Some(self.new_draft(DraftSource::Phrase, key, seed, kid.to_string()));
        Ok(SetupStarted { phrase: words, key: info })
    }

    /// Restore from an encrypted backup file (opened with its password, which
    /// becomes this phone's vault password).
    pub fn setup_restore_backup(&mut self, backup_text: &str, password: &str) -> Result<BackupSummary> {
        self.require_no_vault()?;
        let (plain, _) = envelope::open(BACKUP_FORMAT, backup_text, password).map_err(|e| match e {
            OpenError::WrongPasswordOrDamaged => Error::BackupWrongPassword,
            _ => Error::BackupInvalid,
        })?;
        let data: VaultData = serde_json::from_slice(&plain).map_err(|_| Error::BackupInvalid)?;
        let seed = data.seed().ok_or(Error::BackupInvalid)?;
        if data.schema != VAULT_SCHEMA || !rules::kid_is_valid(&data.kid) {
            return Err(Error::BackupInvalid);
        }
        let key = SealKey::new_random(password, self.kdf);
        let summary = BackupSummary {
            key: key_info(&data.kid, &seed),
            history_count: data.history.len(),
            created_at: data.created_at,
            last_backup_at: data.last_backup_at,
        };
        let mut draft = self.new_draft(DraftSource::Backup, key, seed, data.kid.clone());
        draft.created_at = data.created_at;
        draft.history = data.history.clone();
        draft.last_backup_at = data.last_backup_at;
        draft.keys_at_last_backup = data.keys_at_last_backup;
        draft.restored_totp = Some(data.totp.clone());
        draft.restored_recovery = Some(data.recovery.clone());
        self.draft = Some(draft);
        Ok(summary)
    }

    /// Phrase quiz: `(1-based position, word)` pairs, at least 3.
    pub fn setup_check_phrase(&mut self, answers: &[(usize, String)]) -> Result<()> {
        let draft = self.draft()?;
        let words: Vec<String> = seed_to_phrase(&draft.seed).split(' ').map(str::to_string).collect();
        let mut positions: Vec<usize> = answers.iter().map(|(p, _)| *p).collect();
        positions.sort_unstable();
        positions.dedup();
        if answers.len() < 3 || positions.len() != answers.len() {
            return Err(Error::InvalidInput("answers"));
        }
        let all_match = answers.iter().all(|(pos, word)| {
            *pos >= 1 && words.get(pos - 1).is_some_and(|w| *w == word.trim().to_lowercase())
        });
        if !all_match {
            return Err(Error::PhraseMismatch);
        }
        draft.phrase_verified = true;
        Ok(())
    }

    pub fn setup_new_totp(&mut self) -> Result<TotpEnrollment> {
        let draft = self.draft()?;
        let secret = totp::generate_secret();
        let info = enrollment(&draft.kid, &secret);
        draft.pending_totp = Some(secret);
        Ok(info)
    }

    pub fn setup_confirm_totp(&mut self, code: &str) -> Result<()> {
        let now = self.now();
        let draft = self.draft()?;
        let secret = draft.pending_totp.clone().ok_or(Error::SetupOrder("totp_not_started"))?;
        let step = totp::verify(&secret, code, now, None).ok_or(Error::BadTotpCode)?;
        draft.totp = Some(TotpConfig {
            secret_b32: secret,
            enrolled_at: now,
            last_used_step: Some(step),
        });
        draft.keep_recovery = false;
        Ok(())
    }

    /// Backup restore only: keep the authenticator the backup was set up with.
    pub fn setup_keep_totp(&mut self, code: &str) -> Result<()> {
        let now = self.now();
        let draft = self.draft()?;
        let mut restored = draft.restored_totp.clone().ok_or(Error::SetupOrder("no_restored_totp"))?;
        let step = totp::verify(&restored.secret_b32, code, now, restored.last_used_step)
            .ok_or(Error::BadTotpCode)?;
        restored.last_used_step = Some(step);
        draft.totp = Some(restored);
        draft.keep_recovery = true;
        Ok(())
    }

    /// Write the vault and unlock. Returns the new recovery codes to show once
    /// (empty when a restored backup keeps its authenticator and codes).
    pub fn setup_commit(&mut self) -> Result<Vec<String>> {
        self.require_no_vault()?;
        let now = self.now();
        let (mono, wall) = (self.clock.mono_ms(), now);
        let draft = self.draft()?;
        if draft.source == DraftSource::New && !draft.phrase_verified {
            return Err(Error::SetupOrder("phrase_not_verified"));
        }
        let totp = draft.totp.clone().ok_or(Error::SetupOrder("totp_not_confirmed"))?;
        let (recovery, fresh) = match (&draft.restored_recovery, draft.keep_recovery) {
            (Some(existing), true) => (existing.clone(), Vec::new()),
            _ => RecoveryCodes::generate(),
        };
        let draft = self.draft.take().expect("checked above");
        let data = VaultData {
            schema: VAULT_SCHEMA,
            seed_hex: hex::encode(draft.seed.as_ref()),
            kid: draft.kid.clone(),
            created_at: draft.created_at,
            totp,
            recovery,
            history: draft.history.clone(),
            last_backup_at: draft.last_backup_at,
            keys_at_last_backup: draft.keys_at_last_backup,
            setup_finished: false,
            setup_committed_at: now,
        };
        let session = Session {
            key: draft.key,
            data,
            last_mono: mono,
            last_wall: wall,
            fresh_codes: Some(fresh.clone()),
            pending_totp: None,
        };
        Self::save_vault(&self.dir, &session)?;
        Lockout::default().save(&self.lockout_path()).map_err(storage_err)?;
        self.session = Some(session);
        Ok(fresh)
    }

    pub fn cancel_setup(&mut self) {
        self.draft = None;
    }

    /// The recovery sheet: phrase + codes, only while setup is unfinished.
    pub fn recovery_sheet(&mut self) -> Result<RecoverySheet> {
        let s = self.session()?;
        if s.data.setup_finished {
            return Err(Error::SetupOrder("setup_finished"));
        }
        let seed = s.data.seed().ok_or(Error::Corrupt)?;
        Ok(RecoverySheet {
            key: key_info(&s.data.kid, &seed),
            phrase: seed_to_phrase(&seed).split(' ').map(str::to_string).collect(),
            codes: s.fresh_codes.clone().unwrap_or_default(),
        })
    }

    /// Last wizard step. Requires a backup exported after the vault was created.
    pub fn finish_setup(&mut self) -> Result<()> {
        let s = self.session()?;
        let backed_up = s
            .data
            .last_backup_at
            .is_some_and(|at| at >= s.data.setup_committed_at);
        if !backed_up {
            return Err(Error::SetupOrder("backup_required"));
        }
        s.data.setup_finished = true;
        s.fresh_codes = None;
        self.save()
    }

    // ── Lock / unlock ────────────────────────────────────────────────────────

    pub fn lock(&mut self) {
        self.session = None;
    }

    pub fn touch(&mut self) -> Result<()> {
        self.session().map(|_| ())
    }

    fn load_lockout_and_begin(&self) -> Result<Lockout> {
        let now = self.now();
        let mut lockout = Lockout::load(&self.lockout_path());
        let wait = lockout.wait_ms(now);
        if wait > 0 {
            return Err(Error::TooManyAttempts {
                wait_seconds: seconds_ceil(wait),
            });
        }
        lockout.begin_attempt(now);
        lockout.save(&self.lockout_path()).map_err(storage_err)?;
        Ok(lockout)
    }

    fn wrong(&self, lockout: &Lockout) -> Error {
        Error::WrongCredentials {
            wait_seconds: seconds_ceil(lockout.wait_ms(self.now())),
        }
    }

    pub fn unlock(&mut self, password: &str, factor: &SecondFactor) -> Result<UnlockResult> {
        if !self.vault_exists() {
            return Err(Error::NoVault);
        }
        self.session = None;
        let mut lockout = self.load_lockout_and_begin()?;
        let text = std::fs::read_to_string(self.vault_path()).map_err(storage_err)?;
        let (plain, key) = match envelope::open(VAULT_FORMAT, &text, password) {
            Ok(opened) => opened,
            Err(OpenError::WrongPasswordOrDamaged) => return Err(self.wrong(&lockout)),
            Err(_) => return Err(Error::Corrupt),
        };
        let mut data: VaultData = serde_json::from_slice(&plain).map_err(|_| Error::Corrupt)?;
        let now = self.now();
        let used_recovery_code = match factor {
            SecondFactor::Totp(code) => {
                match totp::verify(&data.totp.secret_b32, code, now, data.totp.last_used_step) {
                    Some(step) => {
                        data.totp.last_used_step = Some(step);
                        false
                    }
                    None => return Err(self.wrong(&lockout)),
                }
            }
            SecondFactor::RecoveryCode(code) => {
                if !data.recovery.consume(code, now) {
                    return Err(self.wrong(&lockout));
                }
                true
            }
        };
        let mut session = Session {
            key,
            data,
            last_mono: self.clock.mono_ms(),
            last_wall: now,
            fresh_codes: None,
            pending_totp: None,
        };
        // Resuming an unfinished wizard: the codes shown before the restart
        // may never have been written down, so issue a fresh set.
        if !session.data.setup_finished {
            let (recovery, fresh) = RecoveryCodes::generate();
            session.data.recovery = recovery;
            session.fresh_codes = Some(fresh);
        }
        // Persist the consumed code / TOTP step before granting access, so a
        // recovery code can never be used twice even if the app dies now.
        Self::save_vault(&self.dir, &session)?;
        lockout.succeed();
        lockout.save(&self.lockout_path()).map_err(storage_err)?;
        let result = UnlockResult {
            used_recovery_code,
            recovery_codes_left: session.data.recovery.remaining(),
            setup_finished: session.data.setup_finished,
        };
        self.session = Some(session);
        Ok(result)
    }

    /// Re-enter the password inside an unlocked session (backup export, phrase
    /// display, …). Counts toward the same lockout as unlocking.
    fn reauth(&mut self, password: &str) -> Result<()> {
        self.session()?;
        let mut lockout = self.load_lockout_and_begin()?;
        let ok = self
            .session
            .as_ref()
            .ok_or(Error::Locked)?
            .key
            .matches_password(password);
        if !ok {
            return Err(self.wrong(&lockout));
        }
        lockout.succeed();
        lockout.save(&self.lockout_path()).map_err(storage_err)?;
        // The Argon2 check took real time: refresh the idle timer.
        self.session().map(|_| ())
    }

    /// "Forgot my password": delete the vault so a restore can run. The seed
    /// is only recoverable from the 24 words or a backup after this.
    pub fn erase_vault(&mut self) -> Result<()> {
        self.session = None;
        self.draft = None;
        for path in [self.vault_path(), self.lockout_path()] {
            if path.exists() {
                std::fs::remove_file(&path).map_err(storage_err)?;
            }
        }
        Ok(())
    }

    // ── Issuing and history ──────────────────────────────────────────────────

    pub fn issue(&mut self, form: &IssueForm) -> Result<HistoryItem> {
        let now = self.now();
        let licensee = form.licensee.trim().to_string();
        if licensee.is_empty() || licensee.chars().count() > 120 {
            return Err(Error::InvalidInput("licensee"));
        }
        let phone = form.phone.trim().to_string();
        let phone_ok = phone.chars().count() <= 40
            && phone
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '+' | '-' | '(' | ')' | '.' | '/'));
        if !phone_ok {
            return Err(Error::InvalidInput("phone"));
        }
        let machine = MachineCode::parse(&form.machine_code).map_err(|e| Error::MachineCode(e.code()))?;
        let term = parse_term(&form.term)?;
        let features = if form.telegram { vec!["telegram".to_string()] } else { Vec::new() };

        let dir = self.dir.clone();
        let s = self.session()?;
        let seed = s.data.seed().ok_or(Error::Corrupt)?;
        let kid = s.data.kid.clone();
        let renewal_of = form
            .renewal_of
            .clone()
            .filter(|id| s.data.history.iter().any(|k| &k.license_id == id));
        let (key, license) = issue_key(
            &seed,
            &kid,
            &KeyRequest {
                licensee: licensee.clone(),
                machine_code: machine.clone(),
                major: APP_MAJOR,
                features,
                issued_at: now,
                term,
                license_id: None,
            },
        )
        .map_err(|e| Error::Issue(e.to_string()))?;

        // Self-check with the same verifier the desktop runs: a key that would
        // not activate is never handed out.
        let trusted = [TrustedKey {
            kid: &kid,
            public_key: public_key_from_seed(&seed),
        }];
        let store = TrustStore {
            keys: &trusted,
            revoked_kids: &[],
            revoked_license_ids: &[],
        };
        if !verify(&store, &key, &machine, APP_MAJOR, now).permits_work() {
            return Err(Error::Issue("self_check".into()));
        }

        let entry = IssuedKey {
            license_id: license.license_id.clone(),
            key,
            licensee,
            phone,
            machine_code: machine.as_str().to_string(),
            features: license.features.clone(),
            issued_at: license.issued_at,
            expires_at: license.expires_at,
            term_kind: term.kind().to_string(),
            term_count: term.count(),
            kid,
            renewal_of,
        };
        s.data.history.push(entry.clone());
        if let Err(e) = Self::save_vault(&dir, s) {
            // Never hand out a key that is not in the saved history.
            s.data.history.pop();
            return Err(e);
        }
        Ok(history::item(&entry, now))
    }

    /// Newest first, optionally filtered by name / phone / machine code.
    pub fn history(&mut self, query: &str) -> Result<Vec<HistoryItem>> {
        let now = self.now();
        let s = self.session()?;
        Ok(s.data
            .history
            .iter()
            .rev()
            .filter(|k| history::matches(k, query))
            .map(|k| history::item(k, now))
            .collect())
    }

    pub fn history_csv(&mut self) -> Result<String> {
        let now = self.now();
        let s = self.session()?;
        Ok(history::to_csv(&s.data.history, now))
    }

    // ── Backup and settings ──────────────────────────────────────────────────

    /// Encrypted backup of everything (seed, kid, authenticator, recovery-code
    /// hashes, history), locked with the vault password re-typed now.
    pub fn export_backup(&mut self, password: &str) -> Result<ExportedBackup> {
        self.reauth(password)?;
        let now = self.now();
        let kdf = self.kdf;
        let s = self.session()?;
        let previous = (s.data.last_backup_at, s.data.keys_at_last_backup);
        s.data.last_backup_at = Some(now);
        s.data.keys_at_last_backup = Some(s.data.history.len());
        let json = Zeroizing::new(serde_json::to_vec(&s.data).expect("vault serializes"));
        let contents = SealKey::new_random(password, kdf).seal(BACKUP_FORMAT, &json);
        let file_name = format!(
            "medflow-keymaker-backup-{}-{}.{BACKUP_EXTENSION}",
            s.data.kid,
            history::iso_date(now)
        );
        if let Err(e) = self.save() {
            if let Some(s) = self.session.as_mut() {
                (s.data.last_backup_at, s.data.keys_at_last_backup) = previous;
            }
            return Err(e);
        }
        Ok(ExportedBackup { file_name, contents })
    }

    pub fn public_key(&mut self) -> Result<KeyInfo> {
        let s = self.session()?;
        let seed = s.data.seed().ok_or(Error::Corrupt)?;
        Ok(key_info(&s.data.kid, &seed))
    }

    pub fn reveal_phrase(&mut self, password: &str) -> Result<Vec<String>> {
        self.reauth(password)?;
        let s = self.session()?;
        let seed = s.data.seed().ok_or(Error::Corrupt)?;
        Ok(seed_to_phrase(&seed).split(' ').map(str::to_string).collect())
    }

    pub fn regenerate_recovery_codes(&mut self, password: &str) -> Result<Vec<String>> {
        self.reauth(password)?;
        let (codes, plain) = RecoveryCodes::generate();
        let s = self.session()?;
        s.data.recovery = codes;
        self.save()?;
        Ok(plain)
    }

    pub fn reenroll_totp_begin(&mut self, password: &str) -> Result<TotpEnrollment> {
        self.reauth(password)?;
        let s = self.session()?;
        let secret = totp::generate_secret();
        let info = enrollment(&s.data.kid, &secret);
        s.pending_totp = Some(secret);
        Ok(info)
    }

    pub fn reenroll_totp_confirm(&mut self, code: &str) -> Result<()> {
        let now = self.now();
        let s = self.session()?;
        let secret = s.pending_totp.clone().ok_or(Error::SetupOrder("totp_not_started"))?;
        let step = totp::verify(&secret, code, now, None).ok_or(Error::BadTotpCode)?;
        s.data.totp = TotpConfig {
            secret_b32: secret,
            enrolled_at: now,
            last_used_step: Some(step),
        };
        s.pending_totp = None;
        self.save()
    }

    pub fn change_password(&mut self, old: &str, new: &str) -> Result<()> {
        Self::check_new_password(new)?;
        self.reauth(old)?;
        let kdf = self.kdf;
        let s = self.session()?;
        s.key = SealKey::new_random(new, kdf);
        self.save()
    }
}
