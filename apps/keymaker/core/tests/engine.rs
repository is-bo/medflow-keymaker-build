//! End-to-end tests of the Key Maker engine against real files in a temp dir.

use medflow_keymaker_core::engine::{check_machine_code, check_phrase, key_info, APP_MAJOR, VAULT_FILE};
use medflow_keymaker_core::envelope::TEST_KDF;
use medflow_keymaker_core::{totp, Clock, Engine, Error, IssueForm, SecondFactor, TermInput};
use medflow_license::{verify, KeyStatus, MachineCode, TrustStore, TrustedKey};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

const PASSWORD: &str = "Tlemcen-2026";
const KID: &str = "prod-2026-09";
const T0: i64 = 1_790_000_000_000;

#[derive(Clone)]
struct TestClock(Arc<AtomicI64>);

impl TestClock {
    fn advance(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
    fn mono_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst) - T0
    }
}

struct Fixture {
    dir: PathBuf,
    clock: TestClock,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("km-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Fixture {
            dir,
            clock: TestClock(Arc::new(AtomicI64::new(T0))),
        }
    }

    /// A fresh engine on the same files — i.e. the app was restarted.
    fn engine(&self) -> Engine {
        Engine::with(&self.dir, Box::new(self.clock.clone()), TEST_KDF)
    }

    fn code(&self, secret: &str) -> String {
        let bytes = totp::base32_decode(secret).unwrap();
        totp::totp_at(&bytes, self.clock.now_ms() / 1000, 6)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct Setup {
    phrase: Vec<String>,
    secret: String,
    codes: Vec<String>,
    public_key_hex: String,
    backup: String,
}

/// The whole wizard, as the UI drives it.
fn run_setup(fx: &Fixture, engine: &mut Engine) -> Setup {
    let started = engine.setup_begin(PASSWORD, KID).unwrap();
    assert_eq!(started.phrase.len(), 24);
    assert!(started
        .key
        .row
        .starts_with("TrustedKey { kid: \"prod-2026-09\", public_key: ["));

    let enrollment = engine.setup_new_totp().unwrap();
    assert!(enrollment
        .uri
        .starts_with("otpauth://totp/MedFlow%20Key%20Maker:prod-2026-09?secret="));
    let wrong = vec![
        (3, "zzz".to_string()),
        (7, started.phrase[6].clone()),
        (19, started.phrase[18].clone()),
    ];
    assert_eq!(engine.setup_check_phrase(&wrong), Err(Error::PhraseMismatch));
    engine.setup_confirm_totp(&fx.code(&enrollment.secret)).unwrap();
    // Commit is refused until the phrase quiz passes.
    assert_eq!(engine.setup_commit(), Err(Error::SetupOrder("phrase_not_verified")));
    let quiz: Vec<(usize, String)> = [3usize, 7, 19, 24]
        .iter()
        .map(|&p| (p, format!(" {} ", started.phrase[p - 1].to_uppercase())))
        .collect();
    engine.setup_check_phrase(&quiz).unwrap();

    let codes = engine.setup_commit().unwrap();
    assert_eq!(codes.len(), 10);
    let sheet = engine.recovery_sheet().unwrap();
    assert_eq!(sheet.phrase, started.phrase);
    assert_eq!(sheet.codes, codes);

    // The wizard cannot finish before a backup exists.
    assert_eq!(engine.finish_setup(), Err(Error::SetupOrder("backup_required")));
    assert!(matches!(
        engine.export_backup("not-the-password1"),
        Err(Error::WrongCredentials { .. })
    ));
    let backup = engine.export_backup(PASSWORD).unwrap();
    assert!(backup.file_name.ends_with(".mfkbackup"));
    engine.finish_setup().unwrap();
    assert!(engine.recovery_sheet().is_err(), "the sheet is only for the wizard");

    Setup {
        phrase: started.phrase,
        secret: enrollment.secret,
        codes,
        public_key_hex: started.key.public_key_hex,
        backup: backup.contents,
    }
}

fn form(machine: &str, term: (&str, Option<u32>), telegram: bool) -> IssueForm {
    IssueForm {
        licensee: "Dr Amina Benali".into(),
        phone: "+213 555 12 34 56".into(),
        machine_code: machine.into(),
        term: TermInput {
            kind: term.0.into(),
            count: term.1,
        },
        telegram,
        renewal_of: None,
    }
}

fn verify_with(public_key_hex: &str, key: &str, machine: &MachineCode, now: i64) -> KeyStatus {
    let pk: [u8; 32] = hex::decode(public_key_hex).unwrap().try_into().unwrap();
    let trusted = [TrustedKey { kid: KID, public_key: pk }];
    let store = TrustStore {
        keys: &trusted,
        revoked_kids: &[],
        revoked_license_ids: &[],
    };
    verify(&store, key, machine, APP_MAJOR, now)
}

#[test]
fn setup_issue_and_verify_round_trip() {
    let fx = Fixture::new("issue");
    let mut engine = fx.engine();
    assert_eq!(engine.status().phase, "setup");
    let setup = run_setup(&fx, &mut engine);
    assert_eq!(engine.status().phase, "unlocked");

    let machine = MachineCode::derive("doctor-pc");
    // Live validation catches a typo in the last symbol.
    let mut typo = machine.as_str().to_string();
    let last = typo.pop().unwrap();
    typo.push(if last == 'A' { 'B' } else { 'A' });
    assert_eq!(check_machine_code(&typo).error, Some("checksum_mismatch"));
    assert_eq!(check_machine_code("MF-12").error, Some("wrong_length"));
    assert!(check_machine_code(&machine.as_str().to_lowercase().replace('-', " ")).ok);
    assert!(matches!(
        engine.issue(&form(&typo, ("months", Some(3)), false)),
        Err(Error::MachineCode("checksum_mismatch"))
    ));
    assert!(matches!(
        engine.issue(&form(machine.as_str(), ("months", Some(0)), false)),
        Err(Error::InvalidInput("term"))
    ));

    let issued = engine
        .issue(&form(machine.as_str(), ("years", Some(1)), true))
        .unwrap();
    assert_eq!(issued.entry.features, vec!["telegram"]);
    let status = verify_with(&setup.public_key_hex, &issued.entry.key, &machine, T0 + 1000);
    assert!(
        matches!(status, KeyStatus::Valid(ref l) if l.licensee == "Dr Amina Benali" && l.has_feature("telegram"))
    );
    // Bound to the machine.
    let other = MachineCode::derive("another-pc");
    assert!(matches!(
        verify_with(&setup.public_key_hex, &issued.entry.key, &other, T0),
        KeyStatus::WrongMachine(_)
    ));

    let lifetime = engine
        .issue(&form(machine.as_str(), ("lifetime", None), false))
        .unwrap();
    assert_eq!(lifetime.entry.expires_at, None);

    let history = engine.history("").unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].entry.license_id, lifetime.entry.license_id, "newest first");
    assert_eq!(engine.history("benali").unwrap().len(), 2);
    assert_eq!(engine.history("nobody").unwrap().len(), 0);
    assert_eq!(engine.status().keys_since_backup, 2);
    assert!(engine.history_csv().unwrap().contains(&issued.entry.license_id));

    // The vault on disk holds no plaintext secret or customer data.
    let raw = std::fs::read_to_string(fx.dir.join(VAULT_FILE)).unwrap();
    assert!(!raw.contains("Benali"));
    assert!(!raw.contains(&setup.secret.replace(' ', "")));
    assert!(!raw.contains(&setup.phrase[..3].join(" ")));
}

