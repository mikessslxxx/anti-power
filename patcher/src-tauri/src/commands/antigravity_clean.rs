//! Coordinate session deletion, index cleanup, and cleanup recovery

use super::antigravity_scan::live_ids;
use super::antigravity_store::{
    checked_file, checked_path, locate, roots, valid_id, CLI, DESKTOP, INDEX_DB, METADATA,
};
use super::cc_switch;
use super::cleanup_extras::{self, Cleaned};
use super::index_only;
use super::magpie_probes;
use super::sessions::{CleanOptions, DeleteReport, RecoveryStatus, SessionRef};
use super::usage_monitor::{self, CacheCleanup};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

pub(super) const STAGE_PREFIX: &str = ".anti-power-clean-";
const PREPARE_PREFIX: &str = ".anti-power-prepare-";
const PURGE_PREFIX: &str = ".anti-power-purge-";
const COMMIT_TABLE: &str = "anti_power_cleanup_commits";
const RECEIPT: &str = "post-clean.json";
const RECEIPT_PREFIX: &str = ".anti-power-extras-";
const SYNC_PENDING: &str = ".anti-power-sync-pending";
const PLAN: &str = "plan.json";
const LOCK: &str = ".anti-power-clean.lock";
const DESKTOP_INDEX: &str = "agyhub_summaries_proto.pb";
const CLI_INDEX: &str = "jetbox_summaries_proto.pb";
pub(super) const RECENTS: &str = "cache/last_conversations.json";
pub(super) const HISTORY: &str = "history.jsonl";
const SIDECAR: &str = "sidecar_data";
type Filter = fn(&[u8], &BTreeSet<String>) -> Result<Option<Vec<u8>>>;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

// A failure that every session in the store would hit again
#[derive(Debug)]
struct StoreWide(String);

impl std::fmt::Display for StoreWide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StoreWide {}

pub(super) fn store_wide(error: impl ToString) -> Box<dyn std::error::Error> {
    Box::new(StoreWide(error.to_string()))
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Client {
    AppOrCli,
    Ide,
}

#[derive(Clone, Copy)]
enum Scope<'a> {
    Selected,
    MagpieProbes,
    MatchingIndexes,
    HistoricalIndexes(&'a Path),
}

pub(super) fn delete_magpie_probes(home: &Path, ids: &[String]) -> Result<RecoveryStatus> {
    if ids.is_empty() {
        return Ok(RecoveryStatus::default());
    }
    if !ids.iter().all(|id| valid_id(id)) || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
    {
        return Err("Invalid Magpie model probe selection".into());
    }
    let root = magpie_probes::root(home)?.ok_or("Magpie session store no longer exists")?;
    let targets: Vec<_> = ids.iter().map(|id| (id.as_str(), false)).collect();
    clean(
        &root,
        CLI_INDEX,
        &targets,
        CleanOptions::default(),
        Scope::MagpieProbes,
        || running_clients()?.require_magpie_closed().map(drop),
    )?;
    let mut recovery = RecoveryStatus::default();
    recover_store(home, &root, true, &mut recovery);
    Ok(recovery)
}

/// Delete sessions, clean leftover indexes, and update external app data
pub(super) fn delete(
    home: &Path,
    sessions: &[SessionRef],
    matching_indexes: &[SessionRef],
    orphan_indexes: &[SessionRef],
    options: CleanOptions,
) -> DeleteReport {
    let (mut report, cleaned_roots) = delete_with_indexes(
        home,
        sessions,
        matching_indexes,
        orphan_indexes,
        options,
        require_clients_closed,
    );
    for root in cleaned_roots {
        recover_store(home, &root, false, &mut report.recovery);
    }
    report
}

fn delete_with_indexes(
    home: &Path,
    sessions: &[SessionRef],
    matching_indexes: &[SessionRef],
    orphan_indexes: &[SessionRef],
    options: CleanOptions,
    check_clients: impl Fn() -> Result<()>,
) -> (DeleteReport, BTreeSet<PathBuf>) {
    let (errors, mut cleaned) =
        delete_with(home, sessions, options, Scope::Selected, &check_clients);
    let mut report = DeleteReport {
        errors,
        recovery: RecoveryStatus::default(),
        orphan_indexes_deleted: 0,
    };
    if cleaned.is_empty() {
        return (report, cleaned);
    }
    let targets = index_only::targets_after_deletion(
        sessions,
        &report.errors,
        matching_indexes,
        orphan_indexes,
    );
    for (indexes, scope) in [
        (&targets.matching, Scope::MatchingIndexes),
        (&targets.historical, Scope::HistoricalIndexes(home)),
    ] {
        if indexes.is_empty() {
            continue;
        }
        let (index_errors, index_roots) =
            delete_with(home, indexes, options, scope, &check_clients);
        cleaned.extend(index_roots);
        for (session, error) in indexes.iter().zip(index_errors) {
            if let Some(error) = error {
                report.recovery.warnings.push(format!(
                    "Could not clean index at {}: {error}",
                    session.source_path.display()
                ));
            } else if matches!(scope, Scope::HistoricalIndexes(_)) {
                report.orphan_indexes_deleted += 1;
            }
        }
    }
    (report, cleaned)
}

fn delete_with(
    home: &Path,
    sessions: &[SessionRef],
    options: CleanOptions,
    scope: Scope<'_>,
    check_clients: impl Fn() -> Result<()>,
) -> (Vec<Option<String>>, BTreeSet<PathBuf>) {
    let mut results = vec![None; sessions.len()];
    let mut cleaned = BTreeSet::new();
    let mut stores = BTreeMap::<_, Vec<_>>::new();
    for (i, session) in sessions.iter().enumerate() {
        match locate_store(home, session) {
            Ok(root) => {
                let store = (root, index_name(&session.provider_id));
                let target = (session.session_id.as_str(), session.index_only);
                stores.entry(store).or_default().push((i, target));
            }
            Err(error) => results[i] = Some(error),
        }
    }
    if let Err(error) = check_clients() {
        for (i, _) in stores.into_values().flatten() {
            results[i] = Some(error.to_string());
        }
        return (results, cleaned);
    }
    for ((root, index), group) in stores {
        let targets: Vec<_> = group.iter().map(|(_, target)| *target).collect();
        let mut committed = false;
        match clean(&root, index, &targets, options, scope, &check_clients) {
            Ok(()) => committed = true,
            Err(error) => {
                // Retry sessions individually on failure, but skip retries for store-wide errors
                let mut error =
                    (group.len() == 1 || error.is::<StoreWide>()).then(|| error.to_string());
                for (i, target) in &group {
                    results[*i] = error.clone().or_else(|| {
                        match clean(&root, index, &[*target], options, scope, &check_clients) {
                            Ok(()) => {
                                committed = true;
                                None
                            }
                            Err(retry) => {
                                if retry.is::<StoreWide>() {
                                    error = Some(retry.to_string());
                                }
                                Some(retry.to_string())
                            }
                        }
                    });
                }
            }
        }
        if committed {
            cleaned.insert(root);
        }
    }
    (results, cleaned)
}

fn locate_store(home: &Path, session: &SessionRef) -> std::result::Result<PathBuf, String> {
    let (root, id) = locate(home, &session.provider_id, &session.source_path)?;
    if id != session.session_id {
        return Err("Antigravity conversation ID does not match its source".into());
    }
    let relative = root.strip_prefix(home).map_err(|error| error.to_string())?;
    checked_path(home, relative)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Antigravity session store no longer exists".into())
}

pub(super) fn index_name(provider_id: &str) -> &'static str {
    if provider_id == DESKTOP {
        DESKTOP_INDEX
    } else {
        CLI_INDEX
    }
}

struct Running {
    clients: [bool; 2],
    cc_switch: bool,
    magpie: bool,
}

impl Running {
    fn require_magpie_closed(self) -> Result<Self> {
        if self.magpie {
            return Err("Quit Magpie before cleaning its model probe sessions".into());
        }
        self.require_closed()
    }

    fn require_closed(self) -> Result<Self> {
        let quit = match self.clients {
            [false, false] => return Ok(self),
            [true, false] => "the Antigravity App and Antigravity CLI",
            [false, true] => "Antigravity IDE",
            [true, true] => "the Antigravity App, Antigravity CLI, and Antigravity IDE",
        };
        Err(format!("Quit {quit} before cleaning sessions").into())
    }
}

fn require_clients_closed() -> Result<()> {
    running_clients()?.require_closed().map(drop)
}

fn running_clients() -> Result<Running> {
    let mut system = System::new()?;
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    if system.processes().is_empty() {
        return Err("Could not verify that Google Antigravity clients are closed".into());
    }
    let mut running = Running {
        clients: [false; 2],
        cc_switch: false,
        magpie: false,
    };
    for process in system.processes().values() {
        let name = process
            .exe()
            .and_then(Path::file_name)
            .unwrap_or_else(|| process.name());
        let name = name.to_string_lossy();
        let path = process
            .exe()
            .map(|path| path.to_string_lossy())
            .unwrap_or_default();
        if let Some(client) = client(&name, &path) {
            running.clients[client as usize] = true;
        }
        running.cc_switch |= cc_switch::is_process_name(&name);
        running.magpie |= matches!(
            name.to_ascii_lowercase().trim_end_matches(".exe"),
            "magpie" | "magpie-bun"
        );
    }
    Ok(running)
}

// Antigravity IDE is out of scope, but it still blocks cleanup while it runs
fn client(name: &str, path: &str) -> Option<Client> {
    let name = name.to_ascii_lowercase();
    let name = name.trim_end_matches(".exe");
    let path = path.to_ascii_lowercase();
    let helper = (name.starts_with("language_server")
        || matches!(name, "agentapi" | "agy-desktop-helper"))
        && path.contains("antigravity");
    let ide = ["antigravity ide", "antigravity-ide", "antigravity-insiders"];
    if ide.contains(&name)
        || (name == "antigravity" || helper) && ide.iter().any(|ide| path.contains(ide))
    {
        Some(Client::Ide)
    } else if matches!(name, "antigravity" | "agy") || helper {
        Some(Client::AppOrCli)
    } else {
        None
    }
}

fn clean(
    root: &Path,
    index: &str,
    targets: &[(&str, bool)],
    options: CleanOptions,
    scope: Scope<'_>,
    check_clients: impl Fn() -> Result<()>,
) -> Result<()> {
    check_clients().map_err(store_wide)?;
    with_store_lock(root, |root| {
        let mut connection = open_index(root).map_err(store_wide)?;
        recover_locked(root, connection.as_ref()).map_err(store_wide)?;
        clean_locked(
            root,
            index,
            targets,
            options,
            scope,
            check_clients,
            connection.as_mut(),
        )
    })
}

/// Finish or undo interrupted cleanups and name the stores that still need attention
pub(super) fn recover_pending(home: &Path, provider_id: &str, recovery: &mut RecoveryStatus) {
    for root in roots(home, provider_id).unwrap_or_default() {
        let relative = root.strip_prefix(home).unwrap_or(&root);
        let Ok(Some(root)) = checked_path(home, relative) else {
            continue;
        };
        recover_store(home, &root, false, recovery);
    }
}

pub(super) fn sweep_usage_monitor(home: &Path, recovery: &mut RecoveryStatus) {
    let writer_running =
        || Ok(running_clients()?.clients.contains(&true) || usage_monitor::writer_running()?);
    match usage_monitor::sweep(home, writer_running) {
        Ok(CacheCleanup::Complete) => {}
        Ok(CacheCleanup::Pending) => recovery.usage_monitor_cleanup_pending = true,
        Err(error) => recovery.warnings.push(format!("Usage Monitor: {error}")),
    }
}

pub(super) fn recover_magpie_pending(home: &Path, recovery: &mut RecoveryStatus) -> bool {
    let Ok(Some(root)) = magpie_probes::root(home) else {
        return false;
    };
    recover_store(home, &root, true, recovery);
    true
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    local_done: bool,
    ids: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    deleted_databases: BTreeSet<PathBuf>,
    options: CleanOptions,
}

impl Receipt {
    fn read(path: &Path) -> Result<Self> {
        let receipt: Self = serde_json::from_reader(File::open(path)?)?;
        if !receipt.ids.iter().all(|id| valid_id(id)) {
            return Err("Invalid session ID in cleanup receipt".into());
        }
        Ok(receipt)
    }
}

fn receipts(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        if let Some(suffix) = name.to_str().and_then(|n| n.strip_prefix(RECEIPT_PREFIX)) {
            if suffix.len() == 12 && suffix.bytes().all(|b| b.is_ascii_alphanumeric()) {
                if !entry.file_type()?.is_file() {
                    return Err("Invalid cleanup receipt".into());
                }
                paths.push(entry.path());
            }
        }
    }
    Ok(paths)
}

fn recover_store(home: &Path, root: &Path, is_magpie: bool, recovery: &mut RecoveryStatus) {
    match recover_store_with(home, root, is_magpie, running_clients) {
        Ok(status) => {
            recovery.warnings.extend(status.warnings);
            recovery.cc_switch_verification_pending |= status.cc_switch_verification_pending;
            recovery.usage_monitor_cleanup_pending |= status.usage_monitor_cleanup_pending;
        }
        Err(error) => recovery
            .warnings
            .push(format!("{}: {error}", root.display())),
    }
}

