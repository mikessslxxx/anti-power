//! Install the bundled Anti-Power Usage plugin into Antigravity

use super::antigravity_clean::antigravity_running;
use super::antigravity_store::checked_path;
use super::usage_ledger::PLUGIN;
use serde::Serialize;
use serde_json::{Map, Value};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const PLUGINS: &str = ".gemini/config/plugins";
const MANIFEST: &str = "plugin.json";
const UPGRADE: &str = ".anti-power-usage-upgrade";
const UPGRADE_MARKER: &[u8] = b"anti-power-usage-v1";
const BUNDLED_CATALOG: &str = "pricing-catalog.json";
// Kept across upgrades; the plugin applies whichever catalog was updated last
const DOWNLOADED_CATALOG: &str = "pricing-catalog.download.json";
const CATALOG_FIELDS: [&str; 5] = [
    "schemaVersion",
    "catalogVersion",
    "updatedAt",
    "pricing",
    "placeholders",
];
const MAX_CATALOG_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginStatus {
    pub path: String,
    pub bundled_version: Option<String>,
    pub installed_version: Option<String>,
    /// Another plugin occupies the folder, so Anti-Power leaves it alone
    pub foreign: bool,
    pub bundled_catalog: Option<String>,
    pub installed_catalog: Option<String>,
    pub downloaded_catalog: Option<String>,
    /// Changes take effect after Antigravity restarts
    pub antigravity_running: bool,
}

fn target(home: &Path) -> PathBuf {
    home.join(PLUGINS).join(PLUGIN)
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn text(value: &Option<Value>, name: &str) -> Option<String> {
    value.as_ref()?.get(name)?.as_str().map(str::to_owned)
}

fn owned_manifest(dir: &Path) -> io::Result<Option<Value>> {
    Ok(checked_path(dir, Path::new(MANIFEST))?
        .and_then(|path| read_json(&path))
        .filter(|manifest| manifest["name"].as_str() == Some(PLUGIN)))
}

/// The installed folder: None when absent, Err when it is not ours or not a real folder
fn installed(home: &Path) -> Result<Option<Value>> {
    let Some(dir) = checked_path(home, &Path::new(PLUGINS).join(PLUGIN))? else {
        return Ok(None);
    };
    if !fs::symlink_metadata(&dir)?.is_dir() {
        return Err(format!("{} is not a plugin folder", dir.display()).into());
    }
    let Some(manifest) = owned_manifest(&dir)? else {
        return Err(format!("Another plugin is installed at {}", dir.display()).into());
    };
    Ok(Some(manifest))
}

fn status(home: &Path, bundled: &Path) -> PluginStatus {
    let dir = target(home);
    let (manifest, foreign) = match installed(home) {
        Ok(manifest) => (manifest, false),
        Err(_) => (None, true),
    };
    let catalog = |path: PathBuf| {
        fs::symlink_metadata(&path).ok().filter(|m| m.is_file())?;
        text(&read_json(&path), "updatedAt").filter(|s| catalog_time(s).is_some())
    };
    PluginStatus {
        path: dir.to_string_lossy().into_owned(),
        bundled_version: text(&read_json(&bundled.join(MANIFEST)), "version"),
        installed_version: text(&manifest, "version"),
        foreign,
        bundled_catalog: catalog(bundled.join(BUNDLED_CATALOG)),
        installed_catalog: manifest.as_ref().and_then(|_| {
            let legacy = Path::new(".gemini/antigravity/sidecar_data")
                .join(PLUGIN)
                .join("panel/data")
                .join(BUNDLED_CATALOG);
            [
                catalog(dir.join(BUNDLED_CATALOG)),
                checked_path(home, &legacy)
                    .ok()
                    .flatten()
                    .and_then(&catalog),
            ]
            .into_iter()
            .flatten()
            .max_by_key(|date| catalog_time(date))
        }),
        downloaded_catalog: manifest.and_then(|_| catalog(dir.join(DOWNLOADED_CATALOG))),
        antigravity_running: antigravity_running().unwrap_or(false),
    }
}

// Plain files and folders only; symlinks in the bundle are refused
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let dest = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &dest)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &dest)?;
        } else {
            return Err(format!(
                "Unexpected entry in the plugin bundle: {}",
                entry.path().display()
            )
            .into());
        }
    }
    Ok(())
}

