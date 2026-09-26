//! Tauri command layer for the MedFlow Key Maker. Every command forwards to
//! `medflow_keymaker_core::Engine`; no security decision is made here or in
//! the webview. Argon2 takes about a second on a phone, so engine calls run on
//! the blocking pool, never on the UI thread.

use medflow_keymaker_core::engine::{
    self, BackupSummary, ExportedBackup, KeyInfo, MachineCheck, PhraseCheck, RecoverySheet,
    SetupStarted, Status, TotpEnrollment, UnlockResult,
};
use medflow_keymaker_core::history::HistoryItem;
use medflow_keymaker_core::{Engine, Error, IssueForm, SecondFactor, TermInput};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

struct AppState {
    engine: Arc<Mutex<Engine>>,
    /// App-private cache folder for files handed to the share sheet.
    share_dir: PathBuf,
}

type CmdResult<T> = Result<T, Error>;

async fn with_engine<T, F>(state: &AppState, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Engine) -> CmdResult<T> + Send + 'static,
{
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // A panic inside one command must not brick every later command.
        let mut guard = engine.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    })
    .await
    .map_err(|e| Error::Storage(e.to_string()))?
}

/// A file prepared for sharing / saving: `path` feeds the share sheet,
/// `contents` feeds "Save to phone" (the system file picker).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreparedFile {
    path: String,
    file_name: String,
    contents: String,
}

fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '-' })
        .take(96)
        .collect();
    let trimmed = cleaned.trim_start_matches('.');
    if trimmed.is_empty() { "medflow-file".into() } else { trimmed.to_string() }
}

fn prepare(dir: &Path, file_name: &str, contents: String) -> CmdResult<PreparedFile> {
    std::fs::create_dir_all(dir).map_err(|e| Error::Storage(e.to_string()))?;
    let file_name = safe_file_name(file_name);
    let path = dir.join(&file_name);
    std::fs::write(&path, contents.as_bytes()).map_err(|e| Error::Storage(e.to_string()))?;
    Ok(PreparedFile {
        path: path.to_string_lossy().into_owned(),
        file_name,
        contents,
    })
}

