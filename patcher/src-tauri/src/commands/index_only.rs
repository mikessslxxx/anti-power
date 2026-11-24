//! Select and validate index-only cleanup targets

use super::antigravity_clean::{session_files, store_read_lock, store_wide};
use super::antigravity_scan::{require_idle_store, store_artifact_ids, store_index_ids};
use super::antigravity_store::{checked_path, strict_roots, valid_id, CLI, DESKTOP};
use super::sessions::SessionRef;
use super::usage_monitor;
use std::collections::{BTreeSet, HashSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub(super) struct Targets {
    pub matching: Vec<SessionRef>,
    pub historical: Vec<SessionRef>,
}

/// Find index records with no artifacts or migration archive in the known local stores
pub(super) fn preview(home: &Path, excluded: &[String]) -> Result<Vec<SessionRef>> {
    let sources = orphan_sources(home)?;
    let excluded: HashSet<_> = excluded.iter().map(|id| id.to_ascii_lowercase()).collect();
    let mut artifacts = HashSet::new();
    let mut candidates = Vec::new();
    for (root, provider) in &sources {
        let Some(root) = existing_store(home, root)? else {
            continue;
        };
        require_idle_store(&root)?;
        artifacts.extend(store_artifact_ids(&root)?);
        for id in store_index_ids(&root, provider)?
            .into_iter()
            .filter(|id| valid_id(id))
        {
            candidates.push(SessionRef {
                provider_id: (*provider).to_owned(),
                source_path: root.join("conversations").join(format!("{id}.db")),
                session_id: id,
                index_only: true,
            });
        }
    }
    candidates.retain(|session| {
        let id = session.session_id.to_ascii_lowercase();
        !excluded.contains(&id) && !artifacts.contains(&id)
    });
    let ids = candidates
        .iter()
        .map(|session| session.session_id.to_ascii_lowercase())
        .collect();
    let archives = archived_ids(&sources, &ids)?;
    candidates.retain(|session| !archives.contains(&session.session_id.to_ascii_lowercase()));
    candidates.sort_by(|a, b| a.source_path.cmp(&b.source_path));
    Ok(candidates)
}

pub(super) fn targets_after_deletion(
    sessions: &[SessionRef],
    errors: &[Option<String>],
    matching_indexes: &[SessionRef],
    orphan_indexes: &[SessionRef],
) -> Targets {
    let deleted_ids: BTreeSet<_> = sessions
        .iter()
        .zip(errors)
        .filter(|(session, error)| !session.index_only && error.is_none())
        .map(|(session, _)| session.session_id.to_ascii_lowercase())
        .collect();
    let mut selected_paths: BTreeSet<_> = sessions
        .iter()
        .map(|session| &session.source_path)
        .collect();
    let matching = matching_indexes
        .iter()
        .filter(|session| {
            deleted_ids.contains(&session.session_id.to_ascii_lowercase())
                && selected_paths.insert(&session.source_path)
        })
        .cloned()
        .collect();
    let selected_ids: HashSet<_> = sessions
        .iter()
        .map(|session| session.session_id.to_ascii_lowercase())
        .collect();
    let historical = orphan_indexes
        .iter()
        .filter(|session| {
            !selected_ids.contains(&session.session_id.to_ascii_lowercase())
                && selected_paths.insert(&session.source_path)
        })
        .cloned()
        .collect();
    Targets {
        matching,
        historical,
    }
}

pub(super) fn require_no_session_data(root: &Path, ids: &BTreeSet<String>) -> Result<()> {
    let presence = root.join("presence");
    for path in session_files(root, ids)? {
        if path.parent() != Some(presence.as_path()) {
            require_empty_path(&path)?;
        }
    }
    Ok(())
}

pub(super) fn require_empty_path(path: &Path) -> Result<()> {
    let mut paths = vec![path.to_owned()];
    while let Some(path) = paths.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            for entry in fs::read_dir(&path)? {
                paths.push(entry?.path());
            }
        } else if !metadata.is_file() || metadata.len() != 0 {
            return Err(format!("Session data is present at {}", path.display()).into());
        }
    }
    Ok(())
}

pub(super) fn lock_historical_sources(
    home: &Path,
    current: &Path,
    related: &BTreeSet<String>,
) -> Result<Vec<File>> {
    let sources = orphan_sources(home).map_err(store_wide)?;
    let ids: HashSet<_> = related.iter().map(|id| id.to_ascii_lowercase()).collect();
    let mut locks = Vec::new();
    for (root, _) in &sources {
        let Some(root) = existing_store(home, root).map_err(store_wide)? else {
            continue;
        };
        if root.canonicalize().map_err(store_wide)? != current {
            locks.push(store_read_lock(&root).map_err(store_wide)?);
            require_idle_store(&root).map_err(store_wide)?;
        }
        let artifacts = store_artifact_ids(&root).map_err(store_wide)?;
        if let Some(id) = ids.intersection(&artifacts).next() {
            return Err(format!("Session data for {id} is present in {}", root.display()).into());
        }
    }
    if let Some(id) = archived_ids(&sources, &ids)
        .map_err(store_wide)?
        .into_iter()
        .next()
    {
        return Err(format!("Usage Monitor archived session {id}").into());
    }
    Ok(locks)
}

fn orphan_sources(home: &Path) -> Result<Vec<(PathBuf, &'static str)>> {
    let mut sources = Vec::new();
    for provider in [DESKTOP, CLI] {
        sources.extend(
            strict_roots(home, provider)?
                .into_iter()
                .map(|root| (root, provider)),
        );
    }
    Ok(sources)
}

fn existing_store(home: &Path, root: &Path) -> Result<Option<PathBuf>> {
    let path = checked_path(home, root.strip_prefix(home)?)?;
    if path.as_ref().is_some_and(|path| !path.is_dir()) {
        return Err(format!("Expected session store directory at {}", root.display()).into());
    }
    Ok(path)
}

fn archived_ids(sources: &[(PathBuf, &str)], ids: &HashSet<String>) -> Result<HashSet<String>> {
    let mut archives = HashSet::new();
    for (root, _) in sources {
        archives.extend(
            usage_monitor::migration_archives(root, ids)?
                .into_iter()
                .map(|(id, _)| id),
        );
    }
    Ok(archives)
}
