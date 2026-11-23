//! Anti-Power Usage ledger: move removed sessions into its anonymous deleted totals
//!
//! Compatibility with ledgers written by the former bundled plugin

use super::antigravity_clean::{conversation_db_id, store_read_lock, STAGE_PREFIX};
use super::antigravity_store::{checked_file, checked_path, strict_roots, CLI, DESKTOP};
use super::cleanup_extras::Cleaned;
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub(super) const PLUGIN: &str = "anti-power-usage";
const LEDGER: &str = "ledger.json";
const PENDING: &str = "ledger.json.pending";
const TEMP_PREFIX: &str = ".anti-power-ledger-";
const TEMP_SUFFIX: &str = ".tmp";
const TEMP_RANDOM_LEN: usize = 12;
const LEDGER_VERSION: u64 = 1;
const COUNTERS: [&str; 6] = [
    "steps",
    "inputTokens",
    "cacheReadTokens",
    "outputTokens",
    "thinkingTokens",
    "unpricedSteps",
];
// The host picks the sidecar data folder, so search a few levels under the plugin's folder
const SEARCH_DEPTH: usize = 3;

/// Fold explicitly deleted database copies into every ledger found
pub(super) fn fold(
    home: &Path,
    cleaned: &Cleaned,
    deleted_databases: &BTreeSet<PathBuf>,
) -> Result<()> {
    if cleaned.ids.is_empty() {
        return Ok(());
    }
    let deleted_ids: BTreeSet<_> = deleted_databases
        .iter()
        .filter_map(|path| conversation_db_id(path))
        .filter(|id| cleaned.ids.contains(*id))
        .map(str::to_ascii_lowercase)
        .collect();
    let selected = store_key(home, &cleaned.root)?;
    let mut stores = strict_roots(home, DESKTOP)?;
    stores.extend(strict_roots(home, CLI)?);
    for path in ledgers(home, &stores)? {
        let dir = path.parent().ok_or("Missing ledger directory")?;
        let lock_name = "ledger.json.lock";
        for suffix in ["", "-wal", "-shm", "-journal"] {
            checked_path(dir, Path::new(&format!("{lock_name}{suffix}")))?;
        }
        let mut lock = Connection::open_with_flags(
            dir.canonicalize()?.join(lock_name),
            OpenFlags::SQLITE_OPEN_NOFOLLOW
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;
        lock.busy_timeout(Duration::from_secs(5))?;
        let _tx = lock.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let main = checked_file(dir, LEDGER)?;
        let pending = checked_file(dir, PENDING)?;
        if let Some(latest) = pending.as_ref().or(main.as_ref()) {
            let mut ledger = read_ledger(latest)?;
            let mut store_locks = Vec::new();
            let changed = if deleted_ids.is_empty() {
                false
            } else {
                let live =
                    live_copies(home, &stores, &cleaned.root, &deleted_ids, &mut store_locks)?;
                fold_deleted_copies(&mut ledger, &deleted_ids, &selected, &live)?
            };
            if changed {
                write_pending(dir, &ledger)?;
            }
            if changed || pending.is_some() {
                fs::rename(dir.join(PENDING), &path)?;
            }
        }
        remove_ledger_copies(dir)?;
    }
    Ok(())
}

fn write_pending(dir: &Path, ledger: &Map<String, Value>) -> Result<()> {
    let mut tmp = tempfile::Builder::new()
        .prefix(TEMP_PREFIX)
        .suffix(TEMP_SUFFIX)
        .rand_bytes(TEMP_RANDOM_LEN)
        .tempfile_in(dir)?;
    serde_json::to_writer(&mut tmp, ledger)?;
    tmp.write_all(b"\n")?;
    tmp.as_file().sync_all()?;
    tmp.persist(dir.join(PENDING))?;
    Ok(())
}

fn store_key(home: &Path, root: &Path) -> Result<String> {
    let relative = root.strip_prefix(home)?;
    if relative == Path::new(".gemini/antigravity") {
        return Ok("app".into());
    }
    if relative == Path::new(".gemini/antigravity-cli") {
        return Ok("cli".into());
    }
    let profile = relative
        .strip_prefix(".cc-switch/agy")?
        .parent()
        .and_then(Path::to_str)
        .ok_or("Invalid CC Switch profile store")?;
    Ok(format!("cc-switch:{profile}"))
}

fn live_copies(
    home: &Path,
    stores: &[PathBuf],
    selected: &Path,
    ids: &BTreeSet<String>,
    locks: &mut Vec<fs::File>,
) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let mut live = BTreeMap::<String, BTreeSet<String>>::new();
    for root in stores {
        let Some(root) = checked_path(home, root.strip_prefix(home)?)? else {
            continue;
        };
        if root != selected {
            locks.push(store_read_lock(&root)?);
        }
        for entry in fs::read_dir(&root)? {
            if entry?
                .file_name()
                .to_string_lossy()
                .starts_with(STAGE_PREFIX)
            {
                return Err("Another session cleanup is pending recovery".into());
            }
        }
        if let Some(dir) = checked_path(&root, Path::new("conversations"))? {
            let key = store_key(home, &root)?;
            for entry in fs::read_dir(dir)? {
                let name = entry?.file_name().to_string_lossy().to_ascii_lowercase();
                if let Some(id) = name.strip_suffix(".db").filter(|id| ids.contains(*id)) {
                    live.entry(id.to_owned()).or_default().insert(key.clone());
                }
            }
        }
    }
    Ok(live)
}

