# AGENTS.md

English | [中文](AGENTS_ZH.md)

## What this project is

Anti-Power is a desktop tool (Tauri + Vue) that lists and cleans local sessions for the Antigravity App and the Antigravity CLI (`agy`). It does not manage other tools. The earlier IDE patches (Markdown copy, LaTeX, Mermaid) were removed after v4.0.0.

Anti-Power ships Anti-Power Usage for local per-request usage and cost. Quota tracking is available in the upstream antigravity-usage-monitor plugin.

## Layout

- `patcher/src-tauri/src/commands/`: Rust backend
  - `antigravity_store.rs`: store roots, ID validation, symlink-safe path resolution
  - `antigravity_scan.rs`: session listing and message loading
  - `antigravity_clean.rs`: session cleanup
  - `cleanup_extras.rs`: Antigravity App window state and caches, CC Switch sync records, and usage ledger cleanup
  - `usage_ledger.rs`: folding removed sessions into Anti-Power Usage's deleted totals
  - `client_versions.rs`: installed App and CLI versions for the session source selector
  - `plugin_install.rs`: plugin installation, CLI status line setup and restoration, price updates, and the macOS launcher
  - `sessions.rs`: session Tauri commands (`scan_sessions`, `load_session_messages`, `delete_sessions`)
- `patcher/src/`: Vue frontend; `components/SessionViewer.vue` is the main view
- `plugin/`: Anti-Power Usage sidecar and bundled catalog. `patcher/plugin-ui/` holds the Vue panel source.
- `patcher/src/locales/`: `zh-CN` and `en-US` strings
- `.github/workflows/release.yml`: installer builds for Windows, macOS, and Linux

## Session stores

| Source | Root | Summary index |
|--------|------|---------------|
| Antigravity App (`antigravity`) | `~/.gemini/antigravity` | `agyhub_summaries_proto.pb` |
| Antigravity CLI (`agy`) | `~/.gemini/antigravity-cli`, `~/.cc-switch/agy/<profile>/antigravity-cli` | `jetbox_summaries_proto.pb` |

A session is identified by its source and UUID. Under each root, a session can appear in `conversations/`, `brain/`, `annotations/`, `presence/`, `conversation_summaries.db` (summaries, FTS, parents), the summary index, `cache/last_conversations.json`, `cache/conversation_metadata.json`, `history.jsonl` (the Antigravity CLI prompt history), and plugin `sidecar_data/`. Outside the stores, the Antigravity App keeps window state in `app_storage.json`, and CC Switch keeps sync progress in `~/.cc-switch/cc-switch.db` and its backups. The README lists what cleanup removes and what it leaves.

Last checked on macOS against Antigravity App 2.34.0 and Antigravity CLI 1.5.14. Antigravity IDE (`~/.gemini/antigravity-ide`) is out of scope.

## Cleanup rules

- Refuse to run while the Antigravity App, Antigravity CLI, or Antigravity IDE is running. CC Switch can stay open.
- Never follow symlinks inside a store.
- Stage files in `.anti-power-clean-*` and record the commit in the index transaction. After an interruption, restore uncommitted files and finish committed deletions. A scan reports a failed recovery as a warning and still lists sessions.
- Clean each store in one pass for a batch: one transaction, one FTS rebuild, one `VACUUM`. When a batch fails, retry sessions one by one unless the failure is store-wide.
- Fail without changes when a summary index has an unknown layout.
- Clean data outside the stores after the stores commit via persisted receipts (`.anti-power-extras-*`, `.anti-power-sync-pending`), retrying unfinished work on later scans and reporting failures as warnings.
- Nested agent runs (`nesting_depth > 0`) go with their parent. Forks stay.
- A session listed by another tool must be listed here, including records with only index entries.

## Build and test

Run in `patcher/`:

```bash
npm install
npm run build          # type check and frontend build
npm run tauri:dev      # run the app
cd src-tauri && cargo test && cargo clippy --all-targets
cd ../../plugin/sidecars/panel && npm test
```

Tests build synthetic stores in temporary directories. Never run cleanup against a real store in tests.

## Conventions

- Keep code and docs short.
- Code comments are in English, with one space after the comment marker and no trailing period.
- Docs use English punctuation in both languages.
- Leave published release notes and changelog entries as they are.
- `patcher/package.json`, `patcher/src-tauri/tauri.conf.json`, `patcher/src-tauri/Cargo.toml`, `plugin/plugin.json`, and `plugin/sidecars/panel/package.json` share one version number (see `docs/guides/release-guide.md`).
