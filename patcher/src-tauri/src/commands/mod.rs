//! Tauri commands

mod antigravity_clean;
mod antigravity_scan;
mod antigravity_store;
mod cc_switch;
mod cleanup_extras;
mod client_versions;
mod index_only;
mod magpie_probes;
mod plugin_install;
mod plugin_links;
mod sessions;
mod usage_ledger;
mod usage_monitor;

pub use client_versions::client_versions;
pub use plugin_install::{install_plugin, launch_antigravity, plugin_status, uninstall_plugin};
pub use sessions::{
    delete_magpie_probes, delete_sessions, load_session_messages, preview_magpie_probes,
    preview_orphan_indexes, scan_sessions,
};
