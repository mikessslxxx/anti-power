//! Read installed client versions for the session source selector
use serde::Serialize;
use serde_json::Value;
use std::env;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

#[derive(Serialize)]
pub struct ClientVersions {
    antigravity: Option<String>,
    agy: Option<String>,
}

fn version(text: &str) -> Option<String> {
    let text = text.trim().trim_start_matches('v');
    semver::Version::parse(text).ok().map(|v| v.to_string())
}

// Read just the root package.json from Electron's packed resources
fn desktop_version(resources: &Path) -> Option<String> {
    let mut archive = File::open(resources.join("app.asar")).ok()?;
    let mut word = || {
        let mut bytes = [0; 4];
        archive.read_exact(&mut bytes).ok()?;
        Some(u32::from_le_bytes(bytes) as u64)
    };
    if word()? != 4 {
        return None;
    }
    let header_size = word()?;
    let payload_size = word()?;
    let json_size = word()?;
    if header_size != payload_size + 4
        || json_size + 4 > payload_size
        || header_size > 8 * 1024 * 1024
    {
        return None;
    }
    let mut header = vec![0; json_size as usize];
    archive.read_exact(&mut header).ok()?;
    let header: Value = serde_json::from_slice(&header).ok()?;
    let entry = &header["files"]["package.json"];
    let offset: u64 = entry["offset"].as_str()?.parse().ok()?;
    let size = entry["size"].as_u64()?;
    if size > 1024 * 1024 {
        return None;
    }
    archive
        .seek(SeekFrom::Start((8 + header_size).checked_add(offset)?))
        .ok()?;
    let package: Value = serde_json::from_reader(archive.take(size)).ok()?;
    if package["name"] != "antigravity" || package["productName"] != "Antigravity" {
        return None;
    }
    version(package["version"].as_str()?)
}

fn resources() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let mut dirs = Vec::new();
        if let Some(home) = env::home_dir() {
            dirs.push(home.join("Applications/Antigravity.app/Contents/Resources"));
        }
        dirs.push("/Applications/Antigravity.app/Contents/Resources".into());
        dirs
    }
    #[cfg(windows)]
    {
        let mut dirs = Vec::new();
        if let Some(local) = env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            dirs.push(local.join("Programs/Antigravity/resources"));
            dirs.push(local.join("Antigravity/resources"));
        }
        for root in ["ProgramFiles", "ProgramFiles(x86)"]
            .into_iter()
            .filter_map(env::var_os)
        {
            dirs.push(PathBuf::from(root).join("Antigravity/resources"));
        }
        dirs
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let mut dirs: Vec<_> = path_dirs()
            .into_iter()
            .filter_map(|dir| {
                let exe = dir.join("antigravity").canonicalize().ok()?;
                Some(exe.parent()?.join("resources"))
            })
            .collect();
        dirs.extend(
            [
                "/opt/Antigravity/resources",
                "/opt/antigravity/resources",
                "/usr/share/antigravity/resources",
            ]
            .map(PathBuf::from),
        );
        dirs
    }
}

fn path_dirs() -> Vec<PathBuf> {
    let path = env::var_os("PATH").unwrap_or_default();
    env::split_paths(&path)
        .filter(|p| p.is_absolute())
        .collect()
}

async fn cli_version() -> Option<String> {
    let mut dirs = path_dirs();
    // Finder and the Dock do not inherit the user's interactive shell PATH
    if let Some(home) = env::home_dir() {
        dirs.push(home.join(".local/bin"));
    }
    #[cfg(target_os = "macos")]
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    let name = if cfg!(windows) { "agy.exe" } else { "agy" };
    let exe = dirs
        .iter()
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())?;
    let mut command = tokio::process::Command::new(exe);
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let output = tokio::time::timeout(
        Duration::from_secs(3),
        command
            .arg("--version")
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = std::str::from_utf8(&output.stdout).ok()?.trim();
    text.split_whitespace().find_map(version)
}

#[tauri::command]
pub async fn client_versions() -> ClientVersions {
    ClientVersions {
        antigravity: resources().iter().find_map(|dir| desktop_version(dir)),
        agy: cli_version().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn reads_the_desktop_manifest_at_its_archive_offset() {
        let resources = tempfile::tempdir().unwrap();
        let package = br#"{"name":"antigravity","productName":"Antigravity","version":"2.34.0"}"#;
        let header =
            serde_json::json!({"files":{"package.json":{"size":package.len(),"offset":"7"}}})
                .to_string();
        let payload_size = (4 + header.len()).next_multiple_of(4);
        let mut archive = Vec::new();
        for word in [4, payload_size + 4, payload_size, header.len()] {
            archive.extend((word as u32).to_le_bytes());
        }
        archive.extend(header.as_bytes());
        archive.resize(12 + payload_size + 7, 0);
        archive.extend(package);
        let path = resources.path().join("app.asar");
        fs::write(&path, &archive).unwrap();
        assert_eq!(desktop_version(resources.path()).as_deref(), Some("2.34.0"));
        fs::write(&path, &archive[..archive.len() - 3]).unwrap();
        assert!(desktop_version(resources.path()).is_none());
    }
}
