// Desktop entry point (development only: `pnpm tauri dev` on a PC). The
// Android app enters through `run()` via `tauri::mobile_entry_point`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    medflow_keymaker_lib::run()
}
