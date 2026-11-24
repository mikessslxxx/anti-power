//! Protect migrated session attachments and clean Usage Monitor session cache

use super::antigravity_clean::{store_read_lock, with_store_lock};
use super::antigravity_scan::live_ids;
use super::antigravity_store::{checked_file, checked_path, strict_roots, valid_id, CLI, DESKTOP};
use super::cleanup_extras::{replace_file, Cleaned};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const PLUGIN: &str = ".gemini/config/plugins/usage-monitor";
const CACHE: &str = "trajectory_usage_cache.json";
const DELETED: &str = "已删除会话 / Deleted session";
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
    // A writer may still hold records it has never flushed to disk
    if writer_running()? {
        return Ok(CacheCleanup::Pending);
    }
    let mut roots = strict_roots(home, DESKTOP)?;
    roots.extend(strict_roots(home, CLI)?);
    let mut caches = BTreeSet::new();
    for root in &roots {
        for data in PANEL_DATA_DIRS {
            let relative = root.strip_prefix(home)?.join(data).join(CACHE);
            if let Some(file) = checked_path(home, &relative)? {
                caches.insert(file);
            }
        }
    }
    if let Some(file) = checked_file(home, &format!("{PLUGIN}/sidecars/panel/.data/{CACHE}"))? {
        caches.insert(file);
    }
    if caches.is_empty() {
        return Ok(CacheCleanup::Complete);
    }

    let compatible = reads_history(home);
    let mut sources = Sources {
        external_complete: compatible,
        ..Sources::default()
    };
    let mut locks = Vec::new();
    // IDE files can keep a copy alive even though Anti-Power only deletes App and CLI sessions
    for root in roots
        .into_iter()
        .chain([".gemini/antigravity-ide", ".gemini/antigravity-idea"].map(|p| home.join(p)))
    {
        if checked_path(home, root.strip_prefix(home)?)?.is_none() {
            continue;
        }
        let is_app = root == home.join(".gemini/antigravity");
        let is_cli = root.ends_with("antigravity-cli");
        if (is_app || is_cli) && root != cleaned.root {
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

    let selected = cleaned
        .ids
        .iter()
        .map(|id| id.to_ascii_lowercase())
        .collect();
    for file in caches {
        let result = with_store_lock(file.parent().ok_or("Invalid cache path")?, |dir| {
            let path = checked_file(dir, CACHE)?.ok_or("Usage Monitor cache moved")?;
            let bytes = fs::read(&path)?;
            let mut records: Map<String, Value> = serde_json::from_slice(&bytes)?;
            if clean_records(&mut records, &sources, &selected, compatible) {
                if writer_running()? {
                    return Ok(CacheCleanup::Pending);
                }
                if fs::read(&path)? != bytes {
                    return Err("Usage Monitor cache changed during cleanup".into());
                }
                replace_file(&path, &serde_json::to_vec(&records)?)?;
            }
            Ok(CacheCleanup::Complete)
        })?;
        if let CacheCleanup::Pending = result {
            return Ok(result);
        }
    }
    Ok(CacheCleanup::Complete)
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

fn redact(record: &mut Value) -> bool {
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
    for step in record
        .get_mut("steps")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if let Some(fields) = step.as_object_mut() {
            for key in ["turnPrompt", "turnPromptClean", "userPrompt"] {
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
    fn missing(&self, id: &str, record: &Value, selected: &BTreeSet<String>) -> bool {
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
                && (record["diskMtimeMs"].as_f64().is_some_and(|ms| ms > 0.0)
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
        if !sources.missing(&id, row, selected) {
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
