//! Anti-Power Rust backend
//!
//! Browse and clean local Antigravity App and Antigravity CLI sessions, and install the usage plugin

mod commands;

use commands::{
    client_versions, delete_sessions, install_plugin, launch_antigravity, load_session_messages,
    plugin_status, scan_sessions, uninstall_plugin, update_plugin_catalog,
};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "macos")]
                {
                    let _ = window.set_decorations(true);
                    let _ = window.set_title_bar_style(tauri::TitleBarStyle::Overlay);
                    let _ = window.set_title("");
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = window.set_decorations(false);
                }
            }
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            client_versions,
            scan_sessions,
            load_session_messages,
            delete_sessions,
            plugin_status,
            launch_antigravity,
            install_plugin,
            uninstall_plugin,
            update_plugin_catalog
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
