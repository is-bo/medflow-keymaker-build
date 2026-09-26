//! The Key Maker → desktop round trip, through the public API only: a seed
//! restored from its recovery phrase issues keys that a verifier holding just
//! the public key classifies correctly.

use medflow_license::calendar::{unix_ms_from_utc, MS_PER_DAY};
use medflow_license::phrase::{phrase_to_seed, seed_to_phrase};
use medflow_license::{
    generate_seed, issue_key, public_key_from_seed, verify, KeyRequest, KeyStatus, MachineCode,
    Term, TrustStore, TrustedKey,
};

const KID: &str = "e2e-2026-09";

fn request(machine: &MachineCode, issued_at: i64, term: Term, features: &[&str]) -> KeyRequest {
    KeyRequest {
        licensee: "Cabinet Dr Test".into(),
        machine_code: machine.clone(),
        major: 2,
        features: features.iter().map(|f| f.to_string()).collect(),
        issued_at,
        term,
        license_id: None,
    }
}

#[test]
fn phrase_restored_seed_issues_keys_the_desktop_classifies() {
    // Key Maker first setup, then "lost phone": restore from the phrase.
    let original = generate_seed().unwrap();
    let restored = phrase_to_seed(&seed_to_phrase(&original)).unwrap();
    assert_eq!(restored, original);

    // Desktop side: only the public key is embedded.
    let keys = [TrustedKey {
        kid: KID,
        public_key: public_key_from_seed(&original),
    }];
    let trust = TrustStore {
        keys: &keys,
        revoked_kids: &[],
        revoked_license_ids: &[],
    };

    let this_pc = MachineCode::derive("this-pc-machine-guid");
    // The doctor reads the code over the phone with a typo-proof checksum.
    let typed = this_pc.as_str().to_lowercase().replace('-', " ");
    let machine = MachineCode::parse(&typed).unwrap();
    assert_eq!(machine, this_pc);

    let now = unix_ms_from_utc(2026, 9, 25, 12, 0, 0).unwrap();

    let one_month = issue_key(&restored, KID, &request(&machine, now, Term::Months(1), &[]))
        .unwrap()
        .0;
    let lifetime = issue_key(
        &restored,
        KID,
        &request(&machine, now, Term::Lifetime, &["telegram"]),
    )
    .unwrap()
    .0;
    let issued_long_ago = unix_ms_from_utc(2026, 6, 1, 9, 0, 0).unwrap();
    let expired = issue_key(
        &restored,
        KID,
        &request(&machine, issued_long_ago, Term::Months(1), &[]),
    )
    .unwrap()
    .0;
    let other_pc = MachineCode::derive("another-pc");
    let wrong_machine = issue_key(&restored, KID, &request(&other_pc, now, Term::Years(1), &[]))
        .unwrap()
        .0;

    let check = |key: &str, at: i64| verify(&trust, key, &this_pc, 2, at);

    match check(&one_month, now) {
        KeyStatus::Valid(l) => {
            assert_eq!(l.term(), Term::Months(1));
            assert_eq!(l.expires_at, Some(unix_ms_from_utc(2026, 10, 25, 12, 0, 0).unwrap()));
        }
        other => panic!("1-month key: {other:?}"),
    }
    assert_eq!(check(&one_month, now + 20 * MS_PER_DAY).code(), "expiring_soon");
    assert_eq!(check(&one_month, now + 31 * MS_PER_DAY).code(), "expired");

    match check(&lifetime, now + 50 * 365 * MS_PER_DAY) {
        KeyStatus::Valid(l) => {
            assert!(l.is_lifetime());
            assert!(l.has_feature("telegram"));
        }
        other => panic!("lifetime key: {other:?}"),
    }

    assert_eq!(check(&expired, now).code(), "expired");
    assert_eq!(check(&wrong_machine, now).code(), "wrong_machine");

    // Same keys on a 3.x build → wrong major (paid upgrade).
    assert_eq!(verify(&trust, &lifetime, &this_pc, 3, now).code(), "wrong_major");

    // After a compromise the kid is removed: nothing it signed verifies.
    let after_rotation = TrustStore {
        keys: &keys,
        revoked_kids: &[KID],
        revoked_license_ids: &[],
    };
    assert_eq!(
        verify(&after_rotation, &lifetime, &this_pc, 2, now).code(),
        "unknown_kid"
    );
}
