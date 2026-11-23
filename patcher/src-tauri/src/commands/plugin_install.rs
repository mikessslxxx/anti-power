//! Install the upstream Usage Monitor release into the Antigravity plugin directory

use super::antigravity_store::checked_path;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const PLUGINS: &str = ".gemini/config/plugins";
const NAME: &str = "usage-monitor";
const RELEASE: &str = "https://github.com/yuhuan-dev/antigravity-usage-monitor/releases/latest/download/usage-monitor.zip";
const DOWNLOAD_LIMIT: usize = 16 * 1024 * 1024;
const EXTRACT_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginStatus {
    path: String,
    installed_version: Option<String>,
    macos_launcher: bool,
    foreign: bool,
    saved_folder: Option<String>,
}

fn version(dir: &Path) -> Result<String> {
    let path = checked_path(dir, Path::new("plugin.json"))?.ok_or("Missing plugin manifest")?;
    let manifest: Value = serde_json::from_slice(&fs::read(path)?)?;
    if manifest["name"] != NAME {
        return Err("The folder belongs to a different plugin".into());
    }
    Ok(semver::Version::parse(
        manifest["version"]
            .as_str()
            .ok_or("Missing plugin version")?,
    )?
    .to_string())
}

pub(super) fn installed(home: &Path) -> Result<Option<String>> {
    checked_path(home, &Path::new(PLUGINS).join(NAME))?
        .map(|dir| version(&dir))
        .transpose()
}

fn status(home: &Path) -> PluginStatus {
    let current = installed(home);
    PluginStatus {
        path: home.join(PLUGINS).join(NAME).display().to_string(),
        macos_launcher: cfg!(target_os = "macos"),
        foreign: current.is_err(),
        saved_folder: None,
        installed_version: current.ok().flatten(),
    }
}