#[test]
fn unlock_needs_password_and_totp_and_refuses_replay() {
    let fx = Fixture::new("unlock");
    let mut engine = fx.engine();
    let setup = run_setup(&fx, &mut engine);
    engine.lock();
    assert_eq!(engine.status().phase, "locked");
    assert!(matches!(engine.history(""), Err(Error::Locked)));

    fx.clock.advance(60_000);
    let code = fx.code(&setup.secret);
    assert!(matches!(
        engine.unlock("wrong password 1", &SecondFactor::Totp(code.clone())),
        Err(Error::WrongCredentials { wait_seconds: 0 })
    ));
    assert!(matches!(
        engine.unlock(PASSWORD, &SecondFactor::Totp("000000".into())),
        Err(Error::WrongCredentials { wait_seconds: 0 })
    ));
    let result = engine.unlock(PASSWORD, &SecondFactor::Totp(code.clone())).unwrap();
    assert!(!result.used_recovery_code);
    assert!(result.setup_finished);
    engine.lock();
    // The same code in the same 30 s window: refused (no replay).
    assert!(matches!(
        engine.unlock(PASSWORD, &SecondFactor::Totp(code)),
        Err(Error::WrongCredentials { .. })
    ));
    fx.clock.advance(30_000);
    engine
        .unlock(PASSWORD, &SecondFactor::Totp(fx.code(&setup.secret)))
        .unwrap();
}

