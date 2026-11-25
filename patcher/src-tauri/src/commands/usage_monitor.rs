//! Protect migrated session attachments and clean Usage Monitor session data

use super::antigravity_clean::{store_read_lock, with_store_lock};
use super::antigravity_scan::live_ids;
use super::antigravity_store::{checked_file, checked_path, strict_roots, valid_id, CLI, DESKTOP};
use super::cleanup_extras::{replace_file, Cleaned};
use rusqlite::{params, Connection, OpenFlags, TransactionBehavior};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const PLUGIN: &str = ".gemini/config/plugins/usage-monitor";
const CACHE: &str = "trajectory_usage_cache.json";
const BACKUP: &str = "trajectory_usage_cache.json.bak";
const LEDGER: &str = "trajectory_store.sqlite";
const DELETED: &str = "已删除会话 / Deleted session";
const STEP_PROMPTS: [&str; 3] = ["turnPrompt", "turnPromptClean", "userPrompt"];
const PANEL_DATA_DIRS: [&str; 2] = [
    "sidecar_data/usage-monitor/panel/data",
    "sidecar_data/usage-monitor/data",
];

const USAGE_FIELDS: [&str; 14] = [
    "accountName",
    "createdAt",
    "createdAtMs",
    "totalDurationSec",
    "isCompactionStep",
    "modelKey",
    "modelDisplayName",
    "responseModel",
    "apiProvider",
    "inputTokens",
    "cacheReadTokens",
    "outputTokens",
    "thinkingTokens",
    "responseTokens",
];

pub(super) enum CacheCleanup {
    Complete,
    Pending,
}

pub(super) fn clean(
    home: &Path,
    cleaned: &Cleaned,
    writer_running: fn() -> Result<bool>,
) -> Result<CacheCleanup> {
    clean_caches(home, Some(cleaned), writer_running)
}

pub(super) fn sweep(home: &Path, writer_running: fn() -> Result<bool>) -> Result<CacheCleanup> {
    clean_caches(home, None, writer_running)
}

fn clean_caches(
    home: &Path,
    cleaned: Option<&Cleaned>,
    writer_running: fn() -> Result<bool>,
) -> Result<CacheCleanup> {
    let mut roots = strict_roots(home, DESKTOP)?;
    roots.extend(strict_roots(home, CLI)?);
    // IDE sessions can keep a local copy alive and its panel can cache App and CLI sessions
    roots.extend([".gemini/antigravity-ide", ".gemini/antigravity-idea"].map(|p| home.join(p)));
    let mut data_dirs = vec![PathBuf::from(format!("{PLUGIN}/sidecars/panel/.data"))];
    for root in &roots {
        let relative = root.strip_prefix(home)?;
        data_dirs.extend(PANEL_DATA_DIRS.map(|data| relative.join(data)));
    }
    let mut caches = BTreeMap::new();
    let mut errors = BTreeSet::new();
    for dir in data_dirs {
        for name in [CACHE, LEDGER, BACKUP] {
            match checked_path(home, &dir.join(name)) {
                Ok(Some(_)) => caches
                    .entry(home.join(&dir))
                    .or_insert_with(Vec::new)
                    .push(name),
                Ok(None) => {}
                Err(error) => {
                    errors.insert(error.to_string());
                }
            }
        }
    }
    // A writer may still hold records it has never flushed to disk
    if (cleaned.is_some() || !caches.is_empty()) && writer_running()? {
        return Ok(CacheCleanup::Pending);
    }
    if caches.is_empty() {
        return cache_cleanup_result(errors);
    }

    let compatible = caches.values().any(|names| names.contains(&CACHE)) && reads_history(home);
    let mut sources = Sources {
        external_complete: compatible,
        ..Sources::default()
    };
    let mut locks = Vec::new();
    for root in roots {
        if checked_path(home, root.strip_prefix(home)?)?.is_none() {
            continue;
        }
        let is_app = root == home.join(".gemini/antigravity");
        let is_cli = root.ends_with("antigravity-cli");
        if (is_app || is_cli) && cleaned.is_none_or(|group| group.root != root) {
            locks.push(store_read_lock(&root)?);
        }
        sources.live.extend(live_ids(
            home,
            &root,
            if is_app { DESKTOP } else { CLI },
            None,
        )?);
        if sources.external_complete && sources.read_external(&root).is_err() {
            sources.external_complete = false;
        }
        if is_app || is_cli {
            for name in ["brain", "conversations"] {
                if let Some(dir) = checked_path(&root, Path::new(name))? {
                    sources.dirs.insert(dir);
                    sources.app |= is_app;
                    sources.cli |= is_cli;
                }
            }
        }
    }

    let selected: BTreeSet<_> = cleaned
        .into_iter()
        .flat_map(|group| &group.ids)
        .map(|id| id.to_ascii_lowercase())
        .collect();
    for (dir, files) in caches {
        let result = with_store_lock(&dir, |dir| {
            let mut backup_ids = selected.clone();
            for name in files {
                let result = (|| {
                    let path = checked_file(dir, name)?.ok_or("Usage Monitor cache moved")?;
                    if name == LEDGER {
                        let Some(ids) = clean_sqlite(&path, &sources, &selected, writer_running)?
                        else {
                            return Ok(CacheCleanup::Pending);
                        };
                        backup_ids.extend(ids);
                        return Ok(CacheCleanup::Complete);
                    }
                    let ids = if name == BACKUP {
                        &backup_ids
                    } else {
                        &selected
                    };
                    clean_json_cache(
                        &path,
                        &sources,
                        ids,
                        name == CACHE && compatible,
                        writer_running,
                    )
                })();
                match result {
                    Ok(CacheCleanup::Complete) => {}
                    Ok(CacheCleanup::Pending) => return Ok(CacheCleanup::Pending),
                    Err(error) => {
                        errors.insert(format!("{}: {error}", dir.join(name).display()));
                    }
                }
            }
            Ok(CacheCleanup::Complete)
        });
        match result {
            Ok(CacheCleanup::Complete) => {}
            Ok(CacheCleanup::Pending) => return Ok(CacheCleanup::Pending),
            Err(error) => {
                errors.insert(format!("{}: {error}", dir.display()));
            }
        }
    }
    cache_cleanup_result(errors)
}

