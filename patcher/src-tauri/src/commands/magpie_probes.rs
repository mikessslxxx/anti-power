//! Locate Magpie's agy store and validate empty model probes

use super::antigravity_clean::{session_files, HISTORY};
use super::antigravity_scan::{has_conversation_data, open_db, require_idle_store};
use super::antigravity_store::{checked_file, checked_path, valid_id, INDEX_DB};
use super::index_only::require_empty_path;
use super::sessions::MagpieProbePreview;
use rusqlite::Connection;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub(super) fn root(home: &Path) -> Result<Option<PathBuf>> {
    resolve_root(
        home,
        std::env::var_os("AGY_BRIDGE_GEMINI_DIR").as_deref(),
        std::env::var_os("XDG_STATE_HOME").as_deref(),
    )
}

fn resolve_root(
    home: &Path,
    bridge_dir: Option<&OsStr>,
    state_home: Option<&OsStr>,
) -> Result<Option<PathBuf>> {
    let (base, relative) = if let Some(dir) = bridge_dir.filter(|dir| !dir.is_empty()) {
        if matches!(
            dir.to_ascii_lowercase().to_str(),
            Some("default" | "0" | "false" | "off" | "no" | "disabled")
        ) {
            return Ok(None);
        }
        let dir = Path::new(dir);
        if !dir.is_absolute() {
            return Err("AGY_BRIDGE_GEMINI_DIR must be an absolute path".into());
        }
        (dir, "antigravity-cli")
    } else if let Some(state) = state_home
        .map(Path::new)
        .filter(|state| state.is_absolute())
    {
        (state, "magpie-agy-bridge/agy-home/antigravity-cli")
    } else {
        (
            home,
            ".local/state/magpie-agy-bridge/agy-home/antigravity-cli",
        )
    };
    let Some(store) = checked_path(base, Path::new(relative))? else {
        return Ok(None);
    };
    if !store.is_dir() {
        return Err("Expected a Magpie session store directory".into());
    }
    let store = store.canonicalize()?;
    // Skip stores shared with normal CLI or CC Switch
    let normal_cli = home.join(".gemini/antigravity-cli");
    let cc_switch = home.join(".cc-switch/agy");
    if normal_cli.canonicalize().is_ok_and(|path| path == store)
        || cc_switch
            .canonicalize()
            .is_ok_and(|path| store.starts_with(path))
    {
        return Ok(None);
    }
    Ok(Some(store))
}

pub(super) fn preview(store: PathBuf) -> Result<MagpieProbePreview> {
    require_idle_store(&store)?;
    let mut ids = Vec::new();
    if let Some(index) = checked_file(&store, INDEX_DB)? {
        let candidates = probe_ids(&open_db(&index)?)?;
        let history = history(&store)?;
        ids.extend(candidates.into_iter().filter(|id| {
            !has_history(&history, id) && require_empty_artifacts(&store, id).is_ok()
        }));
    }
    Ok(MagpieProbePreview { store, ids })
}

pub(super) fn require_probes(root: &Path, db: &Connection, ids: &BTreeSet<String>) -> Result<()> {
    let candidates = probe_ids(db)?;
    let history = history(root)?;
    for id in ids {
        if !candidates.contains(id) || has_history(&history, id) {
            return Err(format!("Session {id} is no longer an empty Magpie model probe").into());
        }
        require_empty_artifacts(root, id)?;
    }
    Ok(())
}

fn probe_ids(db: &Connection) -> Result<BTreeSet<String>> {
    let mut query = db.prepare(
        "SELECT probe.conversation_id, probe.workspace_uris
         FROM conversation_summaries AS probe
         WHERE probe.step_count = 0 AND probe.last_user_input_step_index = -1
           AND probe.title = '' AND probe.preview = ''
           AND probe.nesting_depth = 0 AND probe.parent_conversation_id = ''
           AND probe.workspace_uris LIKE '%magpie-agy-models-%'
           AND NOT EXISTS (
               SELECT 1 FROM conversation_summaries AS child
               WHERE child.parent_conversation_id = probe.conversation_id
           )",
    )?;
    let mut rows = query.query([])?;
    let mut ids = BTreeSet::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        if valid_id(&id) && probe_workspace(&row.get::<_, String>(1)?) {
            ids.insert(id);
        }
    }
    Ok(ids)
}

