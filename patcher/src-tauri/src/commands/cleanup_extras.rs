//! Clean Antigravity App window state, browser caches and Anti-Power Usage totals

use super::antigravity_clean::{store_read_lock, with_store_lock};
use super::antigravity_scan::live_ids;
use super::antigravity_store::{checked_path, strict_roots, APP_DATA, CLI, DESKTOP};
use super::usage_ledger;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const APP_STORAGE: &str = "app_storage.json";
const APP_CACHES: [&str; 2] = ["Cache", "Code Cache"];
const LAYOUT_PREFIX: &str = "antigravity-multi-conversation-layout-v3-";
const LAYOUT_INDEX: &str = "antigravity-multi-conversation-layout-v3-index";
const PANES: [&str; 2] = ["aux-pane-session", "aux-pane-v2-session"];

/// Sessions removed from one store, including nested agent runs
pub(super) struct Cleaned {
    pub root: PathBuf,
    pub ids: BTreeSet<String>,
}

pub(super) fn clean_local(
    home: &Path,
    cleaned: &Cleaned,
    deleted_databases: &BTreeSet<PathBuf>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if cleaned.ids.is_empty() {
        return warnings;
    }
    // The App also shows Antigravity CLI conversations, so its window state covers both stores
    if let Err(error) = clean_app_storage(home, cleaned) {
        warnings.push(format!("Antigravity App window state: {error}"));
    }
    if let Err(error) = usage_ledger::fold(home, cleaned, deleted_databases) {
        warnings.push(format!("Anti-Power Usage totals: {error}"));
    }
    warnings
}

pub(super) fn clean_caches(home: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    for name in APP_CACHES {
        if let Err(error) = remove_app_cache(home, name) {
            warnings.push(format!("Antigravity App {name}: {error}"));
        }
    }
    warnings
}

fn clean_app_storage(home: &Path, cleaned: &Cleaned) -> Result<()> {
    let Some(path) = checked_path(home, &Path::new(APP_DATA).join(APP_STORAGE))? else {
        return Ok(());
    };
    // Different source stores share this JSON file
    with_store_lock(path.parent().ok_or("Missing App data directory")?, |_| {
        let bytes = fs::read(&path)?;
        let Some(filtered) = filtered_app_storage(&bytes, &cleaned.ids)? else {
            return Ok(());
        };
        let mut gone = cleaned.ids.clone();
        let mut locks = Vec::new();
        for provider in [DESKTOP, CLI] {
            for root in strict_roots(home, provider)? {
                if cleaned.root == root || checked_path(home, root.strip_prefix(home)?)?.is_none() {
                    continue;
                }
                locks.push(store_read_lock(&root)?);
                let live = live_ids(home, &root, provider, None)?;
                gone.retain(|id| !live.contains(&id.to_ascii_lowercase()));
                if gone.is_empty() {
                    return Ok(());
                }
            }
        }
        let filtered = if gone == cleaned.ids {
            Some(filtered)
        } else {
            filtered_app_storage(&bytes, &gone)?
        };
        if let Some(filtered) = filtered {
            replace_file(&path, &filtered)?;
        }
        Ok(())
    })
}

fn filtered_app_storage(bytes: &[u8], ids: &BTreeSet<String>) -> Result<Option<Vec<u8>>> {
    let mut state: Map<String, Value> = serde_json::from_slice(bytes)?;
    let mut changed = false;
    state.retain(|key, _| {
        let keep = !key
            .strip_prefix(LAYOUT_PREFIX)
            .is_some_and(|id| ids.contains(id));
        changed |= !keep;
        keep
    });
    // These values are JSON documents stored as strings
    for key in PANES.into_iter().chain([LAYOUT_INDEX]) {
        let Some(Value::String(text)) = state.get_mut(key) else {
            continue;
        };
        let Ok(mut value) = serde_json::from_str::<Value>(text) else {
            continue;
        };
        let count = |value: &Value| match value {
            Value::Object(pane) => pane
                .get("conversationPanes")
                .and_then(Value::as_object)
                .map_or(0, Map::len),
            Value::Array(list) => list.len(),
            _ => 0,
        };
        let before = count(&value);
        match &mut value {
            Value::Object(pane) => {
                if let Some(panes) = pane
                    .get_mut("conversationPanes")
                    .and_then(Value::as_object_mut)
                {
                    panes.retain(|id, _| !ids.contains(id));
                }
            }
            Value::Array(list) => {
                list.retain(|item| !item.as_str().is_some_and(|id| ids.contains(id)));
            }
            _ => {}
        }
        if count(&value) != before {
            *text = serde_json::to_string(&value)?;
            changed = true;
        }
    }
    if !changed {
        return Ok(None);
    }
    let mut result = serde_json::to_vec_pretty(&state)?;
    if bytes.ends_with(b"\n") {
        result.push(b'\n');
    }
    Ok(Some(result))
}

