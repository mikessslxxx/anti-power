//! Anti-Power Rust backend
//!
//! Browse and clean local Antigravity App and Antigravity CLI sessions, and install the usage plugin

mod commands;

use commands::{
    client_versions, delete_sessions, install_plugin, launch_antigravity, load_session_messages,
    plugin_status, preview_orphan_indexes, scan_sessions, uninstall_plugin,
};
use tauri::Manager;
use tauri_plugin_deep_link::DeepLinkExt;

fn focus_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            focus_main(app);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |_| focus_main(&handle));
            #[cfg(target_os = "linux")]
            {
                if let Err(err) = app.deep_link().register_all() {
                    eprintln!("Could not register antipower://: {err}");
                }
            }
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
            preview_orphan_indexes,
            delete_sessions,
            plugin_status,
            install_plugin,
            launch_antigravity,
            uninstall_plugin
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
