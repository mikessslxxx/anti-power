<p align="center">
  <img src="docs/assets/images/LOGO.png" alt="Anti-Power" width="120">
</p>

<h1 align="center">Anti-Power</h1>

<p align="center">
  <a href="https://github.com/mikessslxxx/anti-power/releases"><img src="https://img.shields.io/badge/Version-v5.2.1-blue.svg" alt="Version"></a>
  <img src="https://img.shields.io/badge/Antigravity_App-2.34.0-green.svg" alt="Antigravity App">
  <img src="https://img.shields.io/badge/Antigravity_CLI-1.5.14-green.svg" alt="Antigravity CLI">
</p>

<p align="center">
  <a href="README.md">中文</a> | English
</p>

> Browse and remove local sessions for the **Antigravity App** and **Antigravity CLI** (`agy`), with an Antigravity plugin that records usage for each request.

## Features

- Lists sessions from the Antigravity App (`~/.gemini/antigravity`) and Antigravity CLI (`~/.gemini/antigravity-cli`), plus CC Switch profiles under `~/.cc-switch/agy/<profile>/antigravity-cli`
- Searches by title, project, or session ID, filters by source (with detected client versions), and views messages
- Deletes single or multiple sessions and reports the outcome for each
- Flags index-only leftovers (records left behind after another tool removes the session files) so they can be removed as well

Verified on macOS with Antigravity App 2.34.0 and Antigravity CLI 1.5.14. The Windows and Linux builds are untested. Antigravity IDE (`~/.gemini/antigravity-ide`) is not supported.

## What gets cleaned

Each deletion is scoped to a single source and session ID. After cleanup, tools such as CC Switch no longer list the session:

| Location | Content |
|----------|---------|
| `conversations/<id>.db` with `-wal`, `-shm`, `-journal`, and `<id>.pb` | Conversation data |
| `brain/<id>/` | Logs and artifacts |
| `annotations/<id>.pbtxt`, `presence/<id>.lock` | Annotations and presence lock |
| `conversation_summaries.db` | Summaries, full-text index, parent links |
| `agyhub_summaries_proto.pb`, `jetbox_summaries_proto.pb` | Summary index |
| `cache/last_conversations.json`, `cache/conversation_metadata.json` | Recent sessions and cached summaries |
| `history.jsonl` | Session entries in the Antigravity CLI prompt history |
| `sidecar_data/<plugin>/<panel>/events/*.json` | Plugin sidecar events |
| `data/trajectory_usage_cache.json`, `logs/sidecar.log` under `sidecar_data/<plugin>/<panel>/` | Usage Monitor cache entry and matching log lines |
| `app_storage.json` in the Antigravity App data folder | Window layout and side panel state |
| `ledger.json` under `sidecar_data/anti-power-usage/` | The usage plugin's totals for the session, moved into an anonymous "Deleted sessions" row |

- Subagent conversations are removed with their parent. Sessions created by `/fork` are kept.
- The same ID can exist in both the Antigravity App and Antigravity CLI. Each is listed separately; select both to remove it from both.
- The Antigravity App data folder is `~/Library/Application Support/Antigravity` on macOS, `%APPDATA%\Antigravity` on Windows, and `~/.config/Antigravity` on Linux.
- Runtime logs (`log/`) and files written to `/tmp` or project folders are left untouched.
- After cleanup, `conversation_summaries.db` is compacted so that no fragments of removed rows remain inside it; if another tool is reading the database, compaction is skipped and fragments stay until the next cleanup. Deleted files themselves are not wiped at the disk level.

Two options at the top of the window are off by default, and the choice is remembered:

- **CC Switch sync records**: removes session file sync progress (`session_log_sync`) for selected sessions and sessions with no trace left in their store from `cc-switch.db` and `backups/*.db` under `~/.cc-switch/`, then compacts each database. CC Switch can stay open (the live database is left uncompacted; refresh once after CC Switch exits to finish verification). Usage and cost history (`proxy_request_logs`, `session_usage_dedup`, `usage_daily_rollups`) is kept; it holds session IDs, models, and token usage, not conversation content.
- **Antigravity App browser cache**: empties `Cache/` and `Code Cache/` in the Antigravity App data folder. They cannot be filtered by session and may hold session IDs; the Antigravity App rebuilds them, so the next launch is a little slower.