pub(super) fn replace_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().ok_or("Invalid file path")?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.as_file()
        .set_permissions(fs::metadata(path)?.permissions())?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)?;
    Ok(())
}

// Chromium rebuilds these caches, which cannot be filtered by conversation
fn remove_app_cache(home: &Path, name: &str) -> Result<()> {
    let Some(path) = checked_path(home, &Path::new(APP_DATA).join(name))? else {
        return Ok(());
    };
    if !path.is_dir() {
        return Err(format!("Expected a folder at {}", path.display()).into());
    }
    fs::remove_dir_all(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: &str = "00000000-0000-4000-8000-000000000001";
    const FORK: &str = "00000000-0000-4000-8000-000000000002";

    fn ids(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|id| (*id).to_owned()).collect()
    }

    fn app_storage() -> String {
        let panes = |version: bool| {
            let mut panes = serde_json::json!({"conversationPanes": {
                TARGET: {"tabs": []},
                FORK: {"tabs": []},
            }});
            if version {
                panes["version"] = 1.into();
            }
            panes.to_string()
        };
        let layout = |id: &str| format!(r#"{{"rootNode":{{"cascadeId":"{id}"}}}}"#);
        let state = serde_json::json!({
            "sidebarWidth": "320",
            "aux-pane-session": panes(false),
            "aux-pane-v2-session": panes(true),
            LAYOUT_INDEX: serde_json::json!([TARGET, FORK]).to_string(),
            format!("{LAYOUT_PREFIX}{TARGET}"): layout(TARGET),
            format!("{LAYOUT_PREFIX}{FORK}"): layout(FORK),
        });
        serde_json::to_string_pretty(&state).unwrap() + "\n"
    }

    #[test]
    fn shared_window_layout_survives_an_index_only_copy() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app_data = home.join(APP_DATA);
        let cli = home.join(".gemini/antigravity-cli/cache");
        fs::create_dir_all(&app_data).unwrap();
        fs::create_dir_all(&cli).unwrap();
        fs::write(app_data.join(APP_STORAGE), app_storage()).unwrap();
        let metadata = cli.join("conversation_metadata.json");
        fs::write(
            &metadata,
            serde_json::json!({"conversations":{TARGET:{}}}).to_string(),
        )
        .unwrap();
        let selected = Cleaned {
            root: home.join(".gemini/antigravity"),
            ids: ids(&[TARGET]),
        };
        assert!(clean_local(home, &selected, &BTreeSet::new()).is_empty());
        assert!(fs::read_to_string(app_data.join(APP_STORAGE))
            .unwrap()
            .contains(TARGET));
        fs::remove_file(metadata).unwrap();
        assert!(clean_local(home, &selected, &BTreeSet::new()).is_empty());
        assert!(!fs::read_to_string(app_data.join(APP_STORAGE))
            .unwrap()
            .contains(TARGET));
    }

    #[test]
    fn removes_window_state_for_removed_sessions_only() {
        let original = app_storage();
        let filtered = filtered_app_storage(original.as_bytes(), &ids(&[TARGET]))
            .unwrap()
            .unwrap();
        let text = String::from_utf8(filtered).unwrap();
        assert!(!text.contains(TARGET) && text.contains(FORK) && text.ends_with("}\n"));
        let state: Map<String, Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(state["sidebarWidth"], "320");
        assert!(state.contains_key(&format!("{LAYOUT_PREFIX}{FORK}")));
        let v2: Value =
            serde_json::from_str(state["aux-pane-v2-session"].as_str().unwrap()).unwrap();
        assert_eq!(v2["version"], 1);
        assert!(v2["conversationPanes"].get(FORK).is_some());

        let other = ids(&["00000000-0000-4000-8000-000000000009"]);
        assert!(filtered_app_storage(original.as_bytes(), &other)
            .unwrap()
            .is_none());
        assert!(filtered_app_storage(b"[]", &other).is_err());
    }

    #[test]
    fn removes_only_the_app_cache_folders() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        let app = home.join(APP_DATA);
        for name in APP_CACHES {
            fs::create_dir_all(app.join(name).join("js")).unwrap();
            fs::write(app.join(name).join("js").join("entry"), TARGET).unwrap();
        }
        fs::write(app.join("Preferences"), "{}").unwrap();
        assert!(clean_caches(home).is_empty());
        assert!(APP_CACHES.iter().all(|name| !app.join(name).exists()));
        assert!(app.join("Preferences").is_file());
        // Missing folders are not an error
        assert!(clean_caches(home).is_empty());
    }
}