fn probe_workspace(value: &str) -> bool {
    let Ok(uris) = serde_json::from_str::<Vec<String>>(value) else {
        return false;
    };
    let mut name = None;
    !uris.is_empty()
        && uris.iter().all(|uri| {
            let Some(path) = tauri::Url::parse(uri)
                .ok()
                .and_then(|url| url.to_file_path().ok())
            else {
                return false;
            };
            let Some(suffix) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix("magpie-agy-models-"))
            else {
                return false;
            };
            // macOS can record both /var and /private/var for the same workspace
            suffix.len() == 6
                && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
                && name.get_or_insert_with(|| suffix.to_owned()).as_str() == suffix
        })
}

fn history(root: &Path) -> Result<Vec<u8>> {
    Ok(checked_file(root, HISTORY)?
        .map(fs::read)
        .transpose()?
        .unwrap_or_default())
}

fn has_history(history: &[u8], id: &str) -> bool {
    history
        .windows(id.len())
        .any(|part| part.eq_ignore_ascii_case(id.as_bytes()))
}

fn require_empty_artifacts(root: &Path, id: &str) -> Result<()> {
    let relative = format!("conversations/{id}.db");
    let nonempty_db = checked_file(root, &relative)?
        .filter(|path| fs::symlink_metadata(path).is_ok_and(|meta| meta.len() > 0));
    for path in session_files(root, &BTreeSet::from([id.to_owned()]))? {
        if nonempty_db.as_ref() == Some(&path) {
            let db = open_db(&path)?;
            // A missing steps table cannot prove that an unknown database is empty
            db.prepare("SELECT 1 FROM steps LIMIT 0")?;
            if has_conversation_data(&db)? {
                return Err(format!("Session data is present at {}", path.display()).into());
            }
        } else if !(path.starts_with(root.join("presence"))
            || nonempty_db.is_some()
                && matches!(
                    path.extension().and_then(|ext| ext.to_str()),
                    Some("db-wal" | "db-shm")
                ))
        {
            require_empty_path(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_magpie_store_precedes_state_home_and_the_default() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        let custom = home.join("custom");
        let state = home.join("state");
        let default = home.join(".local/state/magpie-agy-bridge/agy-home/antigravity-cli");
        let xdg_store = state.join("magpie-agy-bridge/agy-home/antigravity-cli");
        let custom_store = custom.join("antigravity-cli");
        for store in [&default, &xdg_store, &custom_store] {
            fs::create_dir_all(store).unwrap();
        }
        let state = Some(state.as_os_str());
        assert_eq!(
            resolve_root(home, Some(custom.as_os_str()), state).unwrap(),
            Some(custom_store.canonicalize().unwrap()),
        );
        assert_eq!(
            resolve_root(home, Some(OsStr::new("")), state).unwrap(),
            Some(xdg_store.canonicalize().unwrap()),
        );
        assert_eq!(
            resolve_root(home, None, None).unwrap(),
            Some(default.canonicalize().unwrap()),
        );
        assert_eq!(
            resolve_root(home, Some(OsStr::new("OFF")), state).unwrap(),
            None
        );
        assert_eq!(
            resolve_root(home, Some(home.join("missing").as_os_str()), state).unwrap(),
            None
        );
    }

    #[test]
    fn skips_explicit_paths_that_share_the_normal_cli_store() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        for dir in [home.join(".gemini"), home.join(".cc-switch/agy/profile")] {
            fs::create_dir_all(dir.join("antigravity-cli")).unwrap();
            assert_eq!(
                resolve_root(home, Some(dir.as_os_str()), None).unwrap(),
                None
            );
        }
        assert!(resolve_root(home, Some(OsStr::new("relative")), None).is_err());
    }
}