fn recover_store_with(
    home: &Path,
    root: &Path,
    is_magpie: bool,
    clients: fn() -> Result<Running>,
) -> Result<RecoveryStatus> {
    let stages = recovery_dirs(root)?;
    let pending_receipts = receipts(root)?;
    let sync_pending = checked_file(root, SYNC_PENDING)?.is_some();
    if stages.is_empty() && pending_receipts.is_empty() && !sync_pending {
        return Ok(RecoveryStatus::default());
    }
    let running = clients()?;
    let closed = if is_magpie {
        running.require_magpie_closed()
    } else {
        running.require_closed()
    };
    let running = match closed {
        Ok(running) => running,
        Err(error) if stages.is_empty() => {
            // Pure cache and sync follow-ups can wait for the clients to close
            for path in &pending_receipts {
                let receipt = Receipt::read(path)?;
                if !receipt.local_done
                    || !receipt.options.usage_monitor
                    || receipt.options.app_cache
                    || receipt.options.cc_switch
                {
                    return Err(error);
                }
            }
            return Ok(RecoveryStatus {
                cc_switch_verification_pending: sync_pending,
                usage_monitor_cleanup_pending: !pending_receipts.is_empty(),
                ..Default::default()
            });
        }
        Err(error) => return Err(error),
    };
    with_store_lock(root, |locked| {
        if !recovery_dirs(locked)?.is_empty() {
            let connection = open_index(locked)?;
            recover_locked(locked, connection.as_ref())?;
            compact(connection.as_ref());
        }
        replay_extras(home, root, running.cc_switch, usage_monitor::writer_running)
    })
}

fn replay_extras(
    home: &Path,
    root: &Path,
    cc_running: bool,
    monitor_running: fn() -> Result<bool>,
) -> Result<RecoveryStatus> {
    let mut warnings = Vec::new();
    let mut monitor_pending = false;
    let pending = root.join(SYNC_PENDING);
    let mut verify = checked_file(root, SYNC_PENDING)?.is_some();
    let mut swept = false;
    let receipts = receipts(root)?;
    let provider = if root == home.join(".gemini/antigravity") {
        DESKTOP
    } else {
        CLI
    };
    for path in receipts {
        let mut receipt = Receipt::read(&path)?;
        // Recheck the store on replay in case the user has restored a backup
        let mut can_clean_targets = true;
        if !receipt.ids.is_empty()
            && (!receipt.local_done || receipt.options.cc_switch || receipt.options.usage_monitor)
        {
            match live_ids(home, root, provider, Some(&receipt.ids)) {
                Ok(live) => receipt
                    .ids
                    .retain(|id| !live.contains(&id.to_ascii_lowercase())),
                Err(error) => {
                    warnings.push(format!(
                        "{}: could not recheck deleted sessions: {error}",
                        root.display()
                    ));
                    can_clean_targets = false;
                }
            }
        }
        let group = Cleaned {
            root: root.to_owned(),
            ids: receipt.ids,
        };
        if can_clean_targets && !receipt.local_done {
            let errors = cleanup_extras::clean_local(home, &group, &receipt.deleted_databases);
            receipt.local_done = errors.is_empty();
            warnings.extend(errors);
        }
        if receipt.local_done {
            receipt.deleted_databases.clear();
        }
        if receipt.options.app_cache {
            let errors = cleanup_extras::clean_caches(home);
            receipt.options.app_cache = !errors.is_empty();
            warnings.extend(errors);
        }
        if receipt.options.usage_monitor && can_clean_targets {
            match usage_monitor::clean(home, &group, monitor_running) {
                Ok(CacheCleanup::Complete) => receipt.options.usage_monitor = false,
                Ok(CacheCleanup::Pending) => monitor_pending = true,
                Err(error) => warnings.push(format!("Usage Monitor: {error}")),
            }
        }
        if can_clean_targets && receipt.options.cc_switch {
            // An in-flight import may write a cursor after our transaction
            if cc_running && !verify {
                File::create_new(&pending)?.sync_all()?;
                verify = true;
            }
            let result = cc_switch::clean(home, &group, cc_running);
            swept = true;
            receipt.options.cc_switch = !result.targets_done;
            // An incomplete orphan sweep can retry without retaining selected IDs
            if !result.warnings.is_empty() && !verify {
                File::create_new(&pending)?.sync_all()?;
                verify = true;
            }
            if verify && !cc_running && result.warnings.is_empty() {
                fs::remove_file(&pending)?;
                verify = false;
            }
            warnings.extend(result.warnings);
        }
        if receipt.local_done
            && !receipt.options.app_cache
            && !receipt.options.cc_switch
            && !receipt.options.usage_monitor
        {
            fs::remove_file(path)?;
        } else {
            receipt.ids = if receipt.local_done
                && !receipt.options.cc_switch
                && !receipt.options.usage_monitor
            {
                BTreeSet::new()
            } else {
                group.ids
            };
            cleanup_extras::replace_file(&path, &serde_json::to_vec(&receipt)?)?;
        }
    }
    if verify && !swept {
        let group = Cleaned {
            root: root.to_owned(),
            ids: BTreeSet::new(),
        };
        let errors = cc_switch::clean(home, &group, cc_running).warnings;
        if errors.is_empty() && !cc_running {
            fs::remove_file(pending)?;
        }
        warnings.extend(errors);
    }
    Ok(RecoveryStatus {
        warnings,
        cc_switch_verification_pending: verify && cc_running,
        usage_monitor_cleanup_pending: monitor_pending,
    })
}

fn store_lock_file(root: &Path) -> Result<File> {
    let path = checked_file(root, LOCK)?.unwrap_or_else(|| root.join(LOCK));
    Ok(fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?)
}

// Keep a different store from staging files while its absence evidence is in use
pub(super) fn store_read_lock(root: &Path) -> Result<File> {
    let file = match checked_file(root, LOCK)? {
        Some(path) => File::open(path)?,
        None => store_lock_file(root)?,
    };
    file.try_lock_shared()?;
    Ok(file)
}

pub(super) fn with_store_lock<T>(
    root: &Path,
    action: impl FnOnce(&Path) -> Result<T>,
) -> Result<T> {
    let lock = || -> Result<(PathBuf, File)> {
        if fs::symlink_metadata(root)?.file_type().is_symlink() {
            return Err("Cannot clean an Antigravity store through a symlink".into());
        }
        let root = root.canonicalize()?;
        let lock = store_lock_file(&root)?;
        lock.try_lock()
            .map_err(|error| format!("Cannot lock session cleanup: {error}"))?;
        Ok((root, lock))
    };
    let (root, _lock) = lock().map_err(store_wide)?;
    action(&root)
}

fn clean_locked(
    root: &Path,
    index: &str,
    targets: &[(&str, bool)],
    options: CleanOptions,
    scope: Scope<'_>,
    check_clients: impl Fn() -> Result<()>,
    connection: Option<&mut Connection>,
) -> Result<()> {
    let mut connection = connection;
    let tx = connection
        .as_mut()
        .map(|conn| conn.transaction_with_behavior(TransactionBehavior::Immediate))
        .transpose()
        .map_err(store_wide)?;
    let selected: Vec<_> = targets.iter().map(|(id, _)| *id).collect();
    let related = related_ids(tx.as_deref(), &selected)?;
    let probes_only = matches!(scope, Scope::MagpieProbes);
    if probes_only {
        let db = tx
            .as_deref()
            .ok_or("Magpie session summary database is missing")?;
        magpie_probes::require_probes(root, db, &related)?;
    }
    let ids = if matches!(scope, Scope::HistoricalIndexes(_)) {
        selected.into_iter().map(str::to_owned).collect()
    } else {
        related.clone()
    };
    let presence = lock_presence(root, &ids)?;
    let only_indexes = matches!(scope, Scope::MatchingIndexes | Scope::HistoricalIndexes(_));
    let index_only_ids = if only_indexes {
        ids.clone()
    } else {
        targets
            .iter()
            .filter(|(_, index_only)| *index_only)
            .map(|(id, _)| (*id).to_owned())
            .collect()
    };
    let placeholders = session_files(root, &index_only_ids)?;
    let presence_dir = root.join("presence");
    let mut checked_files = Vec::new();
    for (i, path) in placeholders.iter().enumerate() {
        if path.parent() != Some(presence_dir.as_path()) {
            index_only::require_empty_path(path)?;
            checked_files.push(i);
        }
    }
    let mut files = if only_indexes {
        Vec::new()
    } else {
        let mut files = placeholders;
        let remaining = ids.difference(&index_only_ids).cloned().collect();
        files.extend(session_files(root, &remaining)?);
        files
    };
    let brain = root.join("brain");
    let attachment_ids = files
        .iter()
        .filter(|path| path.parent() == Some(brain.as_path()))
        .filter_map(|path| path.file_name()?.to_str().map(str::to_ascii_lowercase))
        .collect();
    let archives = usage_monitor::migration_archives(root, &attachment_ids)?;
    usage_monitor::protect_moved_attachments(root, &archives)?;
    let mut replacements = Vec::new();
    if let Some(path) = checked_file(root, index)? {
        let original = fs::read(&path)?;
        if let Some(filtered) = filtered_index(&original, &ids, index == DESKTOP_INDEX)? {
            replacements.push(Replacement {
                path,
                original,
                filtered,
            });
        }
    }
    let caches: [(&str, Filter); 3] = [
        (RECENTS, filtered_recents),
        (METADATA, filtered_metadata),
        (HISTORY, filtered_history),
    ];
    for (name, filter) in caches {
        if let Some(path) = checked_file(root, name)? {
            let original = fs::read(&path)?;
            if let Some(filtered) = filter(&original, &ids)? {
                replacements.push(Replacement {
                    path,
                    original,
                    filtered,
                });
            }
        }
    }
    // Probes have no sidecar state or external usage records
    if !probes_only {
        sidecar_changes(root, &ids, &mut files, &mut replacements)?;
    }
    if let Some(tx) = &tx {
        remove_index_rows(tx, &ids)?;
    }
    check_clients().map_err(store_wide)?;
    let stage = Stage::prepare(root, files, &replacements, tx.is_some())?;
    let receipt = Receipt {
        local_done: probes_only,
        ids,
        deleted_databases: if probes_only {
            BTreeSet::new()
        } else {
            stage.deleted_databases()
        },
        options,
    };
    let result = (move || -> Result<()> {
        stage.write_receipt(&receipt)?;
        stage.apply(root, &replacements)?;
        check_clients().map_err(store_wide)?;
        index_only::require_no_session_data(root, &index_only_ids)?;
        if !only_indexes {
            // An empty placeholder can gain data before it is moved to the staging directory
            for i in checked_files {
                index_only::require_empty_path(&stage.saved_file(i)).map_err(|error| {
                    format!(
                        "{} changed during cleanup: {error}",
                        stage.plan.files[i].path.display()
                    )
                })?;
            }
        }
        let _orphan_locks = match scope {
            Scope::HistoricalIndexes(home) => {
                index_only::lock_historical_sources(home, root, &related)?
            }
            _ => Vec::new(),
        };
        // Staging moved the source `.db`, so only an archive written during cleanup counts here
        usage_monitor::protect_new_archives(root, &attachment_ids, &archives)?;
        stage.commit(tx)
    })();
    drop(presence);

    let recovery = recover_locked(root, connection.as_deref());
    match (result, recovery) {
        (Ok(()), Ok(())) => {
            compact(connection.as_deref());
            Ok(())
        }
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(format!(
            "Cleanup committed, but temporary files could not be removed: {error}"
        )
        .into()),
        (Err(error), Err(recovery)) => Err(format!("{error}. Recovery failed: {recovery}").into()),
    }
}

fn open_index(root: &Path) -> Result<Option<Connection>> {
    let Some(path) = checked_file(root, INDEX_DB)? else {
        return Ok(None);
    };
    for filename in [
        "conversation_summaries.db-wal",
        "conversation_summaries.db-shm",
        "conversation_summaries.db-journal",
    ] {
        checked_file(root, filename)?;
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    conn.busy_timeout(Duration::from_secs(2))?;
    // Overwrite deleted rows so their text does not linger in free pages
    conn.pragma_update(None, "secure_delete", true)?;
    Ok(Some(conn))
}

pub(super) fn has_column(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
        [table, column],
        |row| row.get(0),
    )
}

fn related_ids(conn: Option<&Connection>, selected: &[&str]) -> Result<BTreeSet<String>> {
    let mut ids: BTreeSet<_> = selected.iter().map(|id| (*id).to_owned()).collect();
    let Some(conn) = conn else {
        return Ok(ids);
    };
    if !has_column(conn, "conversation_summaries", "nesting_depth")?
        || !has_column(conn, "conversation_summaries", "parent_conversation_id")?
    {
        return Ok(ids);
    }

    // Forks can have a parent too; only nested agent runs are part of the selected conversation
    let mut children = BTreeMap::<String, Vec<String>>::new();
    let mut statement = conn.prepare(
        "SELECT parent_conversation_id, conversation_id FROM conversation_summaries
         WHERE nesting_depth > 0 AND parent_conversation_id != ''",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (parent, child) = row?;
        if valid_id(&child) {
            children.entry(parent).or_default().push(child);
        }
    }
    let mut pending: Vec<_> = ids.iter().cloned().collect();
    while let Some(parent) = pending.pop() {
        for child in children.remove(&parent).unwrap_or_default() {
            if ids.insert(child.clone()) {
                pending.push(child);
            }
        }
    }
    Ok(ids)
}

fn remove_index_rows(conn: &Connection, ids: &BTreeSet<String>) -> Result<()> {
    // One statement per table, since the FTS table has no index on the ID
    let ids = serde_json::to_string(ids)?;
    for table in [
        "conversation_summaries",
        "conversations_fts",
        "conversation_parents",
    ] {
        let definition: Option<String> = conn
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(definition) = definition {
            let removed = conn.execute(
                &format!(
                    "DELETE FROM {table} WHERE conversation_id IN
                     (SELECT value FROM json_each(?1))"
                ),
                [&ids],
            )?;
            if removed > 0
                && table == "conversations_fts"
                && definition.to_ascii_lowercase().contains("using fts5")
            {
                // Rebuild the FTS index without enabling a persistent format change
                conn.execute(
                    "INSERT INTO conversations_fts(conversations_fts) VALUES ('rebuild')",
                    [],
                )?;
            }
        }
    }
    Ok(())
}