#[test]
fn lockout_escalates_and_survives_restart() {
    let fx = Fixture::new("lockout");
    let mut engine = fx.engine();
    let setup = run_setup(&fx, &mut engine);
    engine.lock();
    let bad = || SecondFactor::Totp("123456".into());

    for expected in [0u64, 0, 30] {
        match engine.unlock("nope-12345", &bad()) {
            Err(Error::WrongCredentials { wait_seconds }) => assert_eq!(wait_seconds, expected),
            other => panic!("unexpected {other:?}"),
        }
    }

    // Restart the app: the delay is still there, and even the right answer is
    // not checked during it.
    let mut engine = fx.engine();
    assert_eq!(engine.status().wait_seconds, 30);
    let good = SecondFactor::Totp(fx.code(&setup.secret));
    assert!(matches!(
        engine.unlock(PASSWORD, &good),
        Err(Error::TooManyAttempts { wait_seconds: 30 })
    ));

    fx.clock.advance(30_000);
    assert!(matches!(
        engine.unlock("nope-12345", &bad()),
        Err(Error::WrongCredentials { wait_seconds: 300 })
    ));
    fx.clock.advance(300_000);
    assert!(matches!(
        engine.unlock("nope-12345", &bad()),
        Err(Error::WrongCredentials { wait_seconds: 3600 })
    ));
    fx.clock.advance(3_600_000);
    assert!(matches!(
        engine.unlock("nope-12345", &bad()),
        Err(Error::WrongCredentials { wait_seconds: 3600 })
    ));
    fx.clock.advance(3_600_000);
    engine
        .unlock(PASSWORD, &SecondFactor::Totp(fx.code(&setup.secret)))
        .unwrap();
    assert_eq!(fx.engine().status().wait_seconds, 0);
}

#[test]
fn recovery_codes_are_single_use() {
    let fx = Fixture::new("recovery");
    let mut engine = fx.engine();
    let setup = run_setup(&fx, &mut engine);
    engine.lock();
    let code = SecondFactor::RecoveryCode(setup.codes[4].to_lowercase());
    let result = engine.unlock(PASSWORD, &code).unwrap();
    assert!(result.used_recovery_code);
    assert_eq!(result.recovery_codes_left, 9);
    engine.lock();
    // Even after a restart the used code stays used.
    let mut engine = fx.engine();
    assert!(matches!(
        engine.unlock(PASSWORD, &code),
        Err(Error::WrongCredentials { .. })
    ));
    // A recovery code never replaces the password.
    let other = SecondFactor::RecoveryCode(setup.codes[5].clone());
    assert!(matches!(
        engine.unlock("wrong-pass-1", &other),
        Err(Error::WrongCredentials { .. })
    ));
    engine.unlock(PASSWORD, &other).unwrap();

    let fresh = engine.regenerate_recovery_codes(PASSWORD).unwrap();
    engine.lock();
    assert!(engine
        .unlock(PASSWORD, &SecondFactor::RecoveryCode(setup.codes[6].clone()))
        .is_err());
    engine
        .unlock(PASSWORD, &SecondFactor::RecoveryCode(fresh[0].clone()))
        .unwrap();
}