fn cache_cleanup_result(errors: BTreeSet<String>) -> Result<CacheCleanup> {
    if errors.is_empty() {
        Ok(CacheCleanup::Complete)
    } else {
        Err(errors.into_iter().collect::<Vec<_>>().join("\n").into())
    }
}

fn clean_json_cache(
    path: &Path,
    sources: &Sources,
    ids: &BTreeSet<String>,
    compatible: bool,
    writer_running: fn() -> Result<bool>,
) -> Result<CacheCleanup> {
    let bytes = fs::read(path)?;
    let mut records: Map<String, Value> = serde_json::from_slice(&bytes)?;
    if clean_records(&mut records, sources, ids, compatible) {
        if writer_running()? {
            return Ok(CacheCleanup::Pending);
        }
        if fs::read(path)? != bytes {
            return Err("Usage Monitor cache changed during cleanup".into());
        }
        replace_file(path, &serde_json::to_vec(&records)?)?;
    }
    Ok(CacheCleanup::Complete)
}

fn clean_sqlite(
    path: &Path,
    sources: &Sources,
    selected: &BTreeSet<String>,
    writer_running: fn() -> Result<bool>,
) -> Result<Option<BTreeSet<String>>> {
    let dir = path.parent().ok_or("Invalid Usage Monitor ledger path")?;
    for suffix in ["-wal", "-shm", "-journal"] {
        checked_file(dir, &format!("{LEDGER}{suffix}"))?;
    }
    let mut conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    conn.busy_timeout(Duration::from_secs(2))?;
    conn.pragma_update(None, "trusted_schema", false)?;
    conn.pragma_update(None, "secure_delete", true)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: String = tx.query_row(
        "SELECT value FROM ledger_meta WHERE key = 'schema_version'",
        [],
        |row| row.get(0),
    )?;
    if version != "1" {
        return Err(format!("Unsupported Usage Monitor ledger schema: {version}").into());
    }

    let mut matched = BTreeSet::new();
    let mut changed = false;
    {
        let mut list =
            tx.prepare("SELECT cascade_id, is_archived, meta_json FROM conversations")?;
        let mut rows = list.query([])?;
        let mut targets = Vec::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            if !valid_id(&id) {
                continue;
            }
            let archived: bool = row.get(1)?;
            let meta: Value = serde_json::from_str(row.get_ref(2)?.as_str()?)?;
            let lower = id.to_ascii_lowercase();
            if sources.missing(&lower, &meta, selected, archived) {
                matched.insert(lower);
                targets.push((id, archived, meta));
            }
        }
        let mut update_meta = tx.prepare(
            "UPDATE conversations SET meta_json = ?2, is_archived = 1 WHERE cascade_id = ?1",
        )?;
        let mut update_steps = tx.prepare(
            "UPDATE steps SET detail_json = json_remove(detail_json, ?2, ?3, ?4)
             WHERE cascade_id = ?1 AND (json_type(detail_json, ?2) IS NOT NULL
                 OR json_type(detail_json, ?3) IS NOT NULL OR json_type(detail_json, ?4) IS NOT NULL)",
        )?;
        let paths = STEP_PROMPTS.map(|field| format!("$.{field}"));
        for (id, archived, mut meta) in targets {
            if redact_header(&mut meta) || !archived {
                update_meta.execute(params![id, serde_json::to_string(&meta)?])?;
                changed = true;
            }
            changed |= update_steps.execute(params![id, paths[0], paths[1], paths[2]])? > 0;
        }
    }
    if changed {
        // Force cached readers to reload even when only step details changed
        tx.execute(
            "UPDATE ledger_meta SET value = CAST(value AS INTEGER) + 1 WHERE key = 'ledger_epoch'",
            [],
        )?;
        if writer_running()? {
            return Ok(None);
        }
    }
    tx.commit()?;
    if !matched.is_empty() {
        let busy: i64 = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        if busy != 0 {
            return Ok(None);
        }
    }
    Ok(Some(matched))
}