fn ledgers(home: &Path, stores: &[PathBuf]) -> Result<BTreeSet<PathBuf>> {
    let mut found = BTreeSet::new();
    for root in stores {
        let relative = root.strip_prefix(home)?.join("sidecar_data").join(PLUGIN);
        if let Some(dir) = checked_path(home, &relative)? {
            find_ledgers(&dir, SEARCH_DEPTH, &mut found)?;
        }
    }
    Ok(found)
}

fn numbered(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
}

fn random_suffix(name: &str, prefix: &str, len: usize) -> bool {
    name.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == len && suffix.bytes().all(|b| b.is_ascii_alphanumeric())
    })
}

fn is_owned_temp(name: &str) -> bool {
    numbered(name, "ledger.json.tmp-")
        || name
            .strip_suffix(TEMP_SUFFIX)
            .is_some_and(|stem| random_suffix(stem, TEMP_PREFIX, TEMP_RANDOM_LEN))
}

fn is_legacy_rust_copy(path: &Path, name: &str) -> bool {
    // Generic `.tmpXXXXXX` names are shared; only remove recognizable v1 ledgers
    random_suffix(name, ".tmp", 6) && read_ledger(path).is_ok()
}

fn remove_ledger_copies(dir: &Path) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let path = entry.path();
        let obsolete = if numbered(&name, "ledger.json.broken-") {
            match serde_json::from_reader::<_, Value>(fs::File::open(&path)?) {
                Ok(value) => value.get("version").is_none_or(current_version),
                Err(error) if error.is_io() => {
                    return Err(format!("Cannot inspect {}: {error}", path.display()).into());
                }
                Err(_) => true,
            }
        } else {
            is_owned_temp(&name) || is_legacy_rust_copy(&path, &name)
        };
        if obsolete {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

// Walk without following symlinks
fn find_ledgers(dir: &Path, depth: usize, found: &mut BTreeSet<PathBuf>) -> io::Result<()> {
    if !fs::symlink_metadata(dir)?.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if kind.is_file()
            && (matches!(name.as_ref(), LEDGER | PENDING)
                || is_owned_temp(&name)
                || numbered(&name, "ledger.json.broken-")
                || is_legacy_rust_copy(&entry.path(), &name))
        {
            found.insert(dir.join(LEDGER));
        } else if kind.is_dir() && depth > 0 {
            find_ledgers(&entry.path(), depth - 1, found)?;
        }
    }
    Ok(())
}

fn read_ledger(path: &Path) -> Result<Map<String, Value>> {
    let invalid = || {
        format!(
            "Unrecognized Anti-Power Usage ledger format at {}",
            path.display()
        )
    };
    let ledger: Map<String, Value> =
        serde_json::from_reader(fs::File::open(path)?).map_err(|_| invalid())?;
    if !ledger.get("version").is_some_and(current_version)
        || !ledger
            .get("sessions")
            .and_then(Value::as_object)
            .is_some_and(|sessions| sessions.values().all(valid_session))
        || !ledger.get("deleted").is_some_and(valid_buckets)
    {
        return Err(invalid().into());
    }
    Ok(ledger)
}