#[test]
fn unfinished_wizard_gets_a_longer_idle_limit() {
    let fx = Fixture::new("idle-setup");
    let mut engine = fx.engine();
    let started = engine.setup_begin(PASSWORD, KID).unwrap();
    let quiz: Vec<(usize, String)> = [1usize, 2, 3].iter().map(|&p| (p, started.phrase[p - 1].clone())).collect();
    engine.setup_check_phrase(&quiz).unwrap();
    let enrollment = engine.setup_new_totp().unwrap();
    engine.setup_confirm_totp(&fx.code(&enrollment.secret)).unwrap();
    engine.setup_commit().unwrap();
    fx.clock.advance(10 * 60_000); // copying the recovery sheet by hand
    assert!(engine.recovery_sheet().is_ok());
    fx.clock.advance(16 * 60_000);
    assert!(matches!(engine.recovery_sheet(), Err(Error::Locked)));
}

#[test]
fn idle_timeout_locks() {
    let fx = Fixture::new("idle");
    let mut engine = fx.engine();
    run_setup(&fx, &mut engine);
    fx.clock.advance(119_000);
    engine.touch().unwrap();
    fx.clock.advance(119_000);
    assert!(engine.history("").is_ok(), "activity keeps it open");
    fx.clock.advance(121_000);
    assert!(matches!(engine.history(""), Err(Error::Locked)));
    assert_eq!(engine.status().phase, "locked");
}

#[test]
fn phrase_restore_reproduces_the_same_public_key() {
    let original = Fixture::new("phrase-a");
    let mut engine = original.engine();
    let setup = run_setup(&original, &mut engine);

    let fresh = Fixture::new("phrase-b");
    let mut restored = fresh.engine();
    let phrase = setup.phrase.join("  ").to_uppercase();
    assert!(check_phrase(&phrase).valid);
    let mut broken = setup.phrase.clone();
    broken[4] = "medflow".into();
    let check = check_phrase(&broken.join(" "));
    assert_eq!((check.valid, check.unknown.clone()), (false, vec![5]));

    let started = restored
        .setup_restore_phrase(&phrase, "New-password-9", KID)
        .unwrap();
    assert_eq!(started.key.public_key_hex, setup.public_key_hex);
    // A new authenticator is mandatory.
    assert_eq!(restored.setup_commit(), Err(Error::SetupOrder("totp_not_confirmed")));
    let enrollment = restored.setup_new_totp().unwrap();
    assert_ne!(enrollment.secret, setup.secret);
    restored
        .setup_confirm_totp(&fresh.code(&enrollment.secret))
        .unwrap();
    assert_eq!(restored.setup_commit().unwrap().len(), 10);
    assert_eq!(restored.public_key().unwrap().public_key_hex, setup.public_key_hex);
    assert_eq!(restored.history("").unwrap().len(), 0);

    // Keys it issues verify against the key the desktop already trusts.
    let machine = MachineCode::derive("pc");
    let issued = restored
        .issue(&form(machine.as_str(), ("months", Some(1)), false))
        .unwrap();
    assert!(verify_with(&setup.public_key_hex, &issued.entry.key, &machine, fresh.clock.now_ms()).permits_work());
    assert_eq!(key_info(KID, &[0u8; 32]).fingerprint.len(), 19);
}

#[test]
fn backup_restore_keeps_history_with_a_new_authenticator() {
    let original = Fixture::new("backup-a");
    let mut engine = original.engine();
    run_setup(&original, &mut engine);
    let machine = MachineCode::derive("pc");
    engine
        .issue(&form(machine.as_str(), ("months", Some(6)), false))
        .unwrap();
    assert_eq!(engine.status().keys_since_backup, 1);
    let backup = engine.export_backup(PASSWORD).unwrap();
    assert_eq!(engine.status().keys_since_backup, 0);
    let public = engine.public_key().unwrap();

    let fresh = Fixture::new("backup-b");
    let mut restored = fresh.engine();
    assert_eq!(
        restored.setup_restore_backup(&backup.contents, "wrong-pass-1").err(),
        Some(Error::BackupWrongPassword)
    );
    assert_eq!(
        restored.setup_restore_backup("{}", PASSWORD).err(),
        Some(Error::BackupInvalid)
    );
    let summary = restored.setup_restore_backup(&backup.contents, PASSWORD).unwrap();
    assert_eq!(summary.history_count, 1);
    assert_eq!(summary.key.public_key_hex, public.public_key_hex);
    assert_eq!(restored.setup_keep_totp("000000"), Err(Error::BadTotpCode));

    let enrollment = restored.setup_new_totp().unwrap();
    fresh.clock.advance(60_000);
    restored
        .setup_confirm_totp(&fresh.code(&enrollment.secret))
        .unwrap();
    assert_eq!(restored.setup_commit().unwrap().len(), 10);
    assert_eq!(restored.history("").unwrap().len(), 1);
    assert_eq!(restored.status().keys_since_backup, 0);
    restored.lock();
    fresh.clock.advance(30_000);
    restored
        .unlock(PASSWORD, &SecondFactor::Totp(fresh.code(&enrollment.secret)))
        .unwrap();

    // Restore never overwrites an existing vault.
    assert_eq!(
        restored.setup_restore_backup(&backup.contents, PASSWORD).err(),
        Some(Error::VaultExists)
    );
}