fn reads_history(home: &Path) -> bool {
    if !super::plugin_install::installed(home).is_ok_and(|v| v.is_some()) {
        return false;
    }
    let read = |name| {
        checked_file(home, &format!("{PLUGIN}/sidecars/panel/backend/{name}"))
            .ok()
            .flatten()
            .and_then(|p| fs::read_to_string(p).ok())
            .map(|s| s.replace("\r\n", "\n"))
    };
    let (Some(config), Some(aggregator)) = (read("config.mjs"), read("usage-aggregator.mjs"))
    else {
        return false;
    };
    // Upstream v1.5.3 shipped before and after ab59b5f; hash the three history-reader functions
    let matches = |source: &str, start: &str, end: &str, digest: &str| {
        source
            .split_once(start)
            .and_then(|(_, tail)| tail.split_once(end))
            .is_some_and(|(body, _)| format!("{:x}", Sha256::digest(body)) == digest)
    };
    matches(
        &config,
        "const HISTORY_PREFIX = 'usage-history:';",
        "export function hasUsageHistory(",
        "85c78fac1c1192296ba401d5195bbe1d22e1b475e33e4e435df4c4c31af92b99",
    ) && matches(
        &config,
        "export function flushTrajectoryCacheNow() {",
        "\n}",
        "b657cce0b8ef19b133b3815f45c76d5a7f3cff71230ada7ea67af86a4d8ec608",
    ) && matches(
        &aggregator,
        "function collectUsageEntriesToAggregate(familySet, endpoint = null) {",
        "\n}",
        "967eee5c283bd2885922ff69d1881a5e8ce267f2b19cd4b9385e2710bf78d0bd",
    )
}

fn history_key(id: &str) -> String {
    format!(
        "usage-history:{:x}",
        Sha256::digest(id.to_ascii_lowercase())
    )
}

fn covers_usage(record: &Value, archive: &Value) -> bool {
    let Some([saved]) = archive.as_array().map(Vec::as_slice) else {
        return false;
    };
    let (Some(steps), Some(copy)) = (record["steps"].as_array(), saved["steps"].as_array()) else {
        return false;
    };
    ["appSource", "accountName"]
        .iter()
        .all(|k| record.get(k) == saved.get(k))
        && steps.len() == copy.len()
        && steps.iter().zip(copy).all(|(step, snapshot)| {
            step.is_object()
                && snapshot.is_object()
                && USAGE_FIELDS
                    .iter()
                    .all(|field| step.get(field) == snapshot.get(field))
        })
}

