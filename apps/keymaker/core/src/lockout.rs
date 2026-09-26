//! Escalating delays after failed unlock attempts, persisted next to the vault
//! so closing and reopening the app does not reset them.
//!
//! Accounting is pessimistic: an attempt is recorded as a failure (and saved)
//! BEFORE the password is checked, and cleared only on success. Killing the
//! app in the middle of a slow Argon2 check therefore still costs an attempt.
//!
//! Policy: 2 free tries (typos, a code typed at the step boundary), then
//! 3rd failure → 30 s, 4th → 5 min, 5th and every later one → 1 h.

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FREE_FAILURES: u32 = 2;

pub fn delay_ms(failures: u32) -> i64 {
    match failures {
        0..=FREE_FAILURES => 0,
        3 => 30_000,
        4 => 5 * 60_000,
        _ => 60 * 60_000,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lockout {
    pub failures: u32,
    /// Unix ms before which no attempt is accepted.
    pub locked_until_ms: i64,
}

impl Lockout {
    /// A missing file means no failures. A damaged file is treated the same:
    /// anyone able to damage it could just as well delete it (see README,
    /// threat model) — the Argon2 cost is the real brute-force defence.
    pub fn load(path: &Path) -> Lockout {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        crate::storage::write_atomic(path, serde_json::to_string(self).expect("serializes").as_bytes())
    }

    /// Milliseconds until the next attempt is allowed (0 = now).
    pub fn wait_ms(&self, now_ms: i64) -> i64 {
        (self.locked_until_ms - now_ms).max(0)
    }

    /// Count this attempt as a failure up front.
    pub fn begin_attempt(&mut self, now_ms: i64) {
        self.failures = self.failures.saturating_add(1);
        let delay = delay_ms(self.failures);
        self.locked_until_ms = if delay > 0 { now_ms + delay } else { 0 };
    }

    pub fn succeed(&mut self) {
        *self = Lockout::default();
    }
}

pub fn seconds_ceil(ms: i64) -> u64 {
    ((ms.max(0) + 999) / 1000) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalates_30s_5min_1h() {
        let mut l = Lockout::default();
        let now = 1_000_000;
        let mut waits = Vec::new();
        for _ in 0..7 {
            l.begin_attempt(now);
            waits.push(l.wait_ms(now));
        }
        assert_eq!(waits, vec![0, 0, 30_000, 300_000, 3_600_000, 3_600_000, 3_600_000]);
        l.succeed();
        assert_eq!(l.wait_ms(now), 0);
        assert_eq!(l.failures, 0);
    }

    #[test]
    fn survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("km-lockout-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lockout.json");
        let mut l = Lockout::default();
        for _ in 0..4 {
            l.begin_attempt(5_000);
        }
        l.save(&path).unwrap();
        let reloaded = Lockout::load(&path);
        assert_eq!(reloaded, l);
        assert_eq!(reloaded.wait_ms(5_000), 300_000);
        std::fs::remove_dir_all(&dir).ok();
    }
}