fn compact(connection: Option<&Connection>) {
    if let Some(conn) = connection {
        // Compact removed rows out of free pages; a reader can defer either step without undoing the deletion
        let _ = conn.execute_batch("VACUUM");
        let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    }
}

fn lock_presence(root: &Path, ids: &BTreeSet<String>) -> Result<Vec<File>> {
    let mut locks = Vec::new();
    for id in ids {
        if let Some(path) = checked_file(root, &format!("presence/{id}.lock"))? {
            let file = fs::OpenOptions::new().read(true).write(true).open(path)?;
            file.try_lock()
                .map_err(|error| format!("Cannot lock conversation {id}: {error}"))?;
            locks.push(file);
        }
    }
    Ok(locks)
}

pub(super) fn session_files(root: &Path, ids: &BTreeSet<String>) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for id in ids {
        for relative in session_artifacts(id) {
            if let Some(path) = checked_file(root, &relative)? {
                files.push(path);
            }
        }
        if let Some(path) = checked_path(root, &Path::new("brain").join(id))? {
            if !path.is_dir() {
                return Err(format!("Expected a session directory at {}", path.display()).into());
            }
            files.push(path);
        }
    }
    Ok(files)
}

fn session_artifacts(id: &str) -> [String; 7] {
    [
        format!("conversations/{id}.db"),
        format!("conversations/{id}.db-wal"),
        format!("conversations/{id}.db-shm"),
        format!("conversations/{id}.db-journal"),
        format!("conversations/{id}.pb"),
        format!("annotations/{id}.pbtxt"),
        format!("presence/{id}.lock"),
    ]
}

pub(super) fn conversation_db_id(path: &Path) -> Option<&str> {
    let id = path.file_stem()?.to_str()?;
    (path.parent()? == Path::new("conversations")
        && path.extension()?.to_str()? == "db"
        && valid_id(id))
    .then_some(id)
}

fn mentions(bytes: &[u8], ids: &BTreeSet<String>) -> bool {
    ids.iter().any(|id| {
        bytes
            .windows(id.len())
            .any(|window| window == id.as_bytes())
    })
}

// A cache in an unknown format blocks cleanup only when it names the conversation
fn unrecognized(bytes: &[u8], ids: &BTreeSet<String>, name: &str) -> Result<Option<Vec<u8>>> {
    if mentions(bytes, ids) {
        return Err(
            format!("A selected conversation is referenced in an unrecognized {name}").into(),
        );
    }
    Ok(None)
}

fn filtered_recents(bytes: &[u8], ids: &BTreeSet<String>) -> Result<Option<Vec<u8>>> {
    let Ok(mut value) = serde_json::from_slice::<BTreeMap<String, String>>(bytes) else {
        return unrecognized(bytes, ids, "recent conversation cache");
    };
    let count = value.len();
    value.retain(|_, id| !ids.contains(id));
    Ok((value.len() != count)
        .then(|| serde_json::to_vec_pretty(&value))
        .transpose()?)
}

fn filtered_metadata(bytes: &[u8], ids: &BTreeSet<String>) -> Result<Option<Vec<u8>>> {
    let mut value: serde_json::Value = serde_json::from_slice(bytes).unwrap_or_default();
    let Some(entries) = value
        .get_mut("conversations")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return unrecognized(bytes, ids, "conversation metadata cache");
    };
    let count = entries.len();
    entries.retain(|id, _| !ids.contains(id));
    Ok((entries.len() != count)
        .then(|| serde_json::to_vec_pretty(&value))
        .transpose()?)
}

// Drop the prompts that the Antigravity CLI recorded for the conversation and keep other lines byte for byte
fn filtered_history(bytes: &[u8], ids: &BTreeSet<String>) -> Result<Option<Vec<u8>>> {
    let mut result = Vec::with_capacity(bytes.len());
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        let row = serde_json::from_slice::<serde_json::Value>(line).unwrap_or_default();
        if !row["conversationId"]
            .as_str()
            .is_some_and(|id| ids.contains(id))
        {
            result.extend_from_slice(line);
        }
    }
    Ok((result.len() != bytes.len()).then_some(result))
}

#[derive(Clone, Copy, PartialEq)]
enum SidecarFile {
    Event,
    UsageCache,
    Log,
}

