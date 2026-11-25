# AGENTS.md

English | [中文](AGENTS_ZH.md)

## What this project is

Anti-Power is a desktop tool (Tauri + Vue) that lists and cleans local sessions for the Antigravity App and the Antigravity CLI (`agy`). It also clears empty model probe sessions from Magpie's separate agy store. The earlier IDE patches (Markdown copy, LaTeX, Mermaid) were removed after v4.0.0.

Anti-Power installs or uninstalls the official upstream Usage Monitor release.

## Layout

- `patcher/src-tauri/src/commands/`: Rust backend
  - `antigravity_store.rs`: store roots, ID validation, symlink-safe path resolution
  - `antigravity_scan.rs`: session listing and message loading
  - `antigravity_clean.rs`: session deletion, store transactions, and recovery
  - `index_only.rs`: matching and leftover index-only target selection and validation
  - `magpie_probes.rs`: Magpie agy store resolution and empty probe validation
  - `cc_switch.rs`: CC Switch sync record cleanup
  - `usage_monitor.rs`: Usage Monitor attachment protection and session data cleanup
  - `cleanup_extras.rs`: Antigravity App window state, browser caches, and usage ledger cleanup
  - `usage_ledger.rs`: folding removed sessions into Anti-Power Usage's deleted totals
  - `client_versions.rs`: installed App and CLI versions for the session source selector
  - `plugin_install.rs`: download, install and uninstall Usage Monitor
  - `plugin_links.rs`: patch Anti-Power navigation links into Usage Monitor
  - `sessions.rs`: session Tauri commands and request/response types
- `patcher/src/`: Vue frontend; `components/SessionViewer.vue` is the main view, `components/SessionMessages.vue` renders a conversation (Markdown via `markdown.ts`)
- `patcher/src/locales/`: `zh-CN` and `en-US` strings
- `.github/workflows/release.yml`: installer builds for Windows, macOS, and Linux

## Session stores

| Source | Root | Summary index |
|--------|------|---------------|
| Antigravity App (`antigravity`) | `~/.gemini/antigravity` | `agyhub_summaries_proto.pb` |
| Antigravity CLI (`agy`) | `~/.gemini/antigravity-cli`, `~/.cc-switch/agy/<profile>/antigravity-cli` | `jetbox_summaries_proto.pb` |

Magpie cleanup uses the separate `antigravity-cli/` store (see README), sharing the CLI summary index.

A session is identified by its source and UUID. Under each root, a session can appear in `conversations/`, `brain/`, `annotations/`, `presence/`, `conversation_summaries.db` (summaries, FTS, parents), the summary index, `cache/last_conversations.json`, `cache/conversation_metadata.json`, `history.jsonl` (the Antigravity CLI prompt history), and plugin `sidecar_data/`. Outside the stores, the Antigravity App keeps window state in `app_storage.json`, and CC Switch keeps sync progress in `~/.cc-switch/cc-switch.db` and its backups. The README lists what cleanup removes and what it leaves.

Last checked on macOS against Antigravity App 2.35.1 and Antigravity CLI 1.5.16. Antigravity IDE (`~/.gemini/antigravity-ide`) is out of scope.

## Cleanup rules

- Refuse session cleanup while the Antigravity App, Antigravity CLI, or Antigravity IDE is running. CC Switch and Usage Monitor can stay open (cache cleanup stays pending until the next scan after Usage Monitor exits).
- Refuse cleanup when a standard Usage Monitor data directory has a migration archive and the source `.db` is gone, and roll back if a new archive appears before commit, to preserve source attachments.
- Never follow symlinks inside a store.
- Stage files in `.anti-power-clean-*` and record the commit in the index transaction. After an interruption, restore uncommitted files and finish committed deletions. A scan reports a failed recovery as a warning and still lists sessions.
- Clean each store in one pass per round (one transaction, one FTS rebuild, one `VACUUM`); for App and CLI sessions, retry individually on batch failure unless the error is store-wide.
- Magpie cleanup also requires Magpie to be closed, rechecks confirmed probes in the store transaction, and aborts the batch if any no longer qualifies.
- After a selected deletion succeeds, clear matching index-only records in other sources and previewed leftover indexes (rechecking artifacts and migration archives across known local stores before commit).
- Fail without changes when a summary index has an unknown layout.
- Clean data outside the stores after the stores commit via persisted receipts (`.anti-power-extras-*`, `.anti-power-sync-pending`), retrying unfinished work on later scans and reporting failures as warnings.
- Nested agent runs (`nesting_depth > 0`) go with their parent. Forks stay.
- List all sessions from the App and CLI stores, including index-only records.

## Build and test

Run in `patcher/`:

```bash
npm install
npm run build          # type check and frontend build
npm run tauri:dev      # run the app
cd src-tauri && cargo test && cargo clippy --all-targets
```

Tests build synthetic stores in temporary directories. Never run cleanup against a real store in tests.

## Conventions

- Keep code and docs short.
- Keep code comments in English, with one space after the comment marker and no trailing period.
- Keep paired Chinese and English docs (`AGENTS*`, `README*`, `CHANGELOG*`) in sync, using English punctuation in both.
- Reuse the current `CHANGELOG.md` and `CHANGELOG_EN.md` entries verbatim in `release-notes.md`; leave published entries as they are.
- `patcher/package.json`, `patcher/src-tauri/tauri.conf.json`, and `patcher/src-tauri/Cargo.toml` share one version number.