/// Files handed to other apps are only needed while the share sheet is open;
/// remove leftovers (share dir + copies the share plugin made) at startup.
fn clear_share_leftovers(share_dir: &Path, cache_dir: &Path) {
    let _ = std::fs::remove_dir_all(share_dir);
    if let Ok(entries) = std::fs::read_dir(cache_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.ends_with(".mflic") || name.ends_with(".csv") || name.ends_with(".mfkbackup") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

// ── Status, setup ────────────────────────────────────────────────────────────

#[tauri::command]
async fn status(state: State<'_, AppState>) -> CmdResult<Status> {
    with_engine(&state, |e| Ok(e.status())).await
}

#[tauri::command]
async fn setup_begin(state: State<'_, AppState>, password: String, kid: String) -> CmdResult<SetupStarted> {
    with_engine(&state, move |e| e.setup_begin(&password, &kid)).await
}

#[tauri::command]
async fn setup_restore_phrase(
    state: State<'_, AppState>,
    phrase: String,
    password: String,
    kid: String,
) -> CmdResult<SetupStarted> {
    with_engine(&state, move |e| e.setup_restore_phrase(&phrase, &password, &kid)).await
}

#[tauri::command]
async fn setup_restore_backup(
    state: State<'_, AppState>,
    contents: String,
    password: String,
) -> CmdResult<BackupSummary> {
    with_engine(&state, move |e| e.setup_restore_backup(&contents, &password)).await
}

#[tauri::command]
async fn setup_check_phrase(state: State<'_, AppState>, answers: Vec<(usize, String)>) -> CmdResult<()> {
    with_engine(&state, move |e| e.setup_check_phrase(&answers)).await
}

#[tauri::command]
async fn setup_new_totp(state: State<'_, AppState>) -> CmdResult<TotpEnrollment> {
    with_engine(&state, |e| e.setup_new_totp()).await
}

#[tauri::command]
async fn setup_confirm_totp(state: State<'_, AppState>, code: String) -> CmdResult<()> {
    with_engine(&state, move |e| e.setup_confirm_totp(&code)).await
}

#[tauri::command]
async fn setup_keep_totp(state: State<'_, AppState>, code: String) -> CmdResult<()> {
    with_engine(&state, move |e| e.setup_keep_totp(&code)).await
}

#[tauri::command]
async fn setup_commit(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    with_engine(&state, |e| e.setup_commit()).await
}

#[tauri::command]
async fn cancel_setup(state: State<'_, AppState>) -> CmdResult<()> {
    with_engine(&state, |e| {
        e.cancel_setup();
        Ok(())
    })
    .await
}

#[tauri::command]
async fn recovery_sheet(state: State<'_, AppState>) -> CmdResult<RecoverySheet> {
    with_engine(&state, |e| e.recovery_sheet()).await
}

#[tauri::command]
async fn finish_setup(state: State<'_, AppState>) -> CmdResult<()> {
    with_engine(&state, |e| e.finish_setup()).await
}

// ── Lock / unlock ────────────────────────────────────────────────────────────

#[tauri::command]
async fn unlock(state: State<'_, AppState>, password: String, factor: SecondFactor) -> CmdResult<UnlockResult> {
    with_engine(&state, move |e| e.unlock(&password, &factor)).await
}

#[tauri::command]
async fn lock(state: State<'_, AppState>) -> CmdResult<()> {
    with_engine(&state, |e| {
        e.lock();
        Ok(())
    })
    .await
}

#[tauri::command]
async fn touch(state: State<'_, AppState>) -> CmdResult<()> {
    with_engine(&state, |e| e.touch()).await
}

#[tauri::command]
async fn erase_vault(state: State<'_, AppState>) -> CmdResult<()> {
    with_engine(&state, |e| e.erase_vault()).await
}

// ── Keys and history ─────────────────────────────────────────────────────────

#[tauri::command]
async fn issue_key(state: State<'_, AppState>, form: IssueForm) -> CmdResult<HistoryItem> {
    with_engine(&state, move |e| e.issue(&form)).await
}

#[tauri::command]
async fn history(state: State<'_, AppState>, query: String) -> CmdResult<Vec<HistoryItem>> {
    with_engine(&state, move |e| e.history(&query)).await
}

#[tauri::command]
async fn history_csv_file(state: State<'_, AppState>) -> CmdResult<PreparedFile> {
    let dir = state.share_dir.clone();
    with_engine(&state, move |e| {
        let csv = e.history_csv()?;
        let name = format!("medflow-keys-{}.csv", medflow_keymaker_core::history::iso_date(now_ms()));
        prepare(&dir, &name, csv)
    })
    .await
}

/// A `.mflic` file for a key already in the history.
#[tauri::command]
async fn key_file(state: State<'_, AppState>, license_id: String) -> CmdResult<PreparedFile> {
    let dir = state.share_dir.clone();
    with_engine(&state, move |e| {
        let item = e
            .history("")?
            .into_iter()
            .find(|i| i.entry.license_id == license_id)
            .ok_or(Error::InvalidInput("license_id"))?;
        let name = format!("medflow-{}-{}.mflic", item.entry.licensee, item.entry.machine_code);
        prepare(&dir, &name, item.entry.key)
    })
    .await
}

#[tauri::command]
fn check_machine_code(input: String) -> MachineCheck {
    engine::check_machine_code(&input)
}

#[tauri::command]
fn check_phrase(input: String) -> PhraseCheck {
    engine::check_phrase(&input)
}

#[tauri::command]
fn preview_expiry(term: TermInput) -> CmdResult<Option<i64>> {
    engine::preview_expiry(&term, now_ms())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ── Backup and settings ──────────────────────────────────────────────────────

#[tauri::command]
async fn export_backup(state: State<'_, AppState>, password: String) -> CmdResult<PreparedFile> {
    let dir = state.share_dir.clone();
    with_engine(&state, move |e| {
        let ExportedBackup { file_name, contents } = e.export_backup(&password)?;
        prepare(&dir, &file_name, contents)
    })
    .await
}

#[tauri::command]
async fn public_key(state: State<'_, AppState>) -> CmdResult<KeyInfo> {
    with_engine(&state, |e| e.public_key()).await
}

#[tauri::command]
async fn reveal_phrase(state: State<'_, AppState>, password: String) -> CmdResult<Vec<String>> {
    with_engine(&state, move |e| e.reveal_phrase(&password)).await
}

#[tauri::command]
async fn regenerate_recovery_codes(state: State<'_, AppState>, password: String) -> CmdResult<Vec<String>> {
    with_engine(&state, move |e| e.regenerate_recovery_codes(&password)).await
}

#[tauri::command]
async fn reenroll_totp_begin(state: State<'_, AppState>, password: String) -> CmdResult<TotpEnrollment> {
    with_engine(&state, move |e| e.reenroll_totp_begin(&password)).await
}

#[tauri::command]
async fn reenroll_totp_confirm(state: State<'_, AppState>, code: String) -> CmdResult<()> {
    with_engine(&state, move |e| e.reenroll_totp_confirm(&code)).await
}

#[tauri::command]
async fn change_password(state: State<'_, AppState>, old_password: String, new_password: String) -> CmdResult<()> {
    with_engine(&state, move |e| e.change_password(&old_password, &new_password)).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_sharekit::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let cache_dir = app.path().app_cache_dir()?;
            let share_dir = cache_dir.join("share");
            std::fs::create_dir_all(&data_dir)?;
            clear_share_leftovers(&share_dir, &cache_dir);
            app.manage(AppState {
                engine: Arc::new(Mutex::new(Engine::new(data_dir))),
                share_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            status,
            setup_begin,
            setup_restore_phrase,
            setup_restore_backup,
            setup_check_phrase,
            setup_new_totp,
            setup_confirm_totp,
            setup_keep_totp,
            setup_commit,
            cancel_setup,
            recovery_sheet,
            finish_setup,
            unlock,
            lock,
            touch,
            erase_vault,
            issue_key,
            history,
            history_csv_file,
            key_file,
            check_machine_code,
            check_phrase,
            preview_expiry,
            export_backup,
            public_key,
            reveal_phrase,
            regenerate_recovery_codes,
            reenroll_totp_begin,
            reenroll_totp_confirm,
            change_password,
        ])
        .run(tauri::generate_context!())
        .expect("error while running MedFlow Key Maker");
}