#[test]
fn keep_authenticator_path_resume_and_settings() {
    let original = Fixture::new("keep-a");
    let mut engine = original.engine();
    let setup = run_setup(&original, &mut engine);

    let fresh = Fixture::new("keep-b");
    fresh.clock.advance(90_000);
    let mut restored = fresh.engine();
    restored.setup_restore_backup(&setup.backup, PASSWORD).unwrap();
    restored.setup_keep_totp(&fresh.code(&setup.secret)).unwrap();
    assert!(restored.setup_commit().unwrap().is_empty(), "kept codes are not re-shown");
    restored.lock();
    // After a restart mid-wizard, unlocking resumes it with a fresh code set.
    fresh.clock.advance(30_000);
    let resumed = restored
        .unlock(PASSWORD, &SecondFactor::Totp(fresh.code(&setup.secret)))
        .unwrap();
    assert!(!resumed.setup_finished);
    assert_eq!(restored.recovery_sheet().unwrap().codes.len(), 10);

    // Settings: change password, show phrase, new authenticator.
    assert!(matches!(
        restored.change_password(PASSWORD, "short"),
        Err(Error::WeakPassword(_))
    ));
    restored.change_password(PASSWORD, "Oran-Clinic-77").unwrap();
    assert_eq!(restored.reveal_phrase("Oran-Clinic-77").unwrap(), setup.phrase);
    let enrollment = restored.reenroll_totp_begin("Oran-Clinic-77").unwrap();
    assert_eq!(restored.reenroll_totp_confirm("000000"), Err(Error::BadTotpCode));
    fresh.clock.advance(30_000);
    restored
        .reenroll_totp_confirm(&fresh.code(&enrollment.secret))
        .unwrap();
    restored.lock();
    fresh.clock.advance(30_000);
    assert!(restored
        .unlock("Oran-Clinic-77", &SecondFactor::Totp(fresh.code(&setup.secret)))
        .is_err());
    fresh.clock.advance(30_000);
    restored
        .unlock("Oran-Clinic-77", &SecondFactor::Totp(fresh.code(&enrollment.secret)))
        .unwrap();
}

#[test]
fn setup_refuses_weak_input_and_existing_vault() {
    let fx = Fixture::new("weak");
    let mut engine = fx.engine();
    assert!(matches!(engine.setup_begin("short1", KID), Err(Error::WeakPassword(_))));
    assert_eq!(
        engine.setup_begin(PASSWORD, "Bad Kid").err().map(|e| e.code()),
        Some("bad_kid")
    );
    assert!(matches!(
        engine.setup_restore_phrase("abandon abandon", PASSWORD, KID),
        Err(Error::Phrase(_))
    ));
    run_setup(&fx, &mut engine);
    assert_eq!(engine.setup_begin(PASSWORD, KID).err(), Some(Error::VaultExists));
    engine.erase_vault().unwrap();
    assert_eq!(engine.status().phase, "setup");
}

#[test]
fn errors_serialize_for_the_ui() {
    let json = serde_json::to_value(Error::WrongCredentials { wait_seconds: 30 }).unwrap();
    assert_eq!(json["code"], "wrong_credentials");
    assert_eq!(json["waitSeconds"], 30);
    let json = serde_json::to_value(Error::WeakPassword(vec!["too_short", "needs_digit"])).unwrap();
    assert_eq!(json["detail"], "too_short,needs_digit");
}
