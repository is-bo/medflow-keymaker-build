//! This computer's machine identity (desktop + dev CLI only).
//!
//! The OS machine id — Windows `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid`,
//! Linux `/etc/machine-id`, macOS `IOPlatformUUID`. It survives reboots and app
//! reinstalls, and changes on an OS reinstall — which is exactly when a doctor
//! should ask for a fresh key. It is read live on every launch and never
//! persisted by MedFlow, so copying an app-data folder to another PC does not
//! carry the machine code with it.

use crate::machine::MachineCode;

/// The raw OS machine id, trimmed. `Err` when the OS will not provide one.
pub fn host_machine_identity() -> Result<String, String> {
    let id = machine_uid::get().map_err(|e| e.to_string())?;
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err("the OS returned an empty machine id".into());
    }
    Ok(id)
}

/// This computer's machine code, or `Err` when the OS id is unavailable.
pub fn host_machine_code() -> Result<MachineCode, String> {
    host_machine_identity().map(|id| MachineCode::derive(&id))
}