fn current_version(version: &Value) -> bool {
    version.as_f64() == Some(LEDGER_VERSION as f64)
}

fn valid_session(entry: &Value) -> bool {
    entry.as_object().is_some_and(|entry| {
        entry.get("buckets").is_some_and(valid_buckets)
            && entry.get("title").is_none_or(Value::is_string)
            && entry.get("legacy").is_none_or(valid_buckets)
            && entry.get("requests").is_none_or(|requests| {
                requests.as_object().is_some_and(|requests| {
                    requests.iter().all(|(step, request)| {
                        !step.is_empty()
                            && step.bytes().all(|b| b.is_ascii_digit())
                            && valid_bucket(request)
                            && valid_counter(request.get("createdAtMs"))
                    })
                })
            })
            && entry.get("knownCopies").is_none_or(|copies| {
                copies.as_array().is_some_and(|copies| {
                    copies
                        .iter()
                        .all(|key| key.as_str().is_some_and(valid_copy_key))
                })
            })
            && entry.get("missingCopies").is_none_or(|copies| {
                copies.as_object().is_some_and(|copies| {
                    copies.iter().all(|(key, time)| {
                        valid_copy_key(key) && (time.is_null() || valid_counter(Some(time)))
                    })
                })
            })
    })
}

fn valid_buckets(buckets: &Value) -> bool {
    buckets
        .as_array()
        .is_some_and(|rows| rows.iter().all(valid_bucket))
}

fn valid_bucket(bucket: &Value) -> bool {
    ["day", "model", "client"]
        .iter()
        .all(|key| bucket[*key].is_string())
        && bucket
            .get("label")
            .is_none_or(|label| label.is_string() || label.is_null())
        && COUNTERS
            .into_iter()
            .chain(["cost"])
            .all(|key| valid_counter(bucket.get(key)))
}

fn valid_counter(value: Option<&Value>) -> bool {
    value.is_none_or(|value| value.as_f64().is_some_and(|number| number >= 0.0))
}

fn valid_copy_key(key: &str) -> bool {
    matches!(key, "app" | "cli")
        || key
            .strip_prefix("cc-switch:")
            .is_some_and(|name| !name.is_empty())
}

fn known_copies(entry: &Value) -> BTreeSet<String> {
    if let Some(copies) = entry["knownCopies"].as_array() {
        return copies
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
    }
    // Older entries stored only the current signatures and the preferred store
    let signatures = entry["copies"]
        .as_str()
        .and_then(|copies| serde_json::from_str::<Vec<(String, String)>>(copies).ok())
        .unwrap_or_default();
    entry["store"]
        .as_str()
        .map(str::to_owned)
        .into_iter()
        .chain(signatures.into_iter().map(|(key, _)| key))
        .filter(|key| valid_copy_key(key))
        .collect()
}