// Plugin sidecars keep files under sidecar_data/<plugin>/<panel>/
fn sidecar_file(relative: &Path) -> Option<SidecarFile> {
    let parts: Vec<&str> = relative
        .components()
        .map(|part| match part {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let [SIDECAR, _, _, folder, name] = parts[..] else {
        return None;
    };
    match (folder, name) {
        ("events", name) if name.ends_with(".json") => Some(SidecarFile::Event),
        // Accept cache replacements in recovery journals from earlier releases
        ("data", "trajectory_usage_cache.json") => Some(SidecarFile::UsageCache),
        ("logs", "sidecar.log") => Some(SidecarFile::Log),
        _ => None,
    }
}

fn entries(dir: &Path, folders: bool) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if !fs::symlink_metadata(dir).is_ok_and(|metadata| metadata.is_dir()) {
        return Ok(paths);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if if folders {
            kind.is_dir()
        } else {
            kind.is_file()
        } {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

fn sidecar_changes(
    root: &Path,
    ids: &BTreeSet<String>,
    files: &mut Vec<PathBuf>,
    replacements: &mut Vec<Replacement>,
) -> Result<()> {
    for plugin in entries(&root.join(SIDECAR), true)? {
        for panel in entries(&plugin, true)? {
            // Usage caches are handled after commit, once every session store has been checked
            for folder in ["events", "logs"] {
                for path in entries(&panel.join(folder), false)? {
                    let Some(kind) = sidecar_file(path.strip_prefix(root)?) else {
                        continue;
                    };
                    let original = fs::read(&path)?;
                    let filtered = match kind {
                        SidecarFile::Event => {
                            if event_conversation(&original).is_some_and(|id| ids.contains(&id)) {
                                files.push(path);
                            }
                            continue;
                        }
                        SidecarFile::UsageCache => continue,
                        SidecarFile::Log => filtered_lines(&original, ids),
                    };
                    if let Some(filtered) = filtered {
                        replacements.push(Replacement {
                            path,
                            original,
                            filtered,
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

// A sidecar event belongs to the conversation it was raised in; events from forks name the original as root
fn event_conversation(bytes: &[u8]) -> Option<String> {
    let event = serde_json::from_slice::<serde_json::Value>(bytes).ok()?;
    let metadata = &event["payload"]["conversationMetadata"]["metadata"];
    let nested = metadata["nestingDepth"].as_u64().unwrap_or(0) > 0;
    let forked = metadata["parentConversationId"]
        .as_str()
        .is_some_and(|parent| !parent.is_empty());
    if forked && !nested {
        return None;
    }
    metadata["rootConversationId"].as_str().map(str::to_owned)
}

fn filtered_lines(bytes: &[u8], ids: &BTreeSet<String>) -> Option<Vec<u8>> {
    let mut result = Vec::with_capacity(bytes.len());
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        if !mentions(line, ids) {
            result.extend_from_slice(line);
        }
    }
    (result.len() != bytes.len()).then_some(result)
}

// Filter summary entries while preserving unrelated protobuf fields byte for byte
fn filtered_index(bytes: &[u8], ids: &BTreeSet<String>, desktop: bool) -> Result<Option<Vec<u8>>> {
    let mut result = Vec::with_capacity(bytes.len());
    let invalid = || store_wide("Unrecognized Antigravity summary index format");
    let mut rest = bytes;
    while !rest.is_empty() {
        let (tag, body, raw) = next_field(&mut rest).ok_or_else(invalid)?;
        let key = summary_entry_id(tag, body, desktop).map_err(|()| invalid())?;
        if let Some(id) = key {
            if ids.contains(id) {
                continue;
            }
        } else if mentions(raw, ids) {
            return Err(
                "A selected conversation is referenced in an unsupported summary field".into(),
            );
        }
        result.extend_from_slice(raw);
    }
    Ok((result.len() != bytes.len()).then_some(result))
}

/// Decode the summary index without treating malformed entries as an empty store
pub(super) fn indexed_ids(bytes: &[u8], desktop: bool, orphan_sweep: bool) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let (tag, body, _) = next_field(&mut rest).ok_or("Malformed summary index")?;
        if let Some(id) =
            summary_entry_id(tag, body, desktop).map_err(|_| "Malformed summary entry")?
        {
            ids.push(id.to_owned());
        } else if orphan_sweep && tag & 7 == 2 {
            return Err("Unknown field in the summary index".into());
        }
    }
    Ok(ids)
}

fn summary_entry_id(tag: u64, body: &[u8], desktop: bool) -> std::result::Result<Option<&str>, ()> {
    // The Antigravity App also indexes some conversation IDs in a UUID-to-timestamp map
    match tag {
        10 => map_key(body, 18).map(Some).ok_or(()),
        18 if desktop => Ok(map_key(body, 16)),
        _ => Ok(None),
    }
}

fn map_key(mut entry: &[u8], value_tag: u64) -> Option<&str> {
    let mut key = None;
    let mut value = false;
    while !entry.is_empty() {
        let (tag, body, _) = next_field(&mut entry)?;
        if tag == 10 && key.is_none() {
            let id = std::str::from_utf8(body).ok()?;
            if !valid_id(id) {
                return None;
            }
            key = Some(id);
        } else if tag == value_tag && !value {
            value = true;
        } else {
            return None;
        }
    }
    key.filter(|_| value)
}

fn next_field<'a>(rest: &mut &'a [u8]) -> Option<(u64, &'a [u8], &'a [u8])> {
    let from = *rest;
    let tag = varint(rest)?;
    if tag >> 3 == 0 {
        return None;
    }
    let size = match tag & 7 {
        0 => {
            varint(rest)?;
            0
        }
        1 => 8,
        2 => usize::try_from(varint(rest)?).ok()?,
        5 => 4,
        _ => return None,
    };
    let body = rest.get(..size)?;
    *rest = rest.get(size..)?;
    Some((tag, body, &from[..from.len() - rest.len()]))
}

fn varint(rest: &mut &[u8]) -> Option<u64> {
    let mut value = 0;
    for shift in (0..64).step_by(7) {
        let (byte, tail) = rest.split_first()?;
        *rest = tail;
        if shift == 63 && *byte > 1 {
            return None;
        }
        value |= u64::from(byte & 0x7f) << shift;
        if *byte < 0x80 {
            return Some(value);
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Prepare,
    Active,
    Purge,
}

impl Phase {
    fn prefix(self) -> &'static str {
        match self {
            Self::Prepare => PREPARE_PREFIX,
            Self::Active => STAGE_PREFIX,
            Self::Purge => PURGE_PREFIX,
        }
    }
}

fn recovery_dirs(root: &Path) -> Result<Vec<(Phase, PathBuf)>> {
    let mut dirs = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        for phase in [Phase::Prepare, Phase::Active, Phase::Purge] {
            if let Some(suffix) = name.strip_prefix(phase.prefix()) {
                if (suffix.len() == 12 || phase == Phase::Active && suffix.len() == 6)
                    && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
                {
                    if !entry.file_type()?.is_dir() {
                        return Err(format!("Invalid cleanup directory: {name}").into());
                    }
                    dirs.push((phase, entry.path()));
                }
                break;
            }
        }
    }
    Ok(dirs)
}

fn recover_locked(root: &Path, connection: Option<&Connection>) -> Result<()> {
    let mut dirs = recovery_dirs(root)?;
    for (phase, dir) in &mut dirs {
        if *phase == Phase::Active {
            let stage = Stage::load(dir)?;
            if !stage.is_committed(connection)? {
                stage.restore(root)?;
            } else if let Some(receipt) = checked_file(dir, RECEIPT)? {
                let suffix = dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|n| n.strip_prefix(STAGE_PREFIX))
                    .ok_or("Invalid cleanup stage")?;
                fs::rename(receipt, root.join(format!("{RECEIPT_PREFIX}{suffix}")))?;
            }
            *dir = phase_path(root, dir, Phase::Active, Phase::Purge)?;
            *phase = Phase::Purge;
        }
    }
    if !dirs.is_empty() {
        if let Some(conn) = connection {
            conn.execute(&format!("DROP TABLE IF EXISTS {COMMIT_TABLE}"), [])?;
        }
        for (_, dir) in dirs {
            fs::remove_dir_all(&dir)
                .map_err(|error| format!("Cleanup files remain at {}: {error}", dir.display()))?;
        }
    }
    Ok(())
}

fn phase_path(root: &Path, path: &Path, from: Phase, to: Phase) -> Result<PathBuf> {
    let suffix = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix(from.prefix()))
        .ok_or("Invalid cleanup directory name")?;
    let destination = root.join(format!("{}{suffix}", to.prefix()));
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(format!(
            "Cleanup directory already exists: {}",
            destination.display()
        )
        .into());
    }
    fs::rename(path, &destination)?;
    Ok(destination)
}

struct Stage {
    dir: PathBuf,
    plan: CleanupPlan,
}

struct Replacement {
    path: PathBuf,
    original: Vec<u8>,
    filtered: Vec<u8>,
}

#[derive(Deserialize, Serialize)]
struct CleanupPlan {
    has_index: bool,
    files: Vec<PlannedFile>,
}

#[derive(Deserialize, Serialize)]
struct PlannedFile {
    path: PathBuf,
    replace: bool,
}

impl PlannedFile {
    fn validate(&self) -> bool {
        if let Some(kind) = sidecar_file(&self.path) {
            return self.replace == (kind != SidecarFile::Event);
        }
        if self.replace {
            return [DESKTOP_INDEX, CLI_INDEX, RECENTS, METADATA, HISTORY]
                .iter()
                .any(|name| self.path == Path::new(name));
        }
        let Some(id) = self.path.file_stem().and_then(|stem| stem.to_str()) else {
            return false;
        };
        if !valid_id(id) {
            return false;
        }
        self.path == Path::new("brain").join(id)
            || session_artifacts(id)
                .iter()
                .any(|name| self.path == Path::new(name))
    }
}

impl Stage {
    fn saved_file(&self, i: usize) -> PathBuf {
        self.dir.join(format!("saved-{i}"))
    }

    fn deleted_databases(&self) -> BTreeSet<PathBuf> {
        self.plan
            .files
            .iter()
            .filter(|file| !file.replace && conversation_db_id(&file.path).is_some())
            .map(|file| file.path.clone())
            .collect()
    }

    fn prepare(
        root: &Path,
        files: Vec<PathBuf>,
        replacements: &[Replacement],
        has_index: bool,
    ) -> Result<Self> {
        let temp = tempfile::Builder::new()
            .prefix(PREPARE_PREFIX)
            .rand_bytes(12)
            .tempdir_in(root)?;
        let mut plan = CleanupPlan {
            has_index,
            files: Vec::new(),
        };
        for path in files {
            plan.files.push(PlannedFile {
                path: path.strip_prefix(root)?.to_owned(),
                replace: false,
            });
        }
        for replacement in replacements {
            let next = temp.path().join(format!("next-{}", plan.files.len()));
            let mut file = File::create_new(next)?;
            file.write_all(&replacement.filtered)?;
            file.sync_all()?;
            plan.files.push(PlannedFile {
                path: replacement.path.strip_prefix(root)?.to_owned(),
                replace: true,
            });
        }
        let mut plan_file = File::create_new(temp.path().join(PLAN))?;
        serde_json::to_writer(&mut plan_file, &plan)?;
        plan_file.sync_all()?;
        drop(plan_file);

        // No live files move until the complete plan has been published
        let dir = phase_path(root, temp.path(), Phase::Prepare, Phase::Active)?;
        Ok(Self { dir, plan })
    }

    fn load(dir: &Path) -> Result<Self> {
        let file = checked_file(dir, PLAN)?
            .ok_or_else(|| format!("Cleanup recovery plan missing at {}", dir.display()))?;
        let plan: CleanupPlan = serde_json::from_reader(File::open(file)?)?;
        let mut paths = BTreeSet::new();
        if !plan
            .files
            .iter()
            .all(|file| file.validate() && paths.insert(&file.path))
        {
            return Err(format!("Invalid cleanup recovery plan at {}", dir.display()).into());
        }
        Ok(Self {
            dir: dir.to_owned(),
            plan,
        })
    }

    fn write_receipt(&self, receipt: &Receipt) -> Result<()> {
        let mut file = File::create_new(self.dir.join(RECEIPT))?;
        serde_json::to_writer(&mut file, receipt)?;
        file.sync_all()?;
        Ok(())
    }

    fn apply(&self, root: &Path, replacements: &[Replacement]) -> Result<()> {
        let mut replacements = replacements.iter();
        for (i, file) in self.plan.files.iter().enumerate() {
            let path = checked_path(root, &file.path)?
                .ok_or_else(|| format!("Session file disappeared: {}", file.path.display()))?;
            let backup = self.saved_file(i);
            fs::rename(&path, &backup)?;
            if file.replace {
                let replacement = replacements.next().ok_or("Missing index replacement")?;
                if fs::read(&backup)? != replacement.original {
                    return Err(format!(
                        "Session index changed during cleanup: {}",
                        path.display()
                    )
                    .into());
                }
                self.write_replacement(&path, &backup, &replacement.filtered)?;
            }
        }
        Ok(())
    }

    fn write_replacement(&self, path: &Path, original: &Path, bytes: &[u8]) -> Result<()> {
        let mut tmp = tempfile::NamedTempFile::new_in(&self.dir)?;
        tmp.as_file()
            .set_permissions(fs::metadata(original)?.permissions())?;
        tmp.write_all(bytes)?;
        tmp.as_file().sync_all()?;
        tmp.persist_noclobber(path)?;
        Ok(())
    }

    fn commit(&self, tx: Option<rusqlite::Transaction<'_>>) -> Result<()> {
        if let Some(tx) = tx {
            // Put the marker in the same transaction as the index changes
            tx.execute(
                &format!("CREATE TABLE IF NOT EXISTS {COMMIT_TABLE} (stage TEXT PRIMARY KEY)"),
                [],
            )?;
            tx.execute(
                &format!("INSERT INTO {COMMIT_TABLE} VALUES (?1)"),
                [self
                    .dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or("Invalid cleanup stage")?],
            )?;
            tx.commit()?;
        } else {
            File::create_new(self.dir.join("committed"))?.sync_all()?;
        }
        Ok(())
    }

    fn is_committed(&self, connection: Option<&Connection>) -> Result<bool> {
        if !self.plan.has_index {
            return Ok(checked_file(&self.dir, "committed")?.is_some());
        }
        let conn = connection.ok_or("Summary database missing during cleanup recovery")?;
        if !has_column(conn, COMMIT_TABLE, "stage")? {
            return Ok(false);
        }
        Ok(conn.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {COMMIT_TABLE} WHERE stage = ?1)"),
            [self
                .dir
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("Invalid cleanup stage")?],
            |row| row.get(0),
        )?)
    }

    fn restore(&self, root: &Path) -> Result<()> {
        for (i, file) in self.plan.files.iter().enumerate().rev() {
            let Some(backup) = checked_path(&self.dir, Path::new(&format!("saved-{i}")))? else {
                continue;
            };
            if let Some(current) = checked_path(root, &file.path)? {
                let next = checked_file(&self.dir, &format!("next-{i}"))?;
                let can_replace = file.replace
                    && current.is_file()
                    && next.is_some_and(|next| {
                        fs::read(next)
                            .ok()
                            .zip(fs::read(&current).ok())
                            .is_some_and(|(expected, bytes)| expected == bytes)
                    });
                if !can_replace {
                    return Err(format!(
                        "Newer data exists at {}; recovery files are at {}",
                        current.display(),
                        self.dir.display()
                    )
                    .into());
                }
            }
            fs::rename(backup, root.join(&file.path))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::antigravity_scan::store_index_ids;
    use super::super::antigravity_store::APP_DATA;
    use super::*;
    use std::cell::Cell;

    const TARGET: &str = "00000000-0000-4000-8000-000000000001";
    const FORK: &str = "00000000-0000-4000-8000-000000000002";
    const NESTED: &str = "00000000-0000-4000-8000-000000000003";

    fn summary_entry(id: &str, timestamp: bool) -> Vec<u8> {
        let mut entry = vec![if timestamp { 18 } else { 10 }, 40, 10, 36];
        entry.extend_from_slice(id.as_bytes());
        entry.extend(if timestamp { [16, 1] } else { [18, 0] });
        entry
    }

    fn replay_extras(home: &Path, root: &Path, cc_running: bool) -> Result<Vec<String>> {
        super::replay_extras(home, root, cc_running, || Ok(false)).map(|status| status.warnings)
    }

    fn clean(
        root: &Path,
        index: &str,
        ids: &[&str],
        options: CleanOptions,
        check_clients: impl Fn() -> Result<()>,
    ) -> Result<()> {
        let targets: Vec<_> = ids.iter().map(|id| (*id, false)).collect();
        super::clean(
            root,
            index,
            &targets,
            options,
            Scope::Selected,
            check_clients,
        )
    }

    fn store() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let conn = Connection::open(root.join(INDEX_DB)).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE conversation_summaries (
                conversation_id TEXT PRIMARY KEY, parent_conversation_id TEXT, nesting_depth INTEGER
             );
             CREATE VIRTUAL TABLE conversations_fts USING fts5(conversation_id UNINDEXED, body);
             CREATE TABLE conversation_parents (conversation_id TEXT PRIMARY KEY, parent_id TEXT);",
        )
        .unwrap();
        for (id, parent, depth) in [(TARGET, "", 0), (FORK, TARGET, 0), (NESTED, TARGET, 1)] {
            conn.execute(
                "INSERT INTO conversation_summaries VALUES (?1, ?2, ?3)",
                rusqlite::params![id, parent, depth],
            )
            .unwrap();
            conn.execute("INSERT INTO conversations_fts VALUES (?1, 'example')", [id])
                .unwrap();
            conn.execute(
                "INSERT INTO conversation_parents VALUES (?1, ?2)",
                [id, parent],
            )
            .unwrap();
            for folder in ["brain", "annotations", "conversations", "cache", "presence"] {
                fs::create_dir_all(root.join(folder)).unwrap();
            }
            fs::create_dir(root.join("brain").join(id)).unwrap();
            fs::write(
                root.join("brain").join(id).join("transcript.jsonl"),
                b"example",
            )
            .unwrap();
            for suffix in [".db", ".db-wal", ".db-shm", ".db-journal"] {
                fs::write(
                    root.join("conversations").join(format!("{id}{suffix}")),
                    b"example",
                )
                .unwrap();
            }
            fs::write(
                root.join("annotations").join(format!("{id}.pbtxt")),
                b"example",
            )
            .unwrap();
            fs::write(root.join("presence").join(format!("{id}.lock")), b"").unwrap();
        }
        fs::write(
            root.join(METADATA),
            serde_json::json!({"conversations": {
                TARGET: {"summary": {"ID": TARGET, "Title": "Target"}, "is_internal": false},
                FORK: {"summary": {"ID": FORK, "Title": "Fork"}, "is_internal": false},
            }})
            .to_string(),
        )
        .unwrap();
        let proto: Vec<_> = [TARGET, FORK, NESTED]
            .into_iter()
            .flat_map(|id| [summary_entry(id, false), summary_entry(id, true)].concat())
            .collect();
        fs::write(root.join(DESKTOP_INDEX), proto).unwrap();
        fs::write(
            root.join("cache/last_conversations.json"),
            format!(r#"{{"/example":"{TARGET}","/fork":"{FORK}"}}"#),
        )
        .unwrap();
        fs::write(root.join("user_settings.pb"), b"untouched").unwrap();
        fs::write(root.join(HISTORY), history(&[TARGET, FORK])).unwrap();
        temp
    }

    fn history(ids: &[&str]) -> String {
        let mut lines = String::from("{\"type\":\"command\",\"display\":\"/help\"}\n");
        for id in ids {
            lines += &format!("{{\"conversationId\":\"{id}\",\"display\":\"prompt\"}}\n");
        }
        lines
    }

    fn assert_rows(root: &Path, expected: &[&str]) {
        let conn = Connection::open(root.join(INDEX_DB)).unwrap();
        for table in [
            "conversation_summaries",
            "conversations_fts",
            "conversation_parents",
        ] {
            let mut statement = conn
                .prepare(&format!(
                    "SELECT conversation_id FROM {table} ORDER BY conversation_id"
                ))
                .unwrap();
            let rows: Vec<String> = statement
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            assert_eq!(rows, expected, "{table}");
        }
    }

    #[test]
    fn keeps_migrated_source_attachments_and_deletes_other_selected_sessions() {
        let home = tempfile::tempdir().unwrap();
        let cli = home.path().join(".gemini/antigravity-cli");
        let archives = home.path().join(
            ".gemini/antigravity/sidecar_data/usage-monitor/panel/data/session-migration-backups/archived-sources",
        );
        for (source, id) in [("cli", TARGET), ("desktop", FORK)] {
            let archive = archives.join(source);
            fs::create_dir_all(&archive).unwrap();
            fs::write(
                archive.join(format!("{id}.protected.json")),
                format!(r#"{{"cascadeId":"{id}","prunedAtMs":1795478400000}}"#),
            )
            .unwrap();
            let brain = cli.join("brain").join(id);
            fs::create_dir_all(&brain).unwrap();
            fs::write(brain.join("attachment.png"), b"attachment").unwrap();
        }
        let sessions = [TARGET, FORK].map(|id| SessionRef {
            provider_id: CLI.into(),
            session_id: id.into(),
            source_path: cli.join("conversations").join(format!("{id}.db")),
            index_only: false,
        });

        let (errors, cleaned) = delete_with(
            home.path(),
            &sessions,
            CleanOptions::default(),
            Scope::Selected,
            || Ok(()),
        );
        assert!(errors[0]
            .as_deref()
            .unwrap()
            .contains("Usage Monitor archived session"));
        assert!(errors[1].is_none());
        assert_eq!(cleaned, BTreeSet::from([cli.clone()]));
        assert!(cli
            .join("brain")
            .join(TARGET)
            .join("attachment.png")
            .is_file());
        assert!(!cli.join("brain").join(FORK).exists());
    }

    #[test]
    fn rolls_back_cleanup_if_a_migration_archive_appears_before_commit() {
        let home = tempfile::tempdir().unwrap();
        let base = home.path().join(".gemini");
        fs::create_dir(&base).unwrap();
        let desktop = base.join("antigravity");
        fs::rename(store().keep(), &desktop).unwrap();
        let brain = desktop.join("brain").join(TARGET);
        let archive = base.join(
            "antigravity-cli/sidecar_data/usage-monitor/panel/data/session-migration-backups/archived-sources/desktop",
        );

        let error = clean(
            &desktop,
            DESKTOP_INDEX,
            &[TARGET],
            CleanOptions::default(),
            || {
                if !brain.exists() {
                    fs::create_dir_all(&archive)?;
                    fs::write(
                        archive.join(format!("{TARGET}_2026-11-24T00-00-00-000Z.db")),
                        b"snapshot",
                    )?;
                }
                Ok(())
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("Usage Monitor archived session"));
        assert!(brain.join("transcript.jsonl").is_file());
        assert!(desktop
            .join("conversations")
            .join(format!("{TARGET}.db"))
            .is_file());
        assert_rows(&desktop, &[TARGET, FORK, NESTED]);
        assert!(recovery_dirs(&desktop).unwrap().is_empty());
    }

    #[test]
    fn deletes_a_session_whose_migration_failed_after_archiving() {
        let home = tempfile::tempdir().unwrap();
        let base = home.path().join(".gemini");
        fs::create_dir(&base).unwrap();
        let desktop = base.join("antigravity");
        fs::rename(store().keep(), &desktop).unwrap();
        // The archive stays when the target write fails, but the source keeps its `.db`
        let archive = base.join(
            "antigravity/sidecar_data/usage-monitor/panel/data/session-migration-backups/archived-sources/desktop",
        );
        fs::create_dir_all(&archive).unwrap();
        fs::write(
            archive.join(format!("{TARGET}_2026-11-24T00-00-00-000Z.db")),
            b"snapshot",
        )
        .unwrap();

        clean(
            &desktop,
            DESKTOP_INDEX,
            &[TARGET],
            CleanOptions::default(),
            || Ok(()),
        )
        .unwrap();
        assert!(!desktop.join("brain").join(TARGET).exists());
        assert!(!desktop
            .join("conversations")
            .join(format!("{TARGET}.db"))
            .exists());
    }

    #[test]
    fn cleanup_clears_an_index_only_session_and_preserves_its_fork() {
        let temp = store();
        let root = temp.path();
        fs::remove_dir_all(root.join("brain").join(TARGET)).unwrap();
        for suffix in [".db", ".db-wal", ".db-shm", ".db-journal"] {
            fs::remove_file(root.join("conversations").join(format!("{TARGET}{suffix}"))).unwrap();
        }
        clean(
            root,
            DESKTOP_INDEX,
            &[TARGET],
            CleanOptions::default(),
            || Ok(()),
        )
        .unwrap();
        assert_rows(root, &[FORK]);
        assert!(!root.join("brain").join(NESTED).exists());
        assert!(!root
            .join("annotations")
            .join(format!("{TARGET}.pbtxt"))
            .exists());
        assert!(!root
            .join("conversations")
            .join(format!("{NESTED}.db-wal"))
            .exists());
        assert!(root.join("brain").join(FORK).is_dir());
        assert_eq!(
            fs::read(root.join(DESKTOP_INDEX)).unwrap(),
            [summary_entry(FORK, false), summary_entry(FORK, true)].concat(),
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                &fs::read(root.join("cache/last_conversations.json")).unwrap(),
            )
            .unwrap(),
            serde_json::json!({"/fork": FORK}),
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(root.join(METADATA)).unwrap())
                .unwrap(),
            serde_json::json!({"conversations": {
                FORK: {"summary": {"ID": FORK, "Title": "Fork"}, "is_internal": false},
            }}),
        );
        assert_eq!(
            fs::read_to_string(root.join(HISTORY)).unwrap(),
            history(&[FORK])
        );
        for (id, kept) in [(TARGET, false), (NESTED, false), (FORK, true)] {
            let lock = root.join("presence").join(format!("{id}.lock"));
            assert_eq!(lock.exists(), kept, "{id}");
        }
        assert_eq!(
            fs::read(root.join("user_settings.pb")).unwrap(),
            b"untouched"
        );
        let index = fs::read(root.join(INDEX_DB)).unwrap();
        assert!(!index
            .windows(NESTED.len())
            .any(|bytes| bytes == NESTED.as_bytes()));
        assert!(root.join(LOCK).is_file());
        clean(
            root,
            DESKTOP_INDEX,
            &[TARGET],
            CleanOptions::default(),
            || Ok(()),
        )
        .unwrap();
    }

    #[test]
    fn client_restart_restores_files_and_rolls_back_indexes() {
        let temp = store();
        let root = temp.path();
        let proto = fs::read(root.join(DESKTOP_INDEX)).unwrap();
        let checks = Cell::new(0);
        let error = clean(
            root,
            DESKTOP_INDEX,
            &[TARGET],
            CleanOptions::default(),
            || {
                checks.set(checks.get() + 1);
                if checks.get() == 3 {
                    Err("client restarted".into())
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "client restarted");
        assert_rows(root, &[TARGET, FORK, NESTED]);
        assert!(root.join("brain").join(TARGET).is_dir());
        assert!(root
            .join("conversations")
            .join(format!("{NESTED}.db-journal"))
            .is_file());
        assert_eq!(fs::read(root.join(DESKTOP_INDEX)).unwrap(), proto);
        assert!(root
            .join("presence")
            .join(format!("{TARGET}.lock"))
            .is_file());
        assert!(fs::read_to_string(root.join(METADATA))
            .unwrap()
            .contains(TARGET));
        assert_eq!(
            fs::read_to_string(root.join(HISTORY)).unwrap(),
            history(&[TARGET, FORK])
        );
    }

    #[test]
    fn unknown_cache_formats_block_cleanup_only_when_they_name_the_session() {
        let ids = BTreeSet::from([TARGET.to_owned()]);
        for bytes in [&b""[..], b"null", br#"{"conversations":null}"#] {
            assert!(filtered_recents(bytes, &ids).unwrap().is_none());
            assert!(filtered_metadata(bytes, &ids).unwrap().is_none());
        }
        let unknown = format!(r#"["{TARGET}"]"#);
        assert!(filtered_recents(unknown.as_bytes(), &ids).is_err());
        assert!(filtered_metadata(unknown.as_bytes(), &ids).is_err());
    }

    #[test]
    fn unknown_index_format_leaves_the_conversation_intact() {
        let temp = store();
        let root = temp.path();
        fs::write(root.join(DESKTOP_INDEX), [10, 255]).unwrap();
        assert!(clean(
            root,
            DESKTOP_INDEX,
            &[TARGET],
            CleanOptions::default(),
            || Ok(())
        )
        .is_err());
        assert_rows(root, &[TARGET, FORK, NESTED]);
        assert!(root.join("brain").join(TARGET).is_dir());
    }

    #[test]
    fn removes_deleted_search_terms_from_disk() {
        let temp = store();
        let root = temp.path();
        let conn = Connection::open(root.join(INDEX_DB)).unwrap();
        conn.execute(
            "UPDATE conversations_fts SET body = 'deletedsearchfixture' WHERE conversation_id = ?1",
            [TARGET],
        )
        .unwrap();
        conn.execute(
            "UPDATE conversations_fts SET body = 'keptsearchfixture' WHERE conversation_id = ?1",
            [FORK],
        )
        .unwrap();
        drop(conn);
        let contains = |term: &str| {
            fs::read(root.join(INDEX_DB))
                .unwrap()
                .windows(term.len())
                .any(|bytes| bytes == term.as_bytes())
        };
        assert!(contains("deletedsearchfixture"));
        clean(
            root,
            DESKTOP_INDEX,
            &[TARGET],
            CleanOptions::default(),
            || Ok(()),
        )
        .unwrap();
        assert!(!contains("deletedsearchfixture"));
        assert!(contains("keptsearchfixture"));
        assert_rows(root, &[FORK]);
    }

    #[test]
    fn interruption_fixture_child() {
        let Some(root) = std::env::var_os("ANTI_POWER_INTERRUPTION_FIXTURE") else {
            return;
        };
        let root = PathBuf::from(root);
        assert!(root.join("interruption-fixture").is_file());
        let mut conn = open_index(&root).unwrap();
        let tx = conn.as_mut().map(|conn| {
            conn.transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap()
        });
        let ids = related_ids(tx.as_deref(), &[TARGET]).unwrap();
        let files = session_files(&root, &ids).unwrap();
        let index = root.join(DESKTOP_INDEX);
        let original = fs::read(&index).unwrap();
        let filtered = filtered_index(&original, &ids, true).unwrap().unwrap();
        let replacements = [Replacement {
            path: index,
            original,
            filtered,
        }];
        let stage = Stage::prepare(&root, files, &replacements, tx.is_some()).unwrap();
        if let Some(tx) = &tx {
            remove_index_rows(tx, &ids).unwrap();
        }
        stage
            .write_receipt(&Receipt {
                ids: ids.clone(),
                options: CleanOptions::default(),
                local_done: false,
                deleted_databases: stage.deleted_databases(),
            })
            .unwrap();
        stage.apply(&root, &replacements).unwrap();
        if std::env::var_os("ANTI_POWER_INTERRUPTION_COMMIT").is_some() {
            stage.commit(tx).unwrap();
        }
        std::process::exit(17);
    }

    #[test]
    fn recovers_before_and_after_commit_even_when_the_process_exits() {
        for with_index in [false, true] {
            for committed in [false, true] {
                let temp = store();
                let canonical_root = temp.path().canonicalize().unwrap();
                let root = canonical_root.as_path();
                fs::write(root.join("interruption-fixture"), b"").unwrap();
                if !with_index {
                    fs::remove_file(root.join(INDEX_DB)).unwrap();
                }
                let original_index = fs::read(root.join(DESKTOP_INDEX)).unwrap();
                let mut child = std::process::Command::new(std::env::current_exe().unwrap());
                child
                    .args([
                        "--exact",
                        "commands::antigravity_clean::tests::interruption_fixture_child",
                    ])
                    .env("ANTI_POWER_INTERRUPTION_FIXTURE", root)
                    .env_remove("ANTI_POWER_INTERRUPTION_COMMIT");
                if committed {
                    child.env("ANTI_POWER_INTERRUPTION_COMMIT", "1");
                }
                let output = child.output().unwrap();
                assert_eq!(output.status.code(), Some(17), "{output:?}");
                assert!(!recovery_dirs(root).unwrap().is_empty());
                assert!(!root.join("brain").join(TARGET).is_dir());

                let conn = open_index(root).unwrap();
                recover_locked(root, conn.as_ref()).unwrap();
                assert!(recovery_dirs(root).unwrap().is_empty());
                assert_eq!(receipts(root).unwrap().len(), usize::from(committed));
                assert_eq!(root.join("brain").join(TARGET).is_dir(), !committed);
                assert_eq!(
                    fs::read(root.join(DESKTOP_INDEX)).unwrap() == original_index,
                    !committed
                );
                assert!(root.join("brain").join(FORK).is_dir());
                if let Some(conn) = &conn {
                    assert!(!has_column(conn, COMMIT_TABLE, "stage").unwrap());
                    if committed {
                        assert_rows(root, &[FORK]);
                    } else {
                        assert_rows(root, &[TARGET, FORK, NESTED]);
                    }
                }
            }
        }
    }

    fn write_conversations(root: &Path, ids: &[&str]) {
        let dir = root.join("conversations");
        fs::create_dir_all(&dir).unwrap();
        for id in ids {
            fs::write(dir.join(format!("{id}.db")), "fixture").unwrap();
        }
    }

    fn sync_db(home: &Path) -> Connection {
        let dir = home.join(".cc-switch");
        fs::create_dir_all(&dir).unwrap();
        let db = Connection::open(dir.join("cc-switch.db")).unwrap();
        db.execute_batch("CREATE TABLE session_log_sync(file_path TEXT PRIMARY KEY)")
            .unwrap();
        db
    }

    fn insert_cursor(db: &Connection, root: &Path, id: &str) -> String {
        let cursor = format!("agy:{}/brain/{id}/transcript.jsonl", root.display());
        db.execute("INSERT INTO session_log_sync VALUES (?1)", [&cursor])
            .unwrap();
        cursor
    }

    fn sync_rows(db: &Connection) -> Vec<String> {
        db.prepare("SELECT file_path FROM session_log_sync ORDER BY 1")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    #[test]
    fn an_incomplete_orphan_sweep_retries_without_keeping_selected_ids() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app = home.join(".gemini/antigravity");
        let profile = home.join(".cc-switch/agy/work/antigravity-cli");
        let metadata = profile.join(METADATA);
        fs::create_dir_all(metadata.parent().unwrap()).unwrap();
        fs::write(&metadata, "unreadable").unwrap();
        let db = sync_db(home);
        let orphan = insert_cursor(&db, &profile, FORK);
        insert_cursor(&db, &app, TARGET);
        write_conversations(&app, &[TARGET]);
        clean(
            &app,
            DESKTOP_INDEX,
            &[TARGET],
            CleanOptions {
                cc_switch: true,
                usage_monitor: false,
                app_cache: false,
            },
            || Ok(()),
        )
        .unwrap();
        assert!(!replay_extras(home, &app, false).unwrap().is_empty());
        assert!(receipts(&app).unwrap().is_empty());
        assert!(fs::read(app.join(SYNC_PENDING)).unwrap().is_empty());
        assert_eq!(sync_rows(&db), [orphan.as_str()]);

        let offline = profile.with_file_name("offline");
        fs::rename(&profile, &offline).unwrap();
        fs::write(&profile, "not a store").unwrap();
        let warnings = replay_extras(home, &app, false).unwrap();
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("could not enumerate agy")));
        assert!(app.join(SYNC_PENDING).exists());
        assert_eq!(sync_rows(&db), [orphan]);

        fs::remove_file(&profile).unwrap();
        fs::rename(offline, &profile).unwrap();
        fs::remove_file(metadata).unwrap();
        assert!(replay_extras(home, &app, false).unwrap().is_empty());
        assert!(sync_rows(&db).is_empty());
        assert!(!app.join(SYNC_PENDING).exists());
    }

    #[test]
    fn torn_history_delays_only_cleanup_that_needs_its_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let cli = home.join(".gemini/antigravity-cli");
        write_conversations(&cli, &[TARGET]);
        let history_path = cli.join(HISTORY);
        let torn_row = |id| format!("{{\"conversationId\":\"{id}\",\"display\":");
        let unrelated = torn_row(FORK);
        fs::write(&history_path, history(&[TARGET]) + &unrelated).unwrap();

        let app_data = home.join(APP_DATA);
        let cache = app_data.join("Cache");
        fs::create_dir_all(&cache).unwrap();
        let state = app_data.join("app_storage.json");
        fs::write(
            &state,
            format!(r#"{{"antigravity-multi-conversation-layout-v3-{TARGET}":"open"}}"#),
        )
        .unwrap();
        let db = sync_db(home);
        let selected = insert_cursor(&db, &cli, TARGET);
        let orphan = insert_cursor(&db, &cli, FORK);
        clean(
            &cli,
            CLI_INDEX,
            &[TARGET],
            CleanOptions {
                cc_switch: true,
                usage_monitor: false,
                app_cache: true,
            },
            || Ok(()),
        )
        .unwrap();
        let remaining = history(&[]) + &unrelated;
        assert_eq!(fs::read_to_string(&history_path).unwrap(), remaining);

        fs::write(&history_path, history(&[]) + &torn_row(TARGET)).unwrap();
        let warnings = replay_extras(home, &cli, false).unwrap();
        assert!(warnings.iter().any(|warning| warning.contains(HISTORY)));
        assert!(!cache.exists());
        assert!(fs::read_to_string(&state).unwrap().contains(TARGET));
        assert_eq!(sync_rows(&db), [selected, orphan.clone()]);
        assert_eq!(receipts(&cli).unwrap().len(), 1);

        fs::write(&history_path, &remaining).unwrap();
        let warnings = replay_extras(home, &cli, false).unwrap();
        assert!(warnings.iter().any(|warning| warning.contains(HISTORY)));
        assert!(receipts(&cli).unwrap().is_empty());
        assert!(!fs::read_to_string(&state).unwrap().contains(TARGET));
        assert_eq!(sync_rows(&db), [orphan]);
        assert!(cli.join(SYNC_PENDING).exists());
        assert_eq!(fs::read_to_string(&history_path).unwrap(), remaining);

        fs::write(history_path, history(&[])).unwrap();
        assert!(replay_extras(home, &cli, false).unwrap().is_empty());
        assert!(sync_rows(&db).is_empty());
        assert!(!cli.join(SYNC_PENDING).exists());
    }

    #[test]
    fn replay_preserves_a_restored_index_only_session_in_the_selected_store() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app = home.join(".gemini/antigravity");
        write_conversations(&app, &[TARGET, FORK]);
        clean(
            &app,
            DESKTOP_INDEX,
            &[TARGET, FORK],
            CleanOptions {
                cc_switch: true,
                usage_monitor: false,
                app_cache: false,
            },
            || Ok(()),
        )
        .unwrap();

        let state = home.join(APP_DATA).join("app_storage.json");
        fs::create_dir_all(state.parent().unwrap()).unwrap();
        let layout = |id: &str| format!("antigravity-multi-conversation-layout-v3-{id}");
        fs::write(
            &state,
            serde_json::json!({layout(TARGET): "open", layout(FORK): "open"}).to_string(),
        )
        .unwrap();
        let db = sync_db(home);
        let restored = insert_cursor(&db, &app, TARGET);
        insert_cursor(&db, &app, FORK);
        // A partial restore must leave the receipt pending until its index can be read
        fs::write(app.join(DESKTOP_INDEX), [10, 255]).unwrap();
        let warnings = replay_extras(home, &app, false).unwrap();
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("could not recheck deleted sessions")));
        assert_eq!(receipts(&app).unwrap().len(), 1);
        assert!(fs::read_to_string(&state).unwrap().contains(FORK));
        fs::write(app.join(DESKTOP_INDEX), summary_entry(TARGET, false)).unwrap();
        assert!(replay_extras(home, &app, false).unwrap().is_empty());
        assert!(receipts(&app).unwrap().is_empty());
        let saved = fs::read_to_string(&state).unwrap();
        assert!(saved.contains(TARGET) && !saved.contains(FORK));
        assert_eq!(sync_rows(&db), [restored]);
    }

    #[test]
    fn replays_ledger_cleanup_and_verifies_late_cc_switch_writes() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app = home.join(".gemini/antigravity");
        let cli = home.join(".gemini/antigravity-cli");
        write_conversations(&app, &[TARGET]);
        write_conversations(&cli, &[TARGET]);
        let cache = home.join(APP_DATA).join("Cache");
        fs::create_dir_all(&cache).unwrap();
        let ledger = app.join("sidecar_data/anti-power-usage/panel/data/ledger.json");
        fs::create_dir_all(ledger.parent().unwrap()).unwrap();
        let saved = serde_json::json!({"version":1, "sessions":{
            TARGET: {"store":"app", "title":"private", "buckets":[{"day":"2026-11-19","model":"m","client":"app","steps":1,"cost":1.0}]},
            FORK: {"store":"app","parentId":TARGET,"buckets":[]}
        }, "deleted":[]});
        fs::write(&ledger, saved.to_string()).unwrap();
        let db = sync_db(home);
        let app_cursor = insert_cursor(&db, &app, TARGET);
        clean(
            &app,
            DESKTOP_INDEX,
            &[TARGET],
            CleanOptions::default(),
            || Ok(()),
        )
        .unwrap();
        assert!(replay_extras(home, &app, false).unwrap().is_empty());
        assert!(fs::read_to_string(&ledger).unwrap().contains(TARGET));
        assert!(cache.is_dir());
        assert_eq!(sync_rows(&db), [app_cursor]);
        let options = CleanOptions {
            cc_switch: true,
            usage_monitor: false,
            app_cache: true,
        };
        clean(&cli, CLI_INDEX, &[TARGET], options, || Ok(())).unwrap();
        insert_cursor(&db, &cli, TARGET);
        let deferred = super::replay_extras(home, &cli, true, || Ok(false)).unwrap();
        assert!(deferred.cc_switch_verification_pending && deferred.warnings.is_empty());
        assert!(sync_rows(&db).is_empty());
        assert!(!cache.exists());
        assert!(receipts(&cli).unwrap().is_empty());
        assert!(cli.join(SYNC_PENDING).is_file());
        insert_cursor(&db, &cli, TARGET);
        assert!(replay_extras(home, &cli, false).unwrap().is_empty());
        assert!(sync_rows(&db).is_empty());
        assert!(!cli.join(SYNC_PENDING).exists());
        let text = fs::read_to_string(&ledger).unwrap();
        assert!(!text.contains(TARGET) && !text.contains("private"));
        let final_ledger: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(final_ledger["deleted"][0]["steps"], 1);
    }

    #[test]
    fn ledger_waits_for_a_known_offline_copy_until_its_database_is_deleted() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app = home.join(".gemini/antigravity");
        let profile = home.join(".cc-switch/agy/work/antigravity-cli");
        write_conversations(&app, &[TARGET]);
        write_conversations(&profile, &[TARGET]);
        let offline = home.join("offline");
        fs::rename(&profile, &offline).unwrap();
        let ledger = app.join("sidecar_data/anti-power-usage/panel/data/ledger.json");
        fs::create_dir_all(ledger.parent().unwrap()).unwrap();
        let saved = serde_json::json!({"version":1, "sessions":{TARGET:{
            "store":"app", "knownCopies":["app", "cc-switch:work"],
            "missingCopies":{"app":10,"cc-switch:work":null},
            "buckets":[{"day":"2026-11-19","model":"m","client":"app","steps":1}]
        }}, "deleted":[]});
        fs::write(&ledger, saved.to_string()).unwrap();
        let options = CleanOptions::default();
        clean(&app, DESKTOP_INDEX, &[TARGET], options, || Ok(())).unwrap();
        assert!(replay_extras(home, &app, false).unwrap().is_empty());
        let read =
            || serde_json::from_slice::<serde_json::Value>(&fs::read(&ledger).unwrap()).unwrap();
        let saved = read();
        assert_eq!(
            saved["sessions"][TARGET]["knownCopies"],
            serde_json::json!(["cc-switch:work"])
        );
        assert_eq!(
            saved["sessions"][TARGET]["missingCopies"],
            serde_json::json!({"cc-switch:work":null})
        );
        assert_eq!(saved["deleted"], serde_json::json!([]));

        fs::create_dir_all(&profile).unwrap();
        fs::write(profile.join(CLI_INDEX), summary_entry(TARGET, false)).unwrap();
        clean(&profile, CLI_INDEX, &[TARGET], options, || Ok(())).unwrap();
        assert!(replay_extras(home, &profile, false).unwrap().is_empty());
        assert_eq!(read(), saved);

        fs::rename(offline.join("conversations"), profile.join("conversations")).unwrap();
        clean(&profile, CLI_INDEX, &[TARGET], options, || Ok(())).unwrap();
        assert!(replay_extras(home, &profile, false).unwrap().is_empty());
        let folded = read();
        assert!(folded["sessions"].get(TARGET).is_none());
        assert_eq!(folded["deleted"][0]["steps"], 1);
    }

    #[test]
    fn retries_cache_cleanup_without_rereading_the_session_indexes() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app = home.join(".gemini/antigravity");
        write_conversations(&app, &[TARGET]);
        clean(
            &app,
            DESKTOP_INDEX,
            &[TARGET],
            CleanOptions {
                cc_switch: false,
                usage_monitor: false,
                app_cache: true,
            },
            || Ok(()),
        )
        .unwrap();
        let cache = home.join(APP_DATA).join("Cache");
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        fs::write(&cache, "not a directory").unwrap();
        assert_eq!(replay_extras(home, &app, false).unwrap().len(), 1);
        let receipts = receipts(&app).unwrap();
        assert_eq!(receipts.len(), 1);
        assert!(!fs::read_to_string(&receipts[0]).unwrap().contains(TARGET));

        fs::remove_file(&cache).unwrap();
        fs::create_dir(&cache).unwrap();
        fs::write(app.join(DESKTOP_INDEX), [10, 255]).unwrap();
        assert!(replay_extras(home, &app, false).unwrap().is_empty());
        assert!(!cache.exists() && !receipts[0].exists());
    }

    #[test]
    fn recovery_preserves_files_written_after_the_interruption() {
        let temp = store();
        let root = temp.path();
        let index = root.join(DESKTOP_INDEX);
        let original = fs::read(&index).unwrap();
        let replacements = [Replacement {
            path: index.clone(),
            original: original.clone(),
            filtered: b"staged replacement".to_vec(),
        }];
        let stage = Stage::prepare(root, Vec::new(), &replacements, false).unwrap();
        stage.apply(root, &replacements).unwrap();
        fs::write(&index, b"newer data").unwrap();

        let error = recover_locked(root, None).unwrap_err();
        assert!(error.to_string().contains("Newer data exists"));
        assert_eq!(fs::read(&index).unwrap(), b"newer data");
        assert_eq!(fs::read(stage.dir.join("saved-0")).unwrap(), original);
    }

    #[test]
    fn cleanup_preserves_an_index_updated_after_it_was_read() {
        let temp = store();
        let root = temp.path();
        let path = root.join(DESKTOP_INDEX);
        let mut updated = fs::read(&path).unwrap();
        updated.extend(summary_entry("00000000-0000-4000-8000-000000000004", false));
        let checks = Cell::new(0);
        let error = clean(
            root,
            DESKTOP_INDEX,
            &[TARGET],
            CleanOptions::default(),
            || {
                checks.set(checks.get() + 1);
                if checks.get() == 2 {
                    fs::write(&path, &updated)?;
                }
                Ok(())
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("Session index changed"));
        assert_eq!(fs::read(path).unwrap(), updated);
        assert_rows(root, &[TARGET, FORK, NESTED]);
        assert!(root.join("brain").join(TARGET).is_dir());
        assert!(recovery_dirs(root).unwrap().is_empty());
    }

    #[test]
    fn rescan_keeps_same_id_in_the_other_store_until_selected() {
        use super::super::antigravity_scan::scan;
        use super::super::antigravity_store::CLI;

        let home = tempfile::tempdir().unwrap();
        let gemini = home.path().join(".gemini");
        fs::create_dir(&gemini).unwrap();
        let desktop = gemini.join("antigravity");
        let cli = gemini.join("antigravity-cli");
        fs::rename(store().keep(), &desktop).unwrap();
        fs::rename(store().keep(), &cli).unwrap();
        let cli_proto: Vec<_> = [TARGET, FORK, NESTED]
            .into_iter()
            .flat_map(|id| summary_entry(id, false))
            .collect();
        fs::write(cli.join(CLI_INDEX), cli_proto).unwrap();
        fs::remove_file(cli.join(DESKTOP_INDEX)).unwrap();
        let target_present = |provider| {
            scan(home.path(), provider)
                .unwrap()
                .iter()
                .any(|session| session.session_id == TARGET)
        };
        assert!(target_present(CLI) && target_present(DESKTOP));
        clean(&cli, CLI_INDEX, &[TARGET], CleanOptions::default(), || {
            Ok(())
        })
        .unwrap();
        assert!(!target_present(CLI) && target_present(DESKTOP));
        clean(
            &desktop,
            DESKTOP_INDEX,
            &[TARGET],
            CleanOptions::default(),
            || Ok(()),
        )
        .unwrap();
        assert!(!target_present(CLI) && !target_present(DESKTOP));
    }

    #[test]
    fn clears_matching_indexes_after_deletion_and_keeps_complete_copies() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app = home.join(".gemini/antigravity");
        let cli = home.join(".gemini/antigravity-cli");
        let profile = home.join(".cc-switch/agy/example/antigravity-cli");
        fs::create_dir_all(cli.join("conversations")).unwrap();
        fs::create_dir_all(profile.parent().unwrap()).unwrap();
        fs::rename(store().keep(), &app).unwrap();
        fs::rename(store().keep(), &profile).unwrap();
        fs::write(
            cli.join(CLI_INDEX),
            [summary_entry(TARGET, false), summary_entry(FORK, false)].concat(),
        )
        .unwrap();
        let session = |root: &Path, provider: &str, id: &str, index_only| SessionRef {
            provider_id: provider.into(),
            session_id: id.into(),
            source_path: root.join("conversations").join(format!("{id}.db")),
            index_only,
        };
        let selected = [session(&app, DESKTOP, TARGET, false)];
        let matches = [
            session(&cli, CLI, TARGET, true),
            session(&profile, CLI, TARGET, true),
            session(&cli, CLI, FORK, true),
        ];
        fs::write(&matches[0].source_path, []).unwrap();
        fs::create_dir_all(cli.join("brain").join(TARGET).join("empty")).unwrap();

        let (report, cleaned) = delete_with_indexes(
            home,
            &selected,
            &matches,
            &[],
            CleanOptions::default(),
            || Err("client running".into()),
        );
        assert_eq!(report.errors, [Some("client running".into())]);
        assert!(cleaned.is_empty() && report.recovery.warnings.is_empty());
        assert!(fs::read(cli.join(CLI_INDEX))
            .unwrap()
            .windows(TARGET.len())
            .any(|bytes| bytes == TARGET.as_bytes()));

        let (report, cleaned) = delete_with_indexes(
            home,
            &selected,
            &matches,
            &[],
            CleanOptions::default(),
            || Ok(()),
        );
        assert_eq!(report.errors, [None]);
        assert_eq!(cleaned, BTreeSet::from([app.clone(), cli.clone()]));
        assert_eq!(report.recovery.warnings.len(), 1);
        assert!(report.recovery.warnings[0].contains("Session data is present"));
        assert_rows(&app, &[FORK]);
        assert_eq!(
            fs::read(cli.join(CLI_INDEX)).unwrap(),
            summary_entry(FORK, false)
        );
        assert_rows(&profile, &[TARGET, FORK, NESTED]);
        assert_eq!(fs::read(&matches[1].source_path).unwrap(), b"example");
    }

    #[test]
    fn selected_index_only_placeholder_can_gain_data_during_cleanup() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        fs::create_dir(home.join(".gemini")).unwrap();
        let root = home.join(".gemini/antigravity");
        fs::rename(store().keep(), &root).unwrap();
        for path in session_files(&root, &BTreeSet::from([FORK.to_owned()])).unwrap() {
            if path.is_dir() {
                fs::remove_dir_all(path).unwrap();
            } else {
                fs::remove_file(path).unwrap();
            }
        }
        let targets = [(TARGET, false), (FORK, true)].map(|(id, index_only)| SessionRef {
            provider_id: DESKTOP.to_owned(),
            source_path: root.join("conversations").join(format!("{id}.db")),
            session_id: id.to_owned(),
            index_only,
        });
        let placeholder = &targets[1].source_path;
        fs::write(placeholder, []).unwrap();
        let checks = Cell::new(0);
        let (report, _) =
            delete_with_indexes(home, &targets, &[], &[], CleanOptions::default(), || {
                checks.set(checks.get() + 1);
                if checks.get() == 3 {
                    fs::write(placeholder, b"newly synced")?;
                }
                Ok(())
            });
        assert_eq!(report.errors[0], None);
        assert!(report.errors[1]
            .as_deref()
            .unwrap()
            .contains("Session data is present"));
        assert_eq!(fs::read(placeholder).unwrap(), b"newly synced");
        assert_rows(&root, &[FORK]);

        fs::write(placeholder, []).unwrap();
        let (report, _) = delete_with_indexes(
            home,
            &targets[1..],
            &[],
            &[],
            CleanOptions::default(),
            || Ok(()),
        );
        assert_eq!(report.errors, [None]);
        assert!(!placeholder.exists());
        assert_rows(&root, &[]);
    }

    #[test]
    fn matching_index_cleanup_rolls_back_if_a_copy_appears() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("conversations")).unwrap();
        let proto = summary_entry(TARGET, false);
        fs::write(root.join(CLI_INDEX), &proto).unwrap();
        let conversation = root.join("conversations").join(format!("{TARGET}.db"));
        let checks = Cell::new(0);

        let error = super::clean(
            root,
            CLI_INDEX,
            &[(TARGET, true)],
            CleanOptions::default(),
            Scope::MatchingIndexes,
            || {
                checks.set(checks.get() + 1);
                if checks.get() == 3 {
                    fs::write(&conversation, b"newly synced")?;
                }
                Ok(())
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("Session data is present"));
        assert_eq!(fs::read(root.join(CLI_INDEX)).unwrap(), proto);
        assert_eq!(fs::read(conversation).unwrap(), b"newly synced");
    }

    #[test]
    fn historical_cleanup_only_clears_approved_unbacked_indexes_after_a_successful_deletion() {
        const ORPHAN: &str = "00000000-0000-4000-8000-000000000004";
        const ARCHIVED: &str = "00000000-0000-4000-8000-000000000005";
        const LATER: &str = "00000000-0000-4000-8000-000000000006";
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app = home.join(".gemini/antigravity");
        let cli = home.join(".gemini/antigravity-cli");
        let profile = home.join(".cc-switch/agy/example/antigravity-cli");
        fs::create_dir_all(cli.join("cache")).unwrap();
        fs::create_dir_all(profile.join("brain").join(NESTED)).unwrap();
        fs::rename(store().keep(), &app).unwrap();
        let archive = app.join(
            "sidecar_data/usage-monitor/panel/data/session-migration-backups/archived-sources/cli",
        );
        fs::create_dir_all(&archive).unwrap();
        fs::write(
            archive.join(format!("{ARCHIVED}_2026-11-24T00-00-00-000Z.db")),
            b"archive",
        )
        .unwrap();
        let write_index = |ids: &[&str]| {
            let bytes: Vec<_> = ids.iter().flat_map(|id| summary_entry(id, false)).collect();
            fs::write(cli.join(CLI_INDEX), bytes).unwrap();
        };
        write_index(&[TARGET, NESTED, FORK, ORPHAN, ARCHIVED]);
        let selected = [SessionRef {
            provider_id: DESKTOP.into(),
            source_path: app.join("conversations").join(format!("{TARGET}.db")),
            session_id: TARGET.into(),
            index_only: false,
        }];

        let preview = index_only::preview(home, &[TARGET.into()]).unwrap();
        assert_eq!(preview.len(), 1);
        assert_eq!(preview[0].session_id, ORPHAN);
        write_index(&[TARGET, NESTED, FORK, ORPHAN, ARCHIVED, LATER]);
        let (report, _) = delete_with_indexes(
            home,
            &selected,
            &[],
            &preview,
            CleanOptions::default(),
            || Err("client running".into()),
        );
        assert!(report.recovery.warnings.is_empty());
        assert_eq!(report.orphan_indexes_deleted, 0);
        assert_eq!(
            index_only::preview(home, &[TARGET.into()]).unwrap().len(),
            2
        );

        let (report, _) = delete_with_indexes(
            home,
            &selected,
            &[],
            &preview,
            CleanOptions::default(),
            || Ok(()),
        );
        assert_eq!(report.errors, [None]);
        assert!(report.recovery.warnings.is_empty());
        assert_eq!(report.orphan_indexes_deleted, 1);
        let remaining = store_index_ids(&cli, CLI).unwrap();
        assert_eq!(
            remaining,
            [TARGET, NESTED, FORK, ARCHIVED, LATER]
                .map(str::to_owned)
                .into()
        );
        assert_rows(&app, &[FORK]);
    }

    #[test]
    fn historical_cleanup_rechecks_other_stores_before_committing() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app = home.join(".gemini/antigravity");
        let cli = home.join(".gemini/antigravity-cli");
        fs::create_dir_all(app.join("conversations")).unwrap();
        fs::create_dir_all(&cli).unwrap();
        fs::write(cli.join(CLI_INDEX), summary_entry(TARGET, false)).unwrap();
        assert_eq!(index_only::preview(home, &[]).unwrap().len(), 1);
        let checks = Cell::new(0);
        let conversation = app.join("conversations").join(format!("{TARGET}.db"));
        let error = super::clean(
            &cli,
            CLI_INDEX,
            &[(TARGET, true)],
            CleanOptions::default(),
            Scope::HistoricalIndexes(home),
            || {
                checks.set(checks.get() + 1);
                if checks.get() == 3 {
                    fs::write(&conversation, b"newly synced")?;
                }
                Ok(())
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("Session data"));
        assert_eq!(
            fs::read(cli.join(CLI_INDEX)).unwrap(),
            summary_entry(TARGET, false)
        );
        assert_eq!(fs::read(conversation).unwrap(), b"newly synced");

        fs::create_dir(app.join(format!("{STAGE_PREFIX}interrupted"))).unwrap();
        assert!(index_only::preview(home, &[])
            .unwrap_err()
            .to_string()
            .contains("cleanup is in progress"));
    }

    #[test]
    fn cleans_several_sessions_in_one_pass() {
        let temp = store();
        let root = temp.path();
        clean(
            root,
            DESKTOP_INDEX,
            &[TARGET, FORK],
            CleanOptions::default(),
            || Ok(()),
        )
        .unwrap();
        assert_rows(root, &[]);
        assert!(fs::read(root.join(DESKTOP_INDEX)).unwrap().is_empty());
        assert_eq!(
            fs::read_to_string(root.join(HISTORY)).unwrap(),
            history(&[])
        );
        assert!(recovery_dirs(root).unwrap().is_empty());
        for id in [TARGET, FORK, NESTED] {
            assert!(!root.join("brain").join(id).exists());
        }
    }

    #[test]
    fn a_blocked_session_does_not_hold_back_the_rest_of_its_batch() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        fs::create_dir(home.join(".gemini")).unwrap();
        let root = home.join(".gemini/antigravity");
        fs::rename(store().keep(), &root).unwrap();
        fs::remove_dir_all(root.join("brain").join(FORK)).unwrap();
        fs::write(root.join("brain").join(FORK), b"unexpected").unwrap();
        let session = |id: &str| SessionRef {
            provider_id: DESKTOP.to_owned(),
            source_path: root.join("conversations").join(format!("{id}.db")),
            session_id: id.to_owned(),
            index_only: false,
        };

        let sessions = [session(TARGET), session(FORK), session("invalid")];
        let (results, _) = delete_with(
            home,
            &sessions,
            CleanOptions::default(),
            Scope::Selected,
            || Ok(()),
        );
        assert_eq!(results[0], None);
        assert!(results[1].as_deref().unwrap().contains("session directory"));
        assert!(results[2].is_some());
        assert_rows(&root, &[FORK]);

        let (results, _) = delete_with(
            home,
            &sessions[1..2],
            CleanOptions::default(),
            Scope::Selected,
            || Err("client running".into()),
        );
        assert_eq!(results, [Some("client running".to_owned())]);
    }

    #[test]
    fn a_store_wide_failure_is_not_retried_for_each_session() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        fs::create_dir(home.join(".gemini")).unwrap();
        let root = home.join(".gemini/antigravity");
        fs::rename(store().keep(), &root).unwrap();
        fs::write(root.join(DESKTOP_INDEX), [10, 255]).unwrap();
        let sessions = [TARGET, FORK].map(|id| SessionRef {
            provider_id: DESKTOP.to_owned(),
            source_path: root.join("conversations").join(format!("{id}.db")),
            session_id: id.to_owned(),
            index_only: false,
        });

        let checks = Cell::new(0);
        let (results, cleaned) = delete_with(
            home,
            &sessions,
            CleanOptions::default(),
            Scope::Selected,
            || {
                checks.set(checks.get() + 1);
                Ok(())
            },
        );
        assert!(cleaned.is_empty());
        for error in results {
            assert!(error.unwrap().contains("summary index format"));
        }
        // One check for the batch and one for the only pass over the store
        assert_eq!(checks.get(), 2);
        assert_rows(&root, &[TARGET, FORK, NESTED]);
    }

    fn sidecar_event(root: &str, parent: &str, depth: u64) -> String {
        serde_json::json!({"payload": {"conversationMetadata": {"metadata": {
            "rootConversationId": root,
            "parentConversationId": parent,
            "nestingDepth": depth,
        }}}})
        .to_string()
    }

    #[test]
    fn cleans_plugin_sidecar_data_for_the_session() {
        let temp = store();
        let root = temp.path();
        let panel = root.join(SIDECAR).join("usage-monitor/panel");
        for folder in ["events", "data", "logs"] {
            fs::create_dir_all(panel.join(folder)).unwrap();
        }
        let events = panel.join("events");
        fs::write(events.join("target.json"), sidecar_event(TARGET, "", 0)).unwrap();
        fs::write(events.join("nested.json"), sidecar_event(TARGET, TARGET, 1)).unwrap();
        fs::write(events.join("fork.json"), sidecar_event(TARGET, TARGET, 0)).unwrap();
        fs::write(events.join("other.json"), sidecar_event(FORK, "", 0)).unwrap();
        fs::write(events.join("unknown.json"), format!("not json {TARGET}")).unwrap();
        let cache = panel.join("data/trajectory_usage_cache.json");
        fs::write(
            &cache,
            format!(r#"{{"{TARGET}":{{"title":"Target"}},"{FORK}":{{}}}}"#),
        )
        .unwrap();
        let log = panel.join("logs/sidecar.log");
        fs::write(
            &log,
            format!("start\nloading {TARGET}\nloading {NESTED}\nloading {FORK}\n"),
        )
        .unwrap();

        clean(
            root,
            DESKTOP_INDEX,
            &[TARGET],
            CleanOptions::default(),
            || Ok(()),
        )
        .unwrap();
        let mut kept: Vec<_> = fs::read_dir(&events)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        kept.sort();
        assert_eq!(kept, ["fork.json", "other.json", "unknown.json"]);
        assert_eq!(
            fs::read_to_string(cache).unwrap(),
            format!(r#"{{"{TARGET}":{{"title":"Target"}},"{FORK}":{{}}}}"#)
        );
        assert_eq!(
            fs::read_to_string(log).unwrap(),
            format!("start\nloading {FORK}\n")
        );
        assert!(recovery_dirs(root).unwrap().is_empty());
    }

    #[test]
    fn optional_monitor_cleanup_sweeps_orphans_and_keeps_other_profiles_and_accounting() {
        use serde_json::{json, Value};
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let app = home.join(".gemini/antigravity");
        let cli = home.join(".gemini/antigravity-cli");
        let profile = home.join(".cc-switch/agy/work/antigravity-cli");
        write_conversations(&cli, &[TARGET]);
        write_conversations(&profile, &[FORK]);
        let data = app.join("sidecar_data/usage-monitor/panel/data");
        fs::create_dir_all(&data).unwrap();
        let cache = data.join("trajectory_usage_cache.json");
        let session = json!({"title":"original title","diskMtimeMs":1,"appSource":"cli",
            "steps":[{"modelKey":"M", "cacheReadTokens":45, "inputTokens":7,"turnPrompt":"original prompt"}]});
        let original = json!({TARGET:session, FORK:session, NESTED:session});
        fs::write(&cache, serde_json::to_vec(&original).unwrap()).unwrap();
        let quota = data.join("cli-status-snapshot.json");
        fs::write(&quota, b"unchanged quota snapshot").unwrap();

        clean(
            &cli,
            CLI_INDEX,
            &[TARGET],
            CleanOptions {
                usage_monitor: true,
                ..Default::default()
            },
            || Ok(()),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&cache).unwrap()).unwrap(),
            original
        );

        // An unreadable store postpones the sweep; refresh replays the same receipt
        fs::write(
            profile.join("conversation_summaries.db"),
            b"unreadable database",
        )
        .unwrap();
        assert!(!replay_extras(home, &cli, false).unwrap().is_empty());
        assert!(!receipts(&cli).unwrap().is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&cache).unwrap()).unwrap(),
            original
        );
        fs::remove_file(profile.join("conversation_summaries.db")).unwrap();
        let busy = super::replay_extras(home, &cli, false, || Ok(true)).unwrap();
        assert!(busy.usage_monitor_cleanup_pending && busy.warnings.is_empty());
        assert!(!cli
            .join("conversations")
            .join(format!("{TARGET}.db"))
            .exists());
        assert!(!receipts(&cli).unwrap().is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&cache).unwrap()).unwrap(),
            original
        );
        let warnings = replay_extras(home, &cli, false).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(receipts(&cli).unwrap().is_empty());
        let after: Value = serde_json::from_slice(&fs::read(&cache).unwrap()).unwrap();
        assert_eq!(after[FORK], session);
        for id in [TARGET, NESTED] {
            assert_ne!(after[id]["title"], session["title"]);
            assert_eq!(
                after[id]["steps"][0],
                json!({"modelKey":"M","cacheReadTokens":45,"inputTokens":7})
            );
        }
        assert_eq!(fs::read(quota).unwrap(), b"unchanged quota snapshot");
    }

    #[test]
    fn monitor_can_flush_new_usage_after_deletion_before_refresh() {
        use serde_json::{json, Value};
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let cli = home.join(".gemini/antigravity-cli");
        write_conversations(&cli, &[TARGET]);
        clean(
            &cli,
            CLI_INDEX,
            &[TARGET],
            CleanOptions {
                usage_monitor: true,
                ..Default::default()
            },
            || Ok(()),
        )
        .unwrap();

        // The standalone writer has not created the cache file yet
        let busy = super::replay_extras(home, &cli, false, || Ok(true)).unwrap();
        assert!(busy.usage_monitor_cleanup_pending && busy.warnings.is_empty());
        assert_eq!(receipts(&cli).unwrap().len(), 1);
        let reopened = recover_store_with(home, &cli, false, || {
            Ok(Running {
                clients: [true, false],
                cc_switch: false,
                magpie: false,
            })
        })
        .unwrap();
        assert!(reopened.usage_monitor_cleanup_pending && reopened.warnings.is_empty());

        let data = cli.join("sidecar_data/usage-monitor/panel/data");
        fs::create_dir_all(&data).unwrap();
        let cache = data.join("trajectory_usage_cache.json");
        let step = json!({"modelKey":"M", "inputTokens":70, "outputTokens":12, "cost":0.05});
        let mut original = json!({TARGET:{"title":"private title","steps":[step]}});
        original[TARGET]["steps"][0]["turnPrompt"] = json!("private prompt");
        fs::write(&cache, serde_json::to_vec(&original).unwrap()).unwrap();
        let failed =
            super::replay_extras(home, &cli, false, || Err("process check failed".into())).unwrap();
        assert!(!failed.usage_monitor_cleanup_pending && !failed.warnings.is_empty());
        assert_eq!(receipts(&cli).unwrap().len(), 1);
        let done = super::replay_extras(home, &cli, false, || Ok(false)).unwrap();
        assert!(!done.usage_monitor_cleanup_pending && done.warnings.is_empty());
        assert!(receipts(&cli).unwrap().is_empty());
        let after: Value = serde_json::from_slice(&fs::read(cache).unwrap()).unwrap();
        assert_ne!(after[TARGET]["title"], original[TARGET]["title"]);
        assert_eq!(after[TARGET]["steps"][0], step);
    }

    #[test]
    fn recovery_plans_accept_only_known_sidecar_files() {
        let planned = |path: &str, replace| {
            PlannedFile {
                path: PathBuf::from(path),
                replace,
            }
            .validate()
        };
        assert!(planned("sidecar_data/swarm/board/events/1.json", false));
        assert!(planned(
            "sidecar_data/a/b/data/trajectory_usage_cache.json",
            true
        ));
        assert!(planned("sidecar_data/a/b/logs/sidecar.log", true));
        assert!(!planned("sidecar_data/a/b/events/1.json", true));
        assert!(!planned("sidecar_data/a/b/logs/sidecar.log", false));
        assert!(!planned("sidecar_data/a/../events/1.json", false));
        assert!(!planned("sidecar_data/a/b/data/settings.json", true));
    }

    #[test]
    fn sync_marker_alone_is_a_notice_while_clients_are_open() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let root = home.join(".gemini/antigravity");
        fs::create_dir_all(&root).unwrap();
        let running = || {
            Ok(Running {
                clients: [true, false],
                cc_switch: false,
                magpie: false,
            })
        };
        assert!(
            !recover_store_with(home, &root, false, running)
                .unwrap()
                .cc_switch_verification_pending
        );
        fs::write(root.join(SYNC_PENDING), []).unwrap();
        let deferred = recover_store_with(home, &root, false, running).unwrap();
        assert!(deferred.cc_switch_verification_pending && deferred.warnings.is_empty());
        assert!(root.join(SYNC_PENDING).is_file());
        fs::write(root.join(format!("{RECEIPT_PREFIX}123456789012")), []).unwrap();
        assert!(recover_store_with(home, &root, false, running).is_err());
    }

    #[test]
    fn cc_switch_does_not_block_cleanup() {
        let running = |clients, cc_switch| {
            Running {
                clients,
                cc_switch,
                magpie: false,
            }
            .require_closed()
        };
        assert!(running([false, false], true).is_ok_and(|running| running.cc_switch));
        assert!(running([false, false], false).is_ok_and(|running| !running.cc_switch));
        for clients in [[true, false], [false, true], [true, true]] {
            let error = running(clients, true).err().unwrap().to_string();
            assert!(error.starts_with("Quit ") && !error.contains("CC Switch"));
        }
    }

    #[test]
    fn names_the_client_that_blocks_cleanup() {
        let app = "/Applications/Antigravity.app/Contents";
        let ide = "/Applications/Antigravity IDE.app/Contents";
        let cli = "/opt/homebrew/Caskroom/antigravity-cli/1.5.12/antigravity";
        for (name, path, expected) in [
            (
                "Antigravity",
                format!("{app}/MacOS/Antigravity"),
                Some(Client::AppOrCli),
            ),
            ("antigravity", cli.to_owned(), Some(Client::AppOrCli)),
            ("agy.exe", String::new(), Some(Client::AppOrCli)),
            (
                "language_server",
                format!("{app}/Resources/bin/language_server"),
                Some(Client::AppOrCli),
            ),
            (
                "language_server",
                format!("{ide}/Resources/bin/language_server"),
                Some(Client::Ide),
            ),
            (
                "Antigravity IDE",
                format!("{ide}/MacOS/Antigravity IDE"),
                Some(Client::Ide),
            ),
            (
                "Antigravity",
                format!("{ide}/MacOS/Antigravity"),
                Some(Client::Ide),
            ),
            (
                "language_server",
                "/usr/local/bin/language_server".to_owned(),
                None,
            ),
            ("Code Helper", String::new(), None),
        ] {
            assert_eq!(client(name, &path), expected, "{name} at {path}");
        }
    }

    #[test]
    fn lists_and_cleans_a_record_that_only_the_summary_index_holds() {
        use super::super::antigravity_scan::scan;
        use super::super::antigravity_store::CLI;

        let home = tempfile::tempdir().unwrap();
        for (provider, relative, index, timestamp) in [
            (CLI, ".gemini/antigravity-cli", CLI_INDEX, false),
            (DESKTOP, ".gemini/antigravity", DESKTOP_INDEX, true),
        ] {
            let root = home.path().join(relative);
            fs::create_dir_all(&root).unwrap();
            fs::write(root.join(index), summary_entry(TARGET, timestamp)).unwrap();
            let rows = scan(home.path(), provider).unwrap();
            assert_eq!(rows.len(), 1);
            assert!(rows[0].session_id == TARGET && rows[0].index_only);

            clean(&root, index, &[TARGET], CleanOptions::default(), || Ok(())).unwrap();
            assert!(scan(home.path(), provider).unwrap().is_empty());
            fs::write(root.join(index), [10, 255]).unwrap();
            assert!(scan(home.path(), provider).unwrap().is_empty());
        }
    }

    #[test]
    fn magpie_cleanup_rechecks_preview_and_keeps_real_sessions() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("magpie-custom/antigravity-cli");
        let conversations = root.join("conversations");
        fs::create_dir_all(&conversations).unwrap();
        let summary = Connection::open(root.join(INDEX_DB)).unwrap();
        summary.execute_batch(
            "CREATE TABLE conversation_summaries (
                conversation_id TEXT PRIMARY KEY, workspace_uris TEXT, step_count INTEGER DEFAULT 0,
                last_user_input_step_index INTEGER DEFAULT -1, title TEXT DEFAULT '',
                preview TEXT DEFAULT '', nesting_depth INTEGER DEFAULT 0, parent_conversation_id TEXT DEFAULT ''
            );",
        ).unwrap();
        let workspace = |name: &str| {
            let path = std::env::temp_dir().join(name);
            let url = tauri::Url::from_file_path(path).unwrap();
            serde_json::to_string(&[url.as_str()]).unwrap()
        };
        let probe = workspace("magpie-agy-models-Ab12Cd");
        let history = "00000000-0000-4000-8000-000000000004";
        for (id, uris) in [
            (TARGET, probe.clone()),
            (FORK, probe.clone()),
            (NESTED, workspace("my-magpie-agy-models-Ab12Cd")),
            (history, probe),
        ] {
            summary.execute(
                "INSERT INTO conversation_summaries (conversation_id,workspace_uris) VALUES (?1,?2)",
                [id, &uris],
            ).unwrap();
        }
        fs::write(
            root.join(HISTORY),
            format!("{{\"conversationId\":\"{history}\"}}\n"),
        )
        .unwrap();
        fs::write(
            root.join(CLI_INDEX),
            [summary_entry(TARGET, false), summary_entry(FORK, false)].concat(),
        )
        .unwrap();
        let empty = conversations.join(format!("{TARGET}.db"));
        let real = conversations.join(format!("{FORK}.db"));
        for path in [&empty, &real] {
            Connection::open(path)
                .unwrap()
                .execute("CREATE TABLE steps (id INTEGER)", [])
                .unwrap();
        }
        Connection::open(&real)
            .unwrap()
            .execute("INSERT INTO steps VALUES (1)", [])
            .unwrap();
        let preview = magpie_probes::preview(root.clone()).unwrap();
        assert_eq!(preview.ids, [TARGET]);

        let attempt = || {
            super::clean(
                &root,
                CLI_INDEX,
                &[(TARGET, false)],
                CleanOptions::default(),
                Scope::MagpieProbes,
                || Ok(()),
            )
        };
        let update = |steps: u8| {
            summary
                .execute(
                    "UPDATE conversation_summaries SET step_count = ?1 WHERE conversation_id = ?2",
                    rusqlite::params![steps, TARGET],
                )
                .unwrap();
        };
        update(1);
        assert!(attempt().is_err() && empty.exists());
        update(0);
        attempt().unwrap();

        assert!(!empty.exists() && real.exists());
        assert_eq!(store_index_ids(&root, CLI).unwrap().len(), 3);
        assert_eq!(
            indexed_ids(&fs::read(root.join(CLI_INDEX)).unwrap(), false, false).unwrap(),
            [FORK]
        );
        assert!(magpie_probes::preview(root).unwrap().ids.is_empty());
    }
}