fn plugins_dir(home: &Path) -> Result<PathBuf> {
    let mut relative = PathBuf::new();
    for name in Path::new(PLUGINS) {
        relative.push(name);
        if checked_path(home, &relative)?.is_none() {
            match fs::create_dir(home.join(&relative)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        }
        if !checked_path(home, &relative)?.is_some_and(|p| p.is_dir()) {
            return Err(format!("Invalid plugin directory: {}", relative.display()).into());
        }
    }
    Ok(home.join(relative))
}

fn change_lock(plugins: &Path) -> Result<fs::File> {
    let config = plugins.parent().ok_or("Missing config folder")?;
    let name = ".anti-power-usage-monitor.lock";
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

fn unpack(bytes: &[u8], stage: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut remaining = EXTRACT_LIMIT;
    for i in 0..archive.len() {
        let member = archive.by_index(i)?;
        let raw = member.name().trim_end_matches('/');
        let mut parts = raw.split('/');
        if parts.next() != Some(NAME)
            || parts.any(|s| {
                s.is_empty()
                    || s == "."
                    || s == ".."
                    || s.ends_with(['.', ' '])
                    || s.contains(['\\', ':'])
            })
        {
            return Err("Invalid release archive path".into());
        }
        let path = stage.join(member.enclosed_name().ok_or("Invalid release path")?);
        if member.is_symlink() {
            return Err("Unexpected symlink in release".into());
        }
        if member.is_dir() {
            fs::create_dir_all(path)?;
            continue;
        }
        let size = member.size();
        if size > remaining {
            return Err("Release exceeds unpacked size limit".into());
        }
        fs::create_dir_all(path.parent().ok_or("Invalid release path")?)?;
        let mut file = fs::File::create_new(path)?;
        if io::copy(&mut member.take(size + 1), &mut file)? != size {
            return Err("Incomplete release member".into());
        }
        remaining -= size;
    }
    let plugin = stage.join(NAME);
    version(&plugin)?;
    for required in [
        "mcp_config.json",
        "sidecars/panel/main.mjs",
        "sidecars/panel/sidecar.json",
        "sidecars/panel/app.js",
    ] {
        if !plugin.join(required).is_file() {
            return Err(format!("Missing release file: {required}").into());
        }
    }
    Ok(())
}

fn install(home: &Path, bytes: &[u8]) -> Result<PluginStatus> {
    let plugins = plugins_dir(home)?;
    let _lock = change_lock(&plugins)?;
    if installed(home)?.is_none() {
        let stage = tempfile::Builder::new()
            .prefix(".usage-monitor-install-")
            .tempdir_in(&plugins)?;
        unpack(bytes, stage.path())?;
        super::plugin_links::apply(&stage.path().join(NAME).join("sidecars/panel"))?;
        fs::rename(stage.path().join(NAME), plugins.join(NAME))?;
    }
    Ok(status(home))
}

fn uninstall(home: &Path) -> Result<PluginStatus> {
    let plugins = plugins_dir(home)?;
    let _lock = change_lock(&plugins)?;
    if installed(home)?.is_some() {
        let dir = plugins.join(NAME);
        if checked_path(&dir, Path::new(".git"))?.is_some()
            || checked_path(&dir, Path::new("sidecars/panel/.data"))?.is_some()
        {
            let backup = tempfile::Builder::new()
                .prefix("usage-monitor-backup-")
                .tempdir_in(plugins.parent().ok_or("Missing config folder")?)?;
            let saved = backup.path().join(NAME);
            fs::rename(dir, &saved)?;
            let _ = backup.keep();
            let mut result = status(home);
            result.saved_folder = Some(saved.display().to_string());
            return Ok(result);
        }
        let retired = tempfile::Builder::new()
            .prefix(".usage-monitor-uninstall-")
            .tempdir_in(&plugins)?;
        fs::rename(dir, retired.path().join(NAME))?;
        retired.close()?;
    }
    Ok(status(home))
}

async fn download() -> std::result::Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(90))
        .user_agent("Anti-Power")
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(RELEASE)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if chunk.len() > DOWNLOAD_LIMIT - bytes.len() {
            return Err("Release download exceeds size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn home_dir() -> std::result::Result<PathBuf, String> {
    std::env::home_dir().ok_or_else(|| "Cannot find the home directory".into())
}

#[tauri::command(async)]
pub fn plugin_status() -> std::result::Result<PluginStatus, String> {
    Ok(status(&home_dir()?))
}

#[tauri::command]
pub async fn install_plugin() -> std::result::Result<PluginStatus, String> {
    let home = home_dir()?;
    if installed(&home).map_err(|e| e.to_string())?.is_some() {
        return Ok(status(&home));
    }
    let bytes = download().await?;
    tauri::async_runtime::spawn_blocking(move || install(&home, &bytes).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command(async)]
pub fn uninstall_plugin() -> std::result::Result<PluginStatus, String> {
    uninstall(&home_dir()?).map_err(|e| e.to_string())
}

/// Launch Services only passes Chromium flags to a cold application
#[tauri::command(async)]
pub fn launch_antigravity() -> std::result::Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        if installed(&home_dir()?)
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Err("Install Usage Monitor first".into());
        }
        let running = Command::new("/usr/bin/pgrep")
            .args(["-x", "Antigravity"])
            .output()
            .map_err(|e| e.to_string())?;
        match running.status.code() {
            Some(0) => return Ok(false),
            Some(1) => {}
            _ => return Err("Could not check whether Antigravity is running".into()),
        }
        let launch = Command::new("/usr/bin/open")
            .args([
                "-b",
                "com.google.antigravity",
                "--args",
                "--remote-debugging-address=127.0.0.1",
                "--remote-debugging-port=0",
            ])
            .output()
            .map_err(|e| e.to_string())?;
        if !launch.status.success() {
            return Err(String::from_utf8_lossy(&launch.stderr).trim().to_owned());
        }
        Ok(true)
    }
    #[cfg(not(target_os = "macos"))]
    Err("This launcher requires macOS".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn archive(extra: &str) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in [
            (
                "usage-monitor/plugin.json",
                r#"{"name":"usage-monitor","version":"1.5.3"}"#,
            ),
            ("usage-monitor/mcp_config.json", "{}"),
            (
                "usage-monitor/sidecars/panel/main.mjs",
                "const out = await forwardApiCall(pathname, method, data);",
            ),
            (
                "usage-monitor/sidecars/panel/shared/window-bridge-protocol.mjs",
                "const BRIDGE_API_ROUTE_DEFS = Object.freeze([]);",
            ),
            (
                "usage-monitor/sidecars/panel/backend/api-actions.mjs",
                "switch (action) { case 'telemetry': break; }",
            ),
            (
                "usage-monitor/sidecars/panel/backend/plugin-updater.mjs",
                "await verifyPluginJsSyntax(extractedRoot);",
            ),
            ("usage-monitor/sidecars/panel/sidecar.json", "{}"),
            (
                "usage-monitor/sidecars/panel/app.js",
                "function setup() { return { el: shellEl, }; }",
            ),
            (extra, "extra"),
        ] {
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(data.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn only_complete_releases_are_published_and_uninstall_preserves_data() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let history = home.join(".gemini/antigravity/sidecar_data/usage-monitor/panel/data");
        fs::create_dir_all(&history).unwrap();
        fs::write(history.join("trajectory_usage_cache.json"), "history").unwrap();
        assert!(install(home, &archive("usage-monitor/../escape")).is_err());
        assert!(!home.join(PLUGINS).join(NAME).exists());
        let release = archive("usage-monitor/LICENSE");
        assert_eq!(
            install(home, &release)
                .unwrap()
                .installed_version
                .as_deref(),
            Some("1.5.3")
        );
        let dir = home.join(PLUGINS).join(NAME);
        assert!(dir.join("sidecars/panel/anti-power/adapter.mjs").is_file());
        assert!(fs::read_to_string(dir.join("sidecars/panel/app.js"))
            .unwrap()
            .contains("antipower://session/"));
        fs::create_dir(dir.join(".git")).unwrap();
        let saved = uninstall(home).unwrap().saved_folder.unwrap();
        assert!(Path::new(&saved).join(".git").is_dir());
        install(home, &release).unwrap();
        assert!(uninstall(home).unwrap().installed_version.is_none());
        assert!(history.join("trajectory_usage_cache.json").is_file());
        fs::create_dir(&dir).unwrap();
        assert!(status(home).foreign);
        assert!(install(home, &release).is_err());
        assert!(uninstall(home).is_err());
    }

    #[test]
    #[ignore = "downloads the current GitHub release"]
    fn official_release_installs_and_uninstalls_in_a_temporary_home() {
        let temp = tempfile::tempdir().unwrap();
        let bytes = tauri::async_runtime::block_on(download()).unwrap();
        assert!(install(temp.path(), &bytes)
            .unwrap()
            .installed_version
            .is_some());
        // The built-in updater and desktop installer must produce identical plugin files
        let staging = tempfile::tempdir().unwrap();
        unpack(&bytes, staging.path()).unwrap();
        let installed = temp.path().join(PLUGINS).join(NAME).join("sidecars/panel");
        let root = staging.path().join(NAME);
        let check = std::process::Command::new("node")
            .args(["--input-type=module", "-e", "import {pathToFileURL} from 'node:url'; await (await import(pathToFileURL(process.argv[1]))).prepareUpdate(process.argv[2]);"])
            .arg(installed.join("anti-power/adapter.mjs"))
            .arg(&root)
            .status().unwrap();
        assert!(check.success());
        for entry in zip::ZipArchive::new(Cursor::new(&bytes))
            .unwrap()
            .file_names()
        {
            let Some(file) = entry.strip_prefix("usage-monitor/sidecars/panel/") else {
                continue;
            };
            if !file.is_empty() && !file.ends_with('/') {
                assert_eq!(
                    fs::read(installed.join(file)).unwrap(),
                    fs::read(root.join("sidecars/panel").join(file)).unwrap(),
                    "{file}"
                );
            }
        }
        assert!(uninstall(temp.path()).unwrap().installed_version.is_none());
    }
}