fn redact_header(record: &mut Value) -> bool {
    let mut changed = false;
    if record["title"].as_str() != Some(DELETED) {
        record["title"] = DELETED.into();
        changed = true;
    }
    if let Some(swarm) = record.get_mut("swarmMeta").and_then(Value::as_object_mut) {
        changed |= swarm.remove("title").is_some();
    }
    if let Some(spec) = record
        .get_mut("subagentSpec")
        .and_then(Value::as_object_mut)
    {
        changed |= spec.remove("initialPrompt").is_some();
        if spec.get("typeName").and_then(Value::as_str) == Some("side_question") {
            changed |= spec.remove("role").is_some();
        }
    }
    changed
}

fn redact(record: &mut Value) -> bool {
    let mut changed = redact_header(record);
    for step in record
        .get_mut("steps")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if let Some(fields) = step.as_object_mut() {
            for key in STEP_PROMPTS {
                changed |= fields.remove(key).is_some();
            }
        }
    }
    changed
}

#[derive(Default)]
struct Sources {
    live: HashSet<String>,
    dirs: HashSet<PathBuf>,
    app: bool,
    cli: bool,
    external: HashSet<String>,
    external_complete: bool,
}

impl Sources {
    fn missing(
        &self,
        id: &str,
        record: &Value,
        selected: &BTreeSet<String>,
        archived: bool,
    ) -> bool {
        if self.live.contains(id)
            || !record.is_object()
            || record
                .get("steps")
                .is_some_and(|v| !v.as_array().is_some_and(|s| s.iter().all(Value::is_object)))
        {
            return false;
        }
        let known_source = match record["appSource"].as_str() {
            Some("cli") => self.cli,
            Some("standalone") => self.app,
            None | Some("") => self.app || self.cli,
            _ => return false,
        };
        if let Some(dirs) = record.get("localStoreDirs") {
            return dirs.as_array().is_some_and(|dirs| {
                !dirs.is_empty()
                    && dirs
                        .iter()
                        .all(|d| d.as_str().is_some_and(|s| self.dirs.contains(Path::new(s))))
            });
        }
        selected.contains(id)
            || known_source
                && (archived
                    || record["diskMtimeMs"].as_f64().is_some_and(|ms| ms > 0.0)
                    || record["offlineSqliteTelemetry"] == true
                    || record["diskFileSig"]
                        .as_str()
                        .is_some_and(|s| !s.is_empty()))
    }

    fn read_external(&mut self, root: &Path) -> Result<()> {
        if let Some(file) = checked_file(root, "brain/.deep_stats_cache.json")? {
            let data: Value = serde_json::from_reader(fs::File::open(file)?)?;
            let entries = data["perConvo"]
                .as_object()
                .ok_or("Unknown deep stats format")?;
            self.external.extend(
                entries
                    .keys()
                    .map(|k| k.rsplit(':').next().unwrap().to_ascii_lowercase()),
            );
        }
        Ok(())
    }
}