App data and CC Switch cleanup runs after session deletion is committed; if it fails, the session stays deleted and the result lists what was not cleaned.

Quit the Antigravity App and Antigravity CLI before deleting; cleanup is also refused while Antigravity IDE is running. Deletion cannot be undone.

If cleanup is interrupted, quit any running Antigravity clients and reopen Anti-Power or refresh the session list: Anti-Power finalizes committed deletions and restores uncommitted ones, or shows a notice above the list if recovery fails.

## Usage plugin

Anti-Power ships an Antigravity plugin, **Anti-Power Usage**, that records each model request:

- the user message, the model's response, its thinking, and tool calls
- new input, cache read, output, and thinking tokens, and the cache hit rate
- time to first token, request duration, and output speed
- cost estimated from model prices; models without a price show as unpriced and stay out of cost totals
- totals for today, 7 days, 30 days, or a custom range, split by day, model, source, and session

Use the usage plugin button in the title bar to install, update, or uninstall it, then turn it on under **Customizations → Installed** in the Antigravity App and open it from the side panel. The running sidecar collects usage at startup and every 10 seconds, even with the panel closed. The plugin only reads local session stores, does not call the Language Server, and uploads nothing; conversation text is read when you open a session and is not stored separately.

The strip at the top of the panel shows request speed, time to first token, and context utilization for the current session. Expand it for the token count, window limit, and usage breakdown. The Antigravity CLI status line also works independently and can display, for example, `48.8 t/s | 4.4s | ◎ 19%`.

On macOS and Linux, installation configures the status line for an existing CLI profile (with an opt-in to replace an existing or disabled setting, restored on uninstall). On macOS, to show the same three metrics in each window's upper-right corner (updating even with the side panel closed), quit the App and click **Launch Antigravity with debugging** in the plugin dialog.

Once all local copies of a session are gone, its recorded usage is folded into an anonymous "Deleted sessions" row (external removals are confirmed after 60 seconds). Prices come from [antigravity-usage-monitor](https://github.com/yuhuan-dev/antigravity-usage-monitor) and can be updated from Anti-Power's plugin dialog; for quota tracking, use that plugin instead.

## Installation

Download an installer from [Releases](https://github.com/mikessslxxx/anti-power/releases):

- **Windows**: `anti-power-windows.exe`
- **macOS (Universal)**: `anti-power-macos-universal.dmg` (unsigned; if macOS reports it is damaged, run `xattr -cr /Applications/Anti-Power.app`)
- **Linux**: `anti-power-linux.AppImage`

Running from source requires Node.js 22.12+ and Rust 1.95+:

```bash
cd patcher
npm install
npm run tauri:dev
```

## Legacy patches

Google Antigravity now renders LaTeX and Mermaid natively, so the patches were removed. [v4.0.0](https://github.com/mikessslxxx/anti-power/releases/tag/v4.0.0) is the last release that includes them and supports up to Antigravity IDE v2.0.6.

## Documentation

- Changelog: [CHANGELOG_EN.md](CHANGELOG_EN.md)
- Release guide: [release-guide.md](docs/guides/release-guide.md)

## Contributing

Issues and pull requests are welcome.

## Acknowledgments

- [@daoif](https://github.com/daoif) (original author)
- [@syanle](https://github.com/syanle)
- [@Sophomoresty](https://github.com/Sophomoresty)
- [@Wusir7355608](https://github.com/Wusir7355608)
- [@aiaiads](https://github.com/aiaiads)

## License

[GPL-3.0](LICENSE). v5.0.1 and earlier were released under the MIT License; that notice is kept in [LICENSE-MIT](LICENSE-MIT).

Parts of `plugin/` are adapted from [antigravity-usage-monitor](https://github.com/yuhuan-dev/antigravity-usage-monitor) (GPL-3.0); see [plugin/NOTICE.md](plugin/NOTICE.md).
