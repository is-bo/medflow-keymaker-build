//! What lives inside the encrypted vault (and, identically, inside a backup).

use crate::history::IssuedKey;
use crate::recovery::RecoveryCodes;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

pub const VAULT_SCHEMA: u32 = 1;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpConfig {
    pub secret_b32: String,
    pub enrolled_at: i64,
    /// Last accepted step — a code is never accepted twice.
    pub last_used_step: Option<u64>,
}

impl Drop for TotpConfig {
    fn drop(&mut self) {
        self.secret_b32.zeroize();
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultData {
    pub schema: u32,
    /// The 32-byte Ed25519 signing seed, hex.
    pub seed_hex: String,
    pub kid: String,
    pub created_at: i64,
    pub totp: TotpConfig,
    pub recovery: RecoveryCodes,
    pub history: Vec<IssuedKey>,
    pub last_backup_at: Option<i64>,
    /// History length when the last backup was made. History is append-only,
    /// so the difference is exactly the keys the backup does not hold — and,
    /// unlike a timestamp comparison, a phone clock change cannot skew it.
    #[serde(default)]
    pub keys_at_last_backup: Option<usize>,
    /// False until the wizard's last screen (public key) is acknowledged, so a
    /// restart mid-wizard resumes it after unlock.
    pub setup_finished: bool,
    /// When this vault was written by the wizard (setup or restore). The
    /// wizard only finishes once a backup newer than this exists.
    #[serde(default)]
    pub setup_committed_at: i64,
}

impl Drop for VaultData {
    fn drop(&mut self) {
        self.seed_hex.zeroize();
    }
}

impl VaultData {
    pub fn seed(&self) -> Option<zeroize::Zeroizing<[u8; 32]>> {
        let mut seed = zeroize::Zeroizing::new([0u8; 32]);
        hex::decode_to_slice(&self.seed_hex, seed.as_mut()).ok()?;
        Some(seed)
    }

    /// Keys issued after the last backup (all of them if never backed up).
    pub fn keys_since_backup(&self) -> usize {
        self.history
            .len()
            .saturating_sub(self.keys_at_last_backup.unwrap_or(0))
    }
}