fn clean_records(
    records: &mut Map<String, Value>,
    sources: &Sources,
    selected: &BTreeSet<String>,
    compatible: bool,
) -> bool {
    let mut linked = HashSet::new();
    let mut seen = HashSet::new();
    for (id, row) in records.iter().filter(|(id, _)| valid_id(id)) {
        if !seen.insert(id.to_ascii_lowercase()) {
            linked.insert(id.to_ascii_lowercase());
        }
        for field in [
            "rootConversationId",
            "parentConversationId",
            "parentCascadeId",
        ] {
            if let Some(parent) = row[field]
                .as_str()
                .filter(|p| !p.is_empty() && !p.eq_ignore_ascii_case(id))
            {
                linked.insert(parent.to_ascii_lowercase());
                linked.insert(id.to_ascii_lowercase());
            }
        }
        for child in row["subagentRefs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r["cascadeId"].as_str())
        {
            linked.insert(child.to_ascii_lowercase());
            linked.insert(id.to_ascii_lowercase());
        }
    }
    let mut remove = Vec::new();
    let mut mask = Vec::new();
    for (key, row) in records.iter().filter(|(key, _)| valid_id(key)) {
        let id = key.to_ascii_lowercase();
        if !sources.missing(&id, row, selected, row["archived"] == true) {
            continue;
        }
        if compatible
            && sources.external_complete
            && !sources.external.contains(&id)
            && !linked.contains(&id)
            && records
                .get(&history_key(&id))
                .is_some_and(|a| covers_usage(row, a))
        {
            remove.push(key.clone());
        } else {
            mask.push(key.clone());
        }
    }
    let mut changed = !remove.is_empty();
    for key in remove {
        records.remove(&key);
    }
    for key in mask {
        changed |= redact(records.get_mut(&key).unwrap());
    }
    changed
}

/// Migration archives Usage Monitor keeps for these sessions, as (lowercase ID, archive path)
pub(super) fn migration_archives(
    root: &Path,
    ids: &HashSet<String>,
) -> Result<BTreeSet<(String, PathBuf)>> {
    let mut archives = BTreeSet::new();
    if ids.is_empty() {
        return Ok(archives);
    }
    let source = match root.file_name().and_then(|name| name.to_str()) {
        Some("antigravity") => "desktop",
        Some("antigravity-cli") => "cli",
        _ => return Ok(archives),
    };
    let Some(base) = root.parent() else {
        return Ok(archives);
    };
    let archive = Path::new("session-migration-backups/archived-sources").join(source);
    let data_dirs = [
        "antigravity",
        "antigravity-cli",
        "antigravity-ide",
        "antigravity-idea",
    ]
    .into_iter()
    .flat_map(|store| PANEL_DATA_DIRS.map(|data| Path::new(store).join(data)))
    .chain([PathBuf::from(
        "config/plugins/usage-monitor/sidecars/panel/.data",
    )]);
    for data in data_dirs {
        let Some(dir) = checked_path(base, &data.join(&archive))? else {
            continue;
        };
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some((id, _)) = name
                .to_str()
                .and_then(|name| name.strip_suffix(".db"))
                .and_then(|stem| stem.split_once('_'))
            else {
                continue;
            };
            let id = id.to_ascii_lowercase();
            if ids.contains(&id) && entry.file_type()?.is_file() {
                archives.insert((id, entry.path()));
            }
        }
    }
    Ok(archives)
}

/// A full move archives the source, removes its `.db`, and gives the destination a new UUID that
/// can still reference the source brain. A failed move keeps the source `.db` beside the archive
pub(super) fn protect_moved_attachments(
    root: &Path,
    archives: &BTreeSet<(String, PathBuf)>,
) -> Result<()> {
    for (id, _) in archives {
        if checked_file(root, &format!("conversations/{id}.db"))?.is_none() {
            return Err(format!(
                "Usage Monitor archived session {id} during migration. Its source attachments may still be in use"
            )
            .into());
        }
    }
    Ok(())
}

/// Refuse to commit if Usage Monitor archived a session while cleanup was running
pub(super) fn protect_new_archives(
    root: &Path,
    ids: &HashSet<String>,
    before: &BTreeSet<(String, PathBuf)>,
) -> Result<()> {
    if let Some((id, _)) = migration_archives(root, ids)?.difference(before).next() {
        return Err(format!(
            "Usage Monitor archived session {id} during cleanup. Its source attachments may still be in use"
        )
        .into());
    }
    Ok(())
}

