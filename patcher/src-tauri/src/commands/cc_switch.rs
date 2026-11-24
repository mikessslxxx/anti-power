//! Clean CC Switch sync records for removed Antigravity sessions

use super::antigravity_clean::{has_column, store_read_lock};
use super::antigravity_scan::live_ids;
use super::antigravity_store::{checked_path, strict_roots, valid_id, CLI, DESKTOP};
use super::cleanup_extras::Cleaned;
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const CC_SWITCH_DB: &str = ".cc-switch/cc-switch.db";
const CC_SWITCH_BACKUPS: &str = ".cc-switch/backups";

pub(super) struct SyncResult {
    pub targets_done: bool,
    pub warnings: Vec<String>,
}

pub(super) fn clean(home: &Path, cleaned: &Cleaned, cc_switch_running: bool) -> SyncResult {
    let mut warnings = Vec::new();
    let mut stores = Vec::new();
    let mut locks = Vec::new();
    for provider in [DESKTOP, CLI] {
        let roots = match strict_roots(home, provider) {
            Ok(roots) => roots,
            Err(error) => {
                warnings.push(format!(
                    "CC Switch: could not enumerate {provider}: {error}"
                ));
                continue;
            }
        };
        for root in roots {
            let mut scan = || -> Result<_> {
                let relative = root.strip_prefix(home)?;
                let Some(path) = checked_path(home, relative)? else {
                    return Ok(None);
                };
                if path != cleaned.root {
                    locks.push(store_read_lock(&path)?);
                }
                Ok(Some((
                    sync_prefix(&path),
                    relative.to_owned(),
                    live_ids(home, &path, provider, None)?,
                )))
            };
            match scan() {
                Ok(Some(store)) => stores.push(store),
                Ok(None) => {}
                Err(error) => warnings.push(format!(
                    "CC Switch: kept records for {}: {error}",
                    root.display()
                )),
            }
        }
    }

    let selected = sync_prefix(&cleaned.root);
    let is_stale = |file_path: &str| {
        let file_path = file_path.replace('\\', "/");
        if sync_id(&file_path, &selected).is_some_and(|id| cleaned.ids.contains(id)) {
            return true;
        }
        stores.iter().any(|(prefix, root, live)| {
            sync_id(&file_path, prefix).is_some_and(|id| {
                !live.contains(&id.to_ascii_lowercase())
                    && checked_path(home, &root.join("brain").join(id))
                        .is_ok_and(|path| path.is_none())
            })
        })
    };
    let mut targets_done = true;
    let paths = cc_switch_databases(home).unwrap_or_else(|error| {
        targets_done = false;
        warnings.push(format!("CC Switch: {error}"));
        Vec::new()
    });
    let live_db = home.join(CC_SWITCH_DB);
    for path in paths {
        let vacuum = !cc_switch_running || path != live_db;
        if let Err(error) = clean_cc_switch(&path, &is_stale, vacuum) {
            targets_done = false;
            warnings.push(format!("CC Switch {}: {error}", path.display()));
        }
    }
    SyncResult {
        targets_done,
        warnings,
    }
}

pub(super) fn is_process_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(name.trim_end_matches(".exe"), "cc-switch" | "cc switch")
}

fn cc_switch_databases(home: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if let Some(path) = checked_path(home, Path::new(CC_SWITCH_DB))? {
        if path.is_file() {
            paths.push(path);
        }
    }
    if let Some(dir) = checked_path(home, Path::new(CC_SWITCH_BACKUPS))? {
        if dir.is_dir() {
            let mut backups = Vec::new();
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let database = entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.ends_with(".db"));
                if database && entry.file_type()?.is_file() {
                    backups.push(entry.path());
                }
            }
            backups.sort();
            paths.extend(backups);
        }
    }
    Ok(paths)
}

fn sync_prefix(root: &Path) -> String {
    format!("agy:{}/brain/", root.to_string_lossy().replace('\\', "/"))
}

fn sync_id<'a>(file_path: &'a str, prefix: &str) -> Option<&'a str> {
    let (id, _) = file_path.strip_prefix(prefix)?.split_once('/')?;
    valid_id(id).then_some(id)
}