fn fold_deleted_copies(
    ledger: &mut Map<String, Value>,
    deleted_ids: &BTreeSet<String>,
    selected: &str,
    live: &BTreeMap<String, BTreeSet<String>>,
) -> Result<bool> {
    let mut buckets = Vec::new();
    let mut folded = BTreeSet::new();
    let mut changed = false;
    let sessions = ledger["sessions"]
        .as_object_mut()
        .ok_or("Missing ledger sessions")?;
    for id in deleted_ids {
        let observed = live.get(id);
        if observed.is_some_and(|copies| copies.contains(selected)) {
            continue;
        }
        if let Some(entry) = sessions.get_mut(id) {
            let mut remaining = known_copies(entry);
            let removed = remaining.remove(selected);
            remaining.extend(observed.into_iter().flatten().cloned());
            if !remaining.is_empty() {
                let entry = entry.as_object_mut().ok_or("Invalid usage session")?;
                let copies = serde_json::to_value(remaining)?;
                if entry.get("knownCopies") != Some(&copies) {
                    entry.insert("knownCopies".into(), copies);
                    changed = true;
                }
                if removed || observed.is_some() {
                    changed |= entry.shift_remove("missingSince").is_some();
                }
                if let Some(missing) = entry
                    .get_mut("missingCopies")
                    .and_then(Value::as_object_mut)
                {
                    changed |= missing.shift_remove(selected).is_some();
                }
                continue;
            }
        } else if observed.is_some() {
            continue;
        }
        folded.insert(id);
        if let Some(entry) = sessions.shift_remove(id) {
            let list = entry["buckets"].as_array().ok_or("Invalid usage buckets")?;
            buckets.extend(list.iter().cloned());
            changed = true;
        }
    }
    for entry in sessions.values_mut() {
        if entry["parentId"]
            .as_str()
            .is_some_and(|id| folded.contains(&id.to_ascii_lowercase()))
        {
            entry["parentId"] = Value::String(String::new());
            changed = true;
        }
    }
    let deleted = ledger["deleted"]
        .as_array_mut()
        .ok_or("Missing deleted usage buckets")?;
    for bucket in buckets {
        merge(deleted, &bucket);
    }
    Ok(changed)
}

// Buckets that share day, model, and client add up, as in `mergeBuckets`
fn merge(deleted: &mut Vec<Value>, bucket: &Value) {
    let keys = ["day", "model", "client"];
    let index = deleted
        .iter()
        .position(|other| keys.iter().all(|&key| other[key] == bucket[key]));
    let index = index.unwrap_or_else(|| {
        let labels =
            ["day", "model", "label", "client"].map(|key| (key.to_owned(), bucket[key].clone()));
        deleted.push(Value::Object(Map::from_iter(labels)));
        deleted.len() - 1
    });
    let total = &mut deleted[index];
    let number = |value: &Value, key: &str| value[key].as_f64().unwrap_or_default();
    for key in COUNTERS {
        total[key] = Value::from((number(total, key) + number(bucket, key)).round() as u64);
    }
    total["cost"] = Value::from(number(total, "cost") + number(bucket, "cost"));
}

#[cfg(test)]
mod tests {
    use super::*;

    const GONE: &str = "00000000-0000-4000-8000-000000000001";
    const COPIED: &str = "00000000-0000-4000-8000-000000000002";

    fn ledger_dir(app: &Path) -> PathBuf {
        let dir = app.join("sidecar_data").join(PLUGIN).join("panel/data");
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn database(id: &str) -> PathBuf {
        PathBuf::from(format!("conversations/{id}.db"))
    }

    fn deletion(root: &Path, id: &str) -> Cleaned {
        Cleaned {
            root: root.to_owned(),
            ids: BTreeSet::from([id.to_owned()]),
        }
    }

    #[test]
    fn folds_the_last_copy_and_preserves_a_session_still_in_another_store() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app = home.join(".gemini/antigravity");
        let data = ledger_dir(&app);
        let copies = home.join(".gemini/antigravity-cli/conversations");
        fs::create_dir_all(&copies).unwrap();
        fs::write(copies.join(format!("{COPIED}.db")), "fixture").unwrap();

        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/ledger-fold.json")).unwrap();
        let path = data.join(LEDGER);
        fs::write(&path, fixture["input"].to_string()).unwrap();
        let cleaned = Cleaned {
            root: app,
            ids: BTreeSet::from([
                fixture["removedId"].as_str().unwrap().to_owned(),
                COPIED.to_owned(),
            ]),
        };
        for _ in 0..2 {
            let paths = BTreeSet::from([database(GONE), database(COPIED)]);
            fold(home, &cleaned, &paths).unwrap();
            let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            let mut expected = fixture["expected"].clone();
            expected["sessions"][COPIED]["knownCopies"] = serde_json::json!(["cli"]);
            assert_eq!(saved, expected);
        }
    }