fn plugins_dir(home: &Path) -> Result<PathBuf> {
    let mut relative = PathBuf::new();
    for name in Path::new(PLUGINS) {
        relative.push(name);
        if checked_path(home, &relative)?.is_none() {
            match fs::create_dir(home.join(&relative)) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        if !checked_path(home, &relative)?.is_some_and(|path| path.is_dir()) {
            return Err(format!("Invalid plugin directory: {}", relative.display()).into());
        }
    }
    Ok(home.join(relative))
}

// Published upgrade state holds `new` (and `previous` when upgrading); recover only ours
fn recover_upgrade(home: &Path) -> Result<()> {
    let Some(txn) = checked_path(home, &Path::new(PLUGINS).join(UPGRADE))? else {
        return Ok(());
    };
    if checked_path(&txn, Path::new("transaction"))?
        .and_then(|path| fs::read(path).ok())
        .as_deref()
        != Some(UPGRADE_MARKER)
    {
        return Err(format!("Unknown plugin upgrade directory: {}", txn.display()).into());
    }
    let dir = target(home);
    let previous = checked_path(&txn, Path::new("previous"))?;
    let fresh = checked_path(&txn, Path::new("new"))?;
    if installed(home)?.is_none() {
        let source = previous.or(fresh).ok_or("Incomplete plugin upgrade")?;
        if owned_manifest(&source)?.is_none() {
            return Err("Invalid recovery plugin".into());
        }
        fs::rename(source, dir)?;
    } else if previous.is_some() && fresh.is_some() {
        return Err(format!(
            "Plugin destination is occupied. Previous installation: {}",
            txn.display()
        )
        .into());
    }
    // Retire the journal atomically before removing its contents
    let retired = tempfile::Builder::new()
        .prefix(".anti-power-usage-done-")
        .tempdir_in(txn.parent().unwrap())?;
    fs::rename(txn, retired.path().join("transaction"))?;
    Ok(())
}

/// Install or upgrade using a complete staged copy
fn install(home: &Path, bundled: &Path) -> Result<()> {
    if text(&read_json(&bundled.join(MANIFEST)), "name").as_deref() != Some(PLUGIN) {
        return Err("The bundled plugin is missing".into());
    }
    recover_upgrade(home)?;
    let upgrading = installed(home)?.is_some();
    let plugins = plugins_dir(home)?;
    let dir = target(home);
    let stage = tempfile::Builder::new()
        .prefix(".anti-power-usage-new-")
        .tempdir_in(&plugins)?;
    let fresh = stage.path().join("new");
    copy_tree(bundled, &fresh)?;
    if !upgrading {
        fs::rename(&fresh, &dir)?;
        return Ok(());
    }
    if let Some(catalog) = checked_path(&dir, Path::new(DOWNLOADED_CATALOG))? {
        fs::copy(catalog, fresh.join(DOWNLOADED_CATALOG))?;
    }
    fs::write(stage.path().join("transaction"), UPGRADE_MARKER)?;
    let txn = plugins.join(UPGRADE);
    fs::rename(stage.path(), &txn)?;
    fs::rename(&dir, txn.join("previous"))?;
    if let Err(error) = fs::rename(txn.join("new"), &dir) {
        if let Err(restore) = recover_upgrade(home) {
            return Err(format!(
                "Plugin install failed: {error}. Recovery: {restore}. Backup: {}",
                txn.display()
            )
            .into());
        }
        return Err(error.into());
    }
    recover_upgrade(home)
}

/// Remove the plugin folder; its usage history in Antigravity's sidecar data stays
fn uninstall(home: &Path) -> Result<()> {
    recover_upgrade(home)?;
    if installed(home)?.is_none() {
        return Ok(());
    }
    let plugins = plugins_dir(home)?;
    let old = tempfile::Builder::new()
        .prefix(".anti-power-usage-old-")
        .tempdir_in(&plugins)?;
    fs::rename(target(home), old.path().join(PLUGIN))?;
    Ok(())
}

fn catalog_time(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.len() == 10 && text.as_bytes().get(4) == Some(&b'-') {
        return super::antigravity_scan::parse_time(&format!("{text}T00:00:00Z"));
    }
    text.contains('T')
        .then(|| super::antigravity_scan::parse_time(text))
        .flatten()
}

/// Keep the catalog fields the plugin reads, after checking every price entry
fn sanitized_catalog(raw: &str) -> Result<Vec<u8>> {
    if raw.len() > MAX_CATALOG_BYTES {
        return Err("The price catalog is too large".into());
    }
    let mut value: Map<String, Value> =
        serde_json::from_str(raw).map_err(|_| "The price catalog is not valid JSON")?;
    value
        .get("updatedAt")
        .and_then(Value::as_str)
        .and_then(catalog_time)
        .ok_or("Invalid price catalog update time")?;
    let pricing = value
        .get("pricing")
        .and_then(Value::as_object)
        .filter(|map| !map.is_empty())
        .ok_or("The price catalog has no prices")?;
    if !value.get("placeholders").is_some_and(Value::is_object) {
        return Err("The price catalog has no prices".into());
    }
    if !pricing.iter().all(|(model, entry)| {
        let [Some(input), Some(cache), Some(output)] =
            ["input", "cache", "output"].map(|key| entry[key].as_f64())
        else {
            return false;
        };
        !model.trim().is_empty()
            && (input > 0.0 || output > 0.0)
            && [input, cache, output]
                .iter()
                .all(|rate| (0.0..=10000.0).contains(rate))
            && ["short", "family"]
                .iter()
                .all(|&key| entry[key].as_str().is_some_and(|s| !s.trim().is_empty()))
    }) {
        return Err("The price catalog has an invalid price".into());
    }
    let kept: Map<String, Value> = CATALOG_FIELDS
        .into_iter()
        .filter_map(|name| value.remove(name).map(|entry| (name.to_owned(), entry)))
        .collect();
    let mut bytes = serde_json::to_vec_pretty(&kept)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Save a downloaded catalog next to the installed plugin, which reloads it on change
fn update_catalog(home: &Path, raw: &str) -> Result<()> {
    recover_upgrade(home)?;
    if installed(home)?.is_none() {
        return Err("Install the plugin first".into());
    }
    let bytes = sanitized_catalog(raw)?;
    let dir = target(home);
    let mut tmp = tempfile::NamedTempFile::new_in(&dir)?;
    tmp.write_all(&bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(dir.join(DOWNLOADED_CATALOG))?;
    Ok(())
}

fn home_dir() -> std::result::Result<PathBuf, String> {
    std::env::home_dir().ok_or_else(|| "Cannot find the home directory".to_owned())
}

fn bundled_dir(app: &AppHandle) -> std::result::Result<PathBuf, String> {
    app.path()
        .resolve("plugin", BaseDirectory::Resource)
        .map_err(|error| error.to_string())
}

// All commands, including status recovery, use the same cross-process lock
fn change_lock(home: &Path) -> Result<fs::File> {
    let plugins = plugins_dir(home)?;
    let config = plugins.parent().ok_or("Invalid plugin directory")?;
    let name = ".anti-power-usage.lock";
    checked_path(config, Path::new(name))?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(config.join(name))?;
    lock.lock()?;
    Ok(lock)
}

#[tauri::command(async)]
pub fn plugin_status(app: AppHandle) -> std::result::Result<PluginStatus, String> {
    let home = home_dir()?;
    let _lock = if checked_path(&home, &Path::new(PLUGINS).join(UPGRADE))
        .map_err(|e| e.to_string())?
        .is_some()
    {
        let lock = change_lock(&home).map_err(|e| e.to_string())?;
        recover_upgrade(&home).map_err(|e| e.to_string())?;
        Some(lock)
    } else {
        None
    };
    Ok(status(&home, &bundled_dir(&app)?))
}

fn change_plugin(
    app: &AppHandle,
    action: impl FnOnce(&Path, &Path) -> Result<()>,
) -> std::result::Result<PluginStatus, String> {
    let (home, bundled) = (home_dir()?, bundled_dir(app)?);
    let _change = change_lock(&home).map_err(|error| error.to_string())?;
    action(&home, &bundled).map_err(|error| error.to_string())?;
    Ok(status(&home, &bundled))
}

#[tauri::command(async)]
pub fn install_plugin(app: AppHandle) -> std::result::Result<PluginStatus, String> {
    change_plugin(&app, install)
}

#[tauri::command(async)]
pub fn uninstall_plugin(app: AppHandle) -> std::result::Result<PluginStatus, String> {
    change_plugin(&app, |home, _| uninstall(home))
}

#[tauri::command(async)]
pub fn update_plugin_catalog(
    app: AppHandle,
    catalog: String,
) -> std::result::Result<PluginStatus, String> {
    change_plugin(&app, |home, _| update_catalog(home, &catalog))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bundle(root: &Path, version: &str) -> PathBuf {
        let dir = root.join(format!("bundle-{version}"));
        fs::create_dir_all(dir.join("sidecars/panel/web")).unwrap();
        fs::write(
            dir.join(MANIFEST),
            format!(r#"{{"name":"{PLUGIN}","version":"{version}"}}"#),
        )
        .unwrap();
        fs::write(dir.join("sidecars/panel/web/app.js"), version).unwrap();
        fs::write(
            dir.join(BUNDLED_CATALOG),
            r#"{"updatedAt":"2026-11-19T00:00:00Z"}"#,
        )
        .unwrap();
        dir
    }

    #[test]
    fn installs_the_repository_plugin() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin");
        install(home, &bundled).unwrap();
        let status = status(home, &bundled);
        assert_eq!(
            status.installed_version.as_deref(),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert!(status.bundled_catalog.is_some());
        for file in [
            "sidecars/panel/main.mjs",
            "sidecars/panel/web/index.html",
            "sidecars/panel/web/app.js",
        ] {
            assert!(target(home).join(file).is_file(), "{file}");
        }
    }

    const CATALOG: &str = r#"{"updatedAt":"2026-11-21T00:00:00Z","releaseUrl":"https://example.invalid",
        "pricing":{"m":{"input":1,"cache":0.1,"output":2,"short":"m","family":"Gemini"}},"placeholders":{}}"#;

    #[test]
    fn installs_upgrades_and_keeps_a_downloaded_catalog() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        let dir = target(home);
        assert!(update_catalog(home, CATALOG).is_err());

        install(home, &bundle(home, "1.0.0")).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("sidecars/panel/web/app.js")).unwrap(),
            "1.0.0"
        );
        update_catalog(home, CATALOG).unwrap();
        let saved = fs::read_to_string(dir.join(DOWNLOADED_CATALOG)).unwrap();
        assert!(saved.contains("2026-11-21") && !saved.contains("releaseUrl"));

        let update = bundle(home, "2.0.0");
        install(home, &update).unwrap();
        let status = status(home, &update);
        assert_eq!(status.installed_version.as_deref(), Some("2.0.0"));
        assert_eq!(
            status.downloaded_catalog.as_deref(),
            Some("2026-11-21T00:00:00Z")
        );
        assert!(!status.foreign);
        assert_eq!(
            fs::read_to_string(dir.join("sidecars/panel/web/app.js")).unwrap(),
            "2.0.0"
        );
        // Only the plugin folder is left; staging folders are gone
        let names: Vec<_> = fs::read_dir(home.join(PLUGINS))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [PLUGIN]);

        uninstall(home).unwrap();
        assert!(!dir.exists());
        uninstall(home).unwrap();
    }

    #[test]
    fn interrupted_upgrade_restores_the_previous_plugin_and_catalog() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        install(home, &bundle(home, "1")).unwrap();
        update_catalog(home, CATALOG).unwrap();
        let txn = home.join(PLUGINS).join(UPGRADE);
        fs::create_dir(&txn).unwrap();
        fs::write(txn.join("transaction"), UPGRADE_MARKER).unwrap();
        copy_tree(&bundle(home, "2"), &txn.join("new")).unwrap();
        fs::rename(target(home), txn.join("previous")).unwrap();
        recover_upgrade(home).unwrap();
        assert_eq!(
            text(&installed(home).unwrap(), "version").as_deref(),
            Some("1")
        );
        assert!(target(home).join(DOWNLOADED_CATALOG).exists());
        assert!(!txn.exists());
        recover_upgrade(home).unwrap();
    }

    #[test]
    fn leaves_a_foreign_folder_alone() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        let dir = target(home);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(MANIFEST), r#"{"name":"someone-else"}"#).unwrap();
        let bundled = bundle(home, "1.0.0");
        assert!(install(home, &bundled).is_err());
        assert!(uninstall(home).is_err());
        assert!(status(home, &bundled).foreign);
        assert_eq!(
            fs::read_to_string(dir.join(MANIFEST)).unwrap(),
            r#"{"name":"someone-else"}"#
        );
    }

    #[test]
    fn rejects_catalogs_without_valid_prices() {
        let catalog: Value = serde_json::from_str(CATALOG).unwrap();
        for (field, invalid) in [
            ("/updatedAt", json!("2026-11-32T00:00:00Z")),
            ("/pricing", json!({})),
            ("/pricing/m/family", json!("")),
            ("/pricing/m/input", json!(-1)),
        ] {
            let mut invalid_catalog = catalog.clone();
            *invalid_catalog.pointer_mut(field).unwrap() = invalid;
            let result = sanitized_catalog(&invalid_catalog.to_string());
            assert!(result.is_err(), "{field}");
        }
        assert!(sanitized_catalog(include_str!("../../../../plugin/pricing-catalog.json")).is_ok());
    }
}