// Standalone Usage Monitor can still republish its in-memory cache after Antigravity exits
pub(super) fn writer_running() -> Result<bool> {
    let mut system = System::new()?;
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_cwd(UpdateKind::OnlyIfNotSet),
    );
    if system.processes().is_empty() {
        return Err("Could not check Usage Monitor processes".into());
    }
    for process in system.processes().values() {
        let executable = process.name().to_string_lossy().to_ascii_lowercase();
        if !matches!(
            executable.trim_end_matches(".exe"),
            "node" | "nodejs" | "bun"
        ) {
            continue;
        }
        if process.cmd().iter().any(|arg| {
            let script = Path::new(arg);
            let path = process.cwd().unwrap_or(Path::new("")).join(script);
            let panel = path.parent().unwrap_or(Path::new(""));
            panel.ends_with("sidecars/panel")
                && matches!(
                    path.file_name().and_then(|s| s.to_str()),
                    Some("standalone.mjs" | "main.mjs" | "mcp-entry.mjs" | "bridge-daemon.mjs")
                )
                && (panel.join("backend/usage-aggregator.mjs").is_file()
                    || panel.ends_with("usage-monitor/sidecars/panel"))
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const GONE: &str = "03c99748-3260-4cd8-bc4f-0dc527ac4101";
    const LIVE: &str = "aac8ef39-9b48-4f89-a778-39df18634944";

    fn fixture() -> (Value, Value) {
        let usage = json!({"accountName":"test", "createdAtMs":1794124800000_i64,
            "modelKey":"gemini-4-carbon-high", "inputTokens":70_000, "cacheReadTokens":20_000,
            "outputTokens":10_000,"thinkingTokens":123,"totalDurationSec":2.125});
        let archive = json!([{"appSource":"cli","accountName":"test","steps":[usage.clone()]}]);
        let mut step = usage;
        step["turnPrompt"] = "private prompt".into();
        step["turnPromptClean"] = "clean private prompt".into();
        step["anomaly"] = json!({"count":1});
        let row = json!({"title":"private title", "swarmMeta":{"title":"private swarm","other":1},
            "diskMtimeMs":1200,"appSource":"cli","accountName":"test","steps":[step]});
        (row, archive)
    }

    #[test]
    fn unreadable_ledger_does_not_block_json_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let cli = home.join(".gemini/antigravity-cli");
        fs::create_dir_all(cli.join("conversations")).unwrap();
        let data = cli.join(PANEL_DATA_DIRS[0]);
        fs::create_dir_all(&data).unwrap();
        let (mut row, _) = fixture();
        for name in [CACHE, BACKUP] {
            fs::write(data.join(name), json!({GONE:row}).to_string()).unwrap();
        }
        fs::write(data.join(LEDGER), b"bad ledger").unwrap();

        let error = sweep(home, || Ok(false)).err().unwrap();
        assert!(error.to_string().contains(LEDGER));
        assert_eq!(fs::read(data.join(LEDGER)).unwrap(), b"bad ledger");
        assert!(redact(&mut row));
        for name in [CACHE, BACKUP] {
            let cache: Value = serde_json::from_slice(&fs::read(data.join(name)).unwrap()).unwrap();
            assert_eq!(cache[GONE], row);
        }
    }

    #[test]
    fn sweeps_sqlite_and_backup_without_changing_live_sessions_or_usage() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let cli = home.join(".gemini/antigravity-cli");
        let data = cli.join(PANEL_DATA_DIRS[0]);
        let other = home.join(".cc-switch/agy/work/antigravity-cli/conversations");
        fs::create_dir_all(cli.join("conversations")).unwrap();
        fs::create_dir_all(other.parent().unwrap().join("brain")).unwrap();
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join(format!("{LIVE}.pb")), [1]).unwrap();
        fs::create_dir_all(&data).unwrap();
        let path = data.join(LEDGER);
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE ledger_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO ledger_meta VALUES ('schema_version', '1'), ('ledger_epoch', '1');
             CREATE TABLE conversations (cascade_id TEXT PRIMARY KEY, lower_id TEXT NOT NULL UNIQUE,
                 is_archived INTEGER NOT NULL, step_count INTEGER NOT NULL, write_seq INTEGER NOT NULL,
                 updated_at_ms INTEGER NOT NULL, meta_json TEXT NOT NULL);
             CREATE TABLE steps (cascade_id TEXT NOT NULL, pos INTEGER NOT NULL,
                 accounting_json TEXT NOT NULL, detail_json TEXT, PRIMARY KEY (cascade_id, pos));",
        )
        .unwrap();
        let meta = json!({"title":"private title", "appSource":"cli", "swarmMeta":{"title":"private swarm"}});
        let accounting = "[1,1794124800000,0,\"M\",\"\",\"\",\"\",\"\",null,19,3,7,1,6]";
        let details = json!({"turnPrompt":"private prompt", "turnPromptClean":"clean prompt",
            "userPrompt":"private user prompt", "ttftSec":2});
        for id in [GONE, LIVE] {
            db.execute(
                "INSERT INTO conversations VALUES (?1, ?1, 1, 1, 2, 3, ?2)",
                params![id, meta.to_string()],
            )
            .unwrap();
            db.execute(
                "INSERT INTO steps VALUES (?1, 0, ?2, ?3)",
                params![id, accounting, details.to_string()],
            )
            .unwrap();
        }
        drop(db);
        let mut backup_row = meta.clone();
        backup_row["steps"] = json!([{"turnPrompt":"old prompt", "inputTokens":19}]);
        let backup = data.join(BACKUP);
        fs::write(
            &backup,
            json!({GONE:backup_row, LIVE:backup_row}).to_string(),
        )
        .unwrap();

        assert!(matches!(
            sweep(home, || Ok(true)).unwrap(),
            CacheCleanup::Pending
        ));
        let sources = Sources {
            cli: true,
            live: [LIVE.to_owned()].into(),
            ..Default::default()
        };
        assert!(clean_sqlite(
            &path.canonicalize().unwrap(),
            &sources,
            &BTreeSet::new(),
            || Ok(true),
        )
        .unwrap()
        .is_none());
        let db = Connection::open(&path).unwrap();
        let still_private: String = db
            .query_row(
                "SELECT meta_json FROM conversations WHERE cascade_id = ?1",
                [GONE],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(still_private, meta.to_string());
        drop(db);
        for _ in 0..2 {
            assert!(matches!(
                sweep(home, || Ok(false)).unwrap(),
                CacheCleanup::Complete
            ));
        }
        let db = Connection::open(&path).unwrap();
        for id in [GONE, LIVE] {
            let (stored_meta, stored_details, stored_accounting): (String, String, String) = db
                .query_row(
                    "SELECT meta_json, detail_json, accounting_json FROM conversations
                     JOIN steps USING (cascade_id) WHERE cascade_id = ?1",
                    [id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!(stored_accounting, accounting);
            if id == GONE {
                let mut redacted = meta.clone();
                redacted["title"] = json!(DELETED);
                redacted["swarmMeta"] = json!({});
                assert_eq!(
                    serde_json::from_str::<Value>(&stored_meta).unwrap(),
                    redacted
                );
                assert_eq!(
                    serde_json::from_str::<Value>(&stored_details).unwrap(),
                    json!({"ttftSec":2})
                );
            } else {
                assert_eq!(stored_meta, meta.to_string());
                assert_eq!(stored_details, details.to_string());
            }
        }
        let epoch: String = db
            .query_row(
                "SELECT value FROM ledger_meta WHERE key = 'ledger_epoch'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(epoch, "2");
        let after: Value = serde_json::from_slice(&fs::read(backup).unwrap()).unwrap();
        assert_eq!(after[GONE]["title"], DELETED);
        assert_eq!(after[GONE]["steps"][0], json!({"inputTokens":19}));
        assert_eq!(after[LIVE], backup_row);
    }

    #[test]
    fn redacts_only_titles_and_prompt_fields_without_a_verified_accounting_copy() {
        let (row, archive) = fixture();
        let mut entries = Map::from_iter([
            (GONE.into(), row.clone()),
            (LIVE.into(), row.clone()),
            (history_key(GONE), archive.clone()),
            ("future:entry".into(), json!([1])),
        ]);
        let sources = Sources {
            live: [LIVE.into()].into(),
            cli: true,
            external_complete: true,
            ..Sources::default()
        };
        assert!(clean_records(
            &mut entries,
            &sources,
            &BTreeSet::new(),
            false
        ));
        let mut expected = row.clone();
        expected["title"] = DELETED.into();
        expected["swarmMeta"]
            .as_object_mut()
            .unwrap()
            .remove("title");
        let step = expected["steps"][0].as_object_mut().unwrap();
        step.remove("turnPrompt");
        step.remove("turnPromptClean");
        assert_eq!(entries[GONE], expected);
        assert_eq!(entries[LIVE], row);
        assert_eq!(entries[&history_key(GONE)], archive);
        assert_eq!(entries["future:entry"], json!([1]));
        assert!(!clean_records(
            &mut entries,
            &sources,
            &BTreeSet::new(),
            false
        ));
    }

    #[test]
    fn retires_only_an_exact_usage_copy_with_the_same_attribution() {
        let (row, archive) = fixture();
        let sources = Sources {
            cli: true,
            external_complete: true,
            ..Sources::default()
        };
        let key = history_key(GONE);
        for (field, value) in [
            ("inputTokens", json!(1)),
            ("accountName", json!("other")),
            ("createdAtMs", json!(0)),
            ("modelKey", json!("other")),
            ("outputTokens", json!(1)),
        ] {
            let mut bad = archive.clone();
            bad[0]["steps"][0][field] = value;
            let mut entries =
                Map::from_iter([(GONE.into(), row.clone()), (key.clone(), bad.clone())]);
            assert!(clean_records(
                &mut entries,
                &sources,
                &BTreeSet::new(),
                true
            ));
            assert_eq!(entries[GONE]["title"], DELETED);
            assert_eq!(entries[&key], bad);
        }
        assert!(!covers_usage(&row, &json!([{"steps":[]}])));
        let mut entries = Map::from_iter([
            (GONE.to_uppercase(), row.clone()),
            (key.clone(), archive.clone()),
        ]);
        assert!(clean_records(
            &mut entries,
            &sources,
            &BTreeSet::new(),
            true
        ));
        assert_eq!(entries, Map::from_iter([(key.clone(), archive.clone())]));
        assert!(!clean_records(
            &mut entries,
            &sources,
            &BTreeSet::new(),
            true
        ));
        for grouped in [true, false] {
            entries.insert(GONE.into(), row.clone());
            if grouped {
                entries.get_mut(GONE).unwrap()["rootConversationId"] = LIVE.into();
            }
            let external = if grouped {
                HashSet::new()
            } else {
                [GONE.into()].into()
            };
            let overlapping = Sources {
                cli: true,
                external_complete: true,
                external,
                ..Sources::default()
            };
            clean_records(&mut entries, &overlapping, &BTreeSet::new(), true);
            assert!(entries.contains_key(GONE));
        }
    }
    #[test]
    #[ignore = "requires Node and checkouts of the original and archive-capable Usage Monitor readers"]
    fn real_readers_keep_the_same_usage_and_cost() {
        use std::process::Command;
        for (variable, capable) in [
            ("USAGE_MONITOR_LEGACY_SRC", false),
            ("USAGE_MONITOR_ARCHIVE_SRC", true),
        ] {
            let source = PathBuf::from(std::env::var_os(variable).expect(variable))
                .canonicalize()
                .unwrap();
            let tmp = tempfile::tempdir().unwrap();
            let home = tmp.path();
            let plugin = home.join(PLUGIN);
            let backend = plugin.join("sidecars/panel/backend");
            fs::create_dir_all(&backend).unwrap();
            fs::copy(source.join("plugin.json"), plugin.join("plugin.json")).unwrap();
            for file in ["config.mjs", "usage-aggregator.mjs"] {
                fs::copy(
                    source.join("sidecars/panel/backend").join(file),
                    backend.join(file),
                )
                .unwrap();
            }
            assert_eq!(reads_history(home), capable);
            let root = home.join(".gemini/antigravity-cli");
            let data = root.join("sidecar_data/usage-monitor/panel/data");
            fs::create_dir_all(&data).unwrap();
            fs::create_dir_all(root.join("conversations")).unwrap();
            let (row, archive) = fixture();
            let before = json!({GONE:row, (history_key(GONE)):archive});
            let original = home.join("before.json");
            let cache = data.join(CACHE);
            fs::write(&original, serde_json::to_vec(&before).unwrap()).unwrap();
            fs::copy(&original, &cache).unwrap();
            clean(
                home,
                &Cleaned {
                    root,
                    ids: [GONE.to_owned()].into(),
                },
                || Ok(false),
            )
            .unwrap();
            let after: Value = serde_json::from_reader(fs::File::open(&cache).unwrap()).unwrap();
            assert_eq!(after.get(GONE).is_none(), capable);
            assert_eq!(after[history_key(GONE)], archive);
            let script = r#"
                import assert from 'node:assert/strict';
                import {readFileSync} from 'node:fs';
                import {backendHarness} from './sidecars/panel/test/backend-harness.mjs';
                const reports = [];
                for (const file of process.argv.slice(1)) {
                    const h = backendHarness({cache:JSON.parse(readFileSync(file)),clock:()=>1794124805000});
                    const totals = [];
                    for (const range of ['all','24h','7d']) {
                        const report = await h.buildTelemetryPayloadInternal('',true,{range});
                        assert.equal(report.consumption.account.totalTokens, 100000);
                        assert.ok(report.consumption.account.costUsd > 0);
                        totals.push(report.consumption.account);
                        for (const [name, preset] of Object.entries(report.consumption.presets)) {
                            assert.ok(preset.account);
                            totals.push([name, preset.account]);
                        }
                    }
                    reports.push(JSON.stringify(totals));
                }
                assert.equal(reports[0],reports[1]);
            "#;
            assert!(Command::new("node")
                .current_dir(source)
                .args(["--input-type=module", "-e", script])
                .arg(original)
                .arg(cache)
                .status()
                .unwrap()
                .success());
        }
    }
}