    #[test]
    #[ignore = "requires Node.js with node:sqlite"]
    fn serializes_with_the_node_sidecar() {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app = home.join(".gemini/antigravity");
        let data = ledger_dir(&app);
        let file = data.join(LEDGER);
        let ledger = serde_json::json!({"version":1, "sessions":{ GONE: {"store":"app","buckets":[{"day":"2026-11-18","model":"carbon","client":"app","steps":1,"cost":0.5}]} }, "deleted":[]});
        fs::write(&file, ledger.to_string()).unwrap();
        let script = r#"
            import { readFileSync, renameSync, writeFileSync, writeSync } from 'node:fs';
            import { DatabaseSync } from 'node:sqlite';
            const [file, id] = process.argv.slice(1);
            const lock = new DatabaseSync(file + '.lock');
            lock.exec('BEGIN IMMEDIATE');
            const latest = JSON.parse(readFileSync(file, 'utf8'));
            writeSync(1, 'locked\n');
            Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 300);
            latest.sessions[id].buckets[0].steps = 2;
            writeFileSync(file + '.pending', JSON.stringify(latest));
            renameSync(file + '.pending', file);
            lock.exec('COMMIT');
            lock.close();
        "#;
        let mut node = Command::new("node")
            .current_dir(home)
            .args(["--input-type=module", "-e", script])
            .arg(&file)
            .arg(GONE)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut ready = String::new();
        BufReader::new(node.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "locked");
        fold(
            home,
            &deletion(&app, GONE),
            &BTreeSet::from([database(GONE)]),
        )
        .unwrap();
        assert!(node.wait().unwrap().success());
        let saved: Value = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
        assert!(saved["sessions"].as_object().unwrap().is_empty());
        assert_eq!(saved["deleted"][0]["steps"], 2);
    }

    #[test]
    fn an_uninspectable_profile_blocks_anonymous_folding() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let profile = home.join(".cc-switch/agy/work");
        fs::create_dir_all(&profile).unwrap();
        fs::write(profile.join("antigravity-cli"), "not a directory").unwrap();
        let app = home.join(".gemini/antigravity");
        let data = ledger_dir(&app);
        let original = serde_json::json!({"version":1, "sessions":{ GONE: {"store":"app","buckets":[]} }, "deleted":[]}).to_string();
        fs::write(data.join(LEDGER), &original).unwrap();
        assert!(fold(
            home,
            &deletion(&app, GONE),
            &BTreeSet::from([database(GONE)]),
        )
        .is_err());
        assert_eq!(fs::read_to_string(data.join(LEDGER)).unwrap(), original);
    }

    #[test]
    fn removes_generated_ledger_copies_without_a_published_ledger() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app = home.join(".gemini/antigravity");
        let data = ledger_dir(&app);
        let obsolete = [
            ("ledger.json.tmp-1234", GONE),
            ("ledger.json.broken-42", GONE),
            (".anti-power-ledger-aBcDeFgH1234.tmp", "partial write"),
            (".tmpaB1234", r#"{"version":1,"sessions":{},"deleted":[]}"#),
        ];
        let keep = [
            ("ledger.json.broken-43", r#"{"version":2}"#),
            ("ledger.json.broken-44", r#"{"version":"next"}"#),
            (".tmpaB1235", r#"{"version":1,"payload":"keep"}"#),
            ("ledger.json.broken-notes", "keep"),
        ];
        for (name, contents) in obsolete.iter().chain(&keep) {
            fs::write(data.join(name), contents).unwrap();
        }
        fold(home, &deletion(&app, GONE), &BTreeSet::new()).unwrap();
        for (name, _) in obsolete {
            assert!(!data.join(name).exists(), "{name}");
        }
        for (name, contents) in keep {
            assert_eq!(fs::read_to_string(data.join(name)).unwrap(), contents);
        }
        assert!(!data.join(LEDGER).exists());
    }

    #[test]
    fn legacy_signatures_supply_copy_keys_only_until_known_copies_are_written() {
        let mut entry = serde_json::json!({"store":"app", "copies":"[[\"cli\",\"sig\"]]"});
        assert_eq!(
            known_copies(&entry),
            BTreeSet::from(["app".into(), "cli".into()])
        );
        entry["knownCopies"] = serde_json::json!(["cli"]);
        assert_eq!(known_copies(&entry), BTreeSet::from(["cli".into()]));
    }

    #[test]
    fn publishes_pending_and_folds_its_totals_instead_of_the_stale_main_file() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app = home.join(".gemini/antigravity");
        let data = ledger_dir(&app);
        let main = data.join(LEDGER);
        let pending = data.join(PENDING);
        let mut ledger = serde_json::json!({"version":1, "sessions":{ GONE: {
            "store":"app", "buckets":[{"day":"2026-11-18", "model":"m", "client":"app", "steps":2}]
        } }, "deleted":[]});
        let cleaned = deletion(&app, GONE);

        fs::write(&pending, ledger.to_string()).unwrap();
        fold(home, &cleaned, &BTreeSet::new()).unwrap();
        assert!(!pending.exists());
        assert_eq!(read_ledger(&main).unwrap(), *ledger.as_object().unwrap());

        ledger["sessions"][GONE]["buckets"][0]["steps"] = 8.into();
        fs::write(&pending, ledger.to_string()).unwrap();
        for _ in 0..2 {
            fold(home, &cleaned, &BTreeSet::from([database(GONE)])).unwrap();
            assert!(!pending.exists());
            let saved = read_ledger(&main).unwrap();
            assert!(saved["sessions"].as_object().unwrap().is_empty());
            assert_eq!(saved["deleted"][0]["steps"], 8);
        }
    }

    #[test]
    fn invalid_or_unsupported_ledgers_preserve_the_original_and_backup() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app = home.join(".gemini/antigravity");
        let dir = ledger_dir(&app);
        let main = dir.join(LEDGER);
        let backup = dir.join("ledger.json.broken-42");
        fs::write(&backup, "recovery data").unwrap();
        let healthy = serde_json::json!({"version":1, "sessions":{ GONE:{"store":"app","buckets":[]} }, "deleted":[]});
        let mut bad_deleted = healthy.clone();
        bad_deleted["deleted"] = serde_json::json!([
            {"day":"2026-11-18","model":"m","client":"app","cost":"invalid"}
        ]);
        let mut bad_session = healthy.clone();
        bad_session["sessions"][COPIED] = serde_json::json!({"buckets":[], "requests":{"0":[]}});
        for (name, invalid) in [
            (LEDGER, serde_json::json!({"version":2})),
            (LEDGER, bad_deleted),
            (PENDING, bad_session),
        ] {
            fs::write(&main, healthy.to_string()).unwrap();
            let path = dir.join(name);
            fs::write(&path, invalid.to_string()).unwrap();
            assert!(fold(
                home,
                &deletion(&app, GONE),
                &BTreeSet::from([database(GONE)])
            )
            .is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), invalid.to_string());
            assert_eq!(fs::read_to_string(&backup).unwrap(), "recovery data");
            if name == PENDING {
                assert_eq!(fs::read_to_string(&main).unwrap(), healthy.to_string());
            }
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    #[cfg(unix)]
    fn folding_waits_for_a_linked_profile_to_become_readable() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let profiles = home.join(".cc-switch/agy");
        let moved = home.join("relocated");
        let conversations = moved.join("antigravity-cli/conversations");
        fs::create_dir_all(&conversations).unwrap();
        fs::write(conversations.join(format!("{COPIED}.db")), "fixture").unwrap();
        fs::create_dir_all(&profiles).unwrap();
        let profile = profiles.join("work");
        std::os::unix::fs::symlink(&moved, &profile).unwrap();
        let app = home.join(".gemini/antigravity");
        let data = ledger_dir(&app);
        let ledger = data.join(LEDGER);
        let original =
            serde_json::json!({"version":1,"sessions":{COPIED:{"buckets":[]}},"deleted":[]})
                .to_string();
        fs::write(&ledger, &original).unwrap();
        let cleaned = deletion(&app, COPIED);
        let paths = BTreeSet::from([database(COPIED)]);
        assert!(fold(home, &cleaned, &paths).is_err());
        assert_eq!(fs::read_to_string(&ledger).unwrap(), original);
        fs::remove_file(&profile).unwrap();
        fs::rename(moved, profile).unwrap();
        fold(home, &cleaned, &paths).unwrap();
        let saved = read_ledger(&ledger).unwrap();
        assert_eq!(
            saved["sessions"][COPIED]["knownCopies"],
            serde_json::json!(["cc-switch:work"])
        );
    }
}