// Only remove sync progress; usage and cost history stays
fn clean_cc_switch(path: &Path, is_stale: &impl Fn(&str) -> bool, vacuum: bool) -> Result<()> {
    // The path has no symlinks below home, and SQLite refuses any above it
    let mut conn = Connection::open_with_flags(
        path.canonicalize()?,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.pragma_update(None, "secure_delete", true)?;
    if !has_column(&conn, "session_log_sync", "file_path")? {
        return Ok(());
    }
    // Read and delete under one write lock, so a concurrent sync cannot slip in between
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut removed = 0;
    {
        let stale: Vec<String> = {
            let mut statement =
                tx.prepare("SELECT file_path FROM session_log_sync WHERE file_path LIKE 'agy:%'")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            let mut stale = Vec::new();
            for row in rows {
                let file_path = row?;
                if is_stale(&file_path) {
                    stale.push(file_path);
                }
            }
            stale
        };
        let mut statement = tx.prepare("DELETE FROM session_log_sync WHERE file_path = ?1")?;
        for file_path in &stale {
            removed += statement.execute([file_path])?;
        }
    }
    tx.commit()?;
    if removed > 0 && vacuum {
        // Removed rows are already overwritten, so compacting is best effort
        let _ = conn.execute_batch("VACUUM");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const TARGET: &str = "00000000-0000-4000-8000-000000000001";
    const FORK: &str = "00000000-0000-4000-8000-000000000002";

    fn ids(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn recognizes_cc_switch() {
        assert!(
            is_process_name("cc-switch")
                && is_process_name("CC Switch")
                && is_process_name("cc-switch.exe")
        );
        assert!(!is_process_name("cc-switch-helper"));
    }

    fn cc_switch_db(path: &Path, root: &Path) {
        let conn = sync_db(
            path,
            &[
                transcript(root, TARGET),
                transcript(root, FORK),
                transcript(Path::new("/other/antigravity"), TARGET),
            ],
        );
        conn.execute_batch(
            "CREATE TABLE proxy_request_logs (request_id TEXT PRIMARY KEY, session_id TEXT, data_source TEXT);
             CREATE TABLE session_usage_dedup (data_source TEXT, request_id TEXT, PRIMARY KEY (data_source, request_id));
             CREATE TABLE usage_daily_rollups (date TEXT, request_count INTEGER);
             INSERT INTO usage_daily_rollups VALUES ('2026-11-20', 3);",
        )
        .unwrap();
        for (source, id) in [
            ("agy_session", TARGET),
            ("agy_session", FORK),
            ("codex_session", TARGET),
        ] {
            for step in [1, 2] {
                let request = format!("agy:{id}:{step}");
                conn.execute(
                    "INSERT INTO proxy_request_logs VALUES (?1, ?2, ?3)",
                    [
                        format!("{source}/{request}"),
                        id.to_owned(),
                        source.to_owned(),
                    ],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO session_usage_dedup VALUES (?1, ?2)",
                    [source.to_owned(), request],
                )
                .unwrap();
            }
        }
    }

    fn rows(path: &Path, sql: &str) -> Vec<String> {
        let conn = Connection::open(path).unwrap();
        let mut statement = conn.prepare(sql).unwrap();
        statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    #[test]
    fn forgets_cc_switch_sync_progress_but_keeps_usage_and_other_files() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        let root = home.join(".gemini/antigravity");
        let backups = home.join(CC_SWITCH_BACKUPS);
        fs::create_dir_all(&backups).unwrap();
        let databases = [home.join(CC_SWITCH_DB), backups.join("db_backup_1.db")];
        for path in &databases {
            cc_switch_db(path, &root);
        }
        fs::write(backups.join("notes.txt"), TARGET).unwrap();
        // Keep brain/FORK on disk so FORK is not swept
        fs::create_dir_all(root.join("brain").join(FORK)).unwrap();
        let cleaned = Cleaned {
            root,
            ids: ids(&[TARGET]),
        };
        assert!(clean(home, &cleaned, false).warnings.is_empty());
        for path in &databases {
            let logs = rows(path, "SELECT file_path FROM session_log_sync ORDER BY 1");
            assert_eq!(logs.len(), 2);
            assert!(logs.iter().any(|path| path.contains(FORK)));
            assert!(logs.iter().any(|path| path.starts_with("agy:/other/")));
            assert_eq!(
                rows(path, "SELECT request_id FROM proxy_request_logs").len(),
                6
            );
            assert_eq!(
                rows(path, "SELECT request_id FROM session_usage_dedup").len(),
                6
            );
            assert_eq!(rows(path, "SELECT date FROM usage_daily_rollups").len(), 1);
        }
        assert_eq!(
            fs::read_to_string(backups.join("notes.txt")).unwrap(),
            TARGET
        );
    }

    fn sync_db(path: &Path, files: &[String]) -> Connection {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch("CREATE TABLE session_log_sync (file_path TEXT PRIMARY KEY)")
            .unwrap();
        for file in files {
            conn.execute("INSERT INTO session_log_sync VALUES (?1)", [file])
                .unwrap();
        }
        conn
    }

    fn transcript(root: &Path, id: &str) -> String {
        format!(
            "agy:{}/brain/{id}/.system_generated/logs/transcript_full.jsonl",
            root.to_string_lossy()
        )
    }

    #[test]
    fn sweeps_records_of_sessions_already_gone_from_their_store() {
        const ORPHAN: &str = "00000000-0000-4000-8000-000000000003";
        const INDEXED: &str = "00000000-0000-4000-8000-000000000004";
        const EMPTY_BRAIN: &str = "00000000-0000-4000-8000-000000000005";
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        let desktop = home.join(".gemini/antigravity");
        let cli = home.join(".gemini/antigravity-cli");
        let profile = home.join(".cc-switch/agy/work/antigravity-cli");
        let missing = home.join(".cc-switch/agy/gone/antigravity-cli");
        for root in [&desktop, &cli, &profile] {
            fs::create_dir_all(root).unwrap();
        }
        // An index row keeps a session alive even without files
        let index = Connection::open(cli.join("conversation_summaries.db")).unwrap();
        index
            .execute_batch(&format!(
                "CREATE TABLE conversation_summaries (conversation_id TEXT, title TEXT);
                 INSERT INTO conversation_summaries VALUES ('{INDEXED}', 'Indexed');"
            ))
            .unwrap();
        drop(index);
        // CC Switch may still read an empty brain folder, so it counts as alive here
        fs::create_dir_all(desktop.join("brain").join(EMPTY_BRAIN)).unwrap();

        // A removed profile is not a known store, so its records stay
        let kept = [
            transcript(&cli, INDEXED),
            transcript(&desktop, EMPTY_BRAIN),
            transcript(Path::new("/other/antigravity"), ORPHAN),
            format!("agy:{}/brain/not-an-id/x.jsonl", desktop.to_string_lossy()),
            "claude:/somewhere/session.jsonl".to_owned(),
            transcript(&missing, ORPHAN),
        ];
        let swept = [
            transcript(&desktop, ORPHAN),
            transcript(&cli, ORPHAN),
            transcript(&profile, ORPHAN),
            transcript(&desktop, TARGET),
        ];
        let db = home.join(CC_SWITCH_DB);
        let all: Vec<String> = kept.iter().chain(&swept).cloned().collect();
        sync_db(&db, &all);

        let cleaned = Cleaned {
            root: desktop,
            ids: ids(&[TARGET]),
        };
        // CC Switch is running: records are still cleaned, only compaction is skipped
        assert!(clean(home, &cleaned, true).warnings.is_empty());
        let left = rows(&db, "SELECT file_path FROM session_log_sync ORDER BY 1");
        let mut expected = kept;
        expected.sort();
        assert_eq!(left, expected);
    }

    #[test]
    fn preserves_unselected_cursors_when_a_secondary_source_is_unreadable() {
        for source in [
            "agyhub_summaries_proto.pb",
            "cache/conversation_metadata.json",
            "conversations",
            "conversation_summaries.db",
            "fts",
            "unknown protobuf field",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path();
            let app = home.join(".gemini/antigravity");
            fs::create_dir_all(app.join("cache")).unwrap();
            if source == "fts" {
                let db = Connection::open(app.join("conversation_summaries.db")).unwrap();
                db.execute_batch("CREATE TABLE conversations_fts (unknown TEXT)")
                    .unwrap();
            } else if source == "unknown protobuf field" {
                let mut raw = vec![26, 36];
                raw.extend_from_slice(FORK.as_bytes());
                fs::write(app.join("agyhub_summaries_proto.pb"), raw).unwrap();
            } else {
                fs::write(app.join(source), "invalid").unwrap();
            }
            fs::create_dir_all(home.join(".cc-switch")).unwrap();
            let db = home.join(CC_SWITCH_DB);
            sync_db(&db, &[transcript(&app, TARGET), transcript(&app, FORK)]);
            let cleaned = Cleaned {
                root: app.clone(),
                ids: ids(&[TARGET]),
            };
            let result = clean(home, &cleaned, false);
            assert!(result.targets_done, "{source}");
            assert_eq!(result.warnings.len(), 1, "{source}");
            assert!(result.warnings[0].contains("kept records"), "{source}");
            assert_eq!(
                rows(&db, "SELECT file_path FROM session_log_sync"),
                vec![transcript(&app, FORK)]
            );
        }
    }
}
