# Anti-Power Usage

Antigravity plugin that records every model request in local Antigravity App and Antigravity CLI sessions,
including CC Switch profiles under `~/.cc-switch/agy/<profile>/antigravity-cli`:

- the user message and the model's response, thinking, and tool calls
- new input, cache read, output, and thinking tokens
- time to first token, request duration, and output speed
- cost from `pricing-catalog.json`; requests on unknown models show as unpriced and are left out of cost totals

Anti-Power installs it into `~/.gemini/config/plugins/anti-power-usage`. Turn it on under Customizations → Installed, then open it from the side panel.

## Session metrics

The panel shows the selected conversation's latest model request speed, time to first token, and context utilization. Expand the strip to see used tokens, the reported window limit, and the available breakdown. The calculation includes output and accounts for context compaction. An unknown limit displays as `—`.

On macOS and Linux, installing configures the status line for an existing agy profile (opt in to replace an existing or disabled setting; uninstalling restores it). For manual setup, set `statusLine.command` to `node "/absolute/path/to/anti-power-usage/sidecars/panel/lib/statusline.mjs"`. It centers speed, first-token latency, and context usage (preferring agy's native percentage) between the shortcut hint and the active mode, model, and effort, and runs without the desktop sidecar.

The CLI status line keeps the conversation's latest measured speed and first-token latency, falling back to a matching usage-monitor snapshot within 20 seconds. Until speed is available, it shows the CLI activity state.

On macOS, quit the Antigravity App and click **Launch Antigravity with debugging** in Anti-Power's plugin manager; the sidecar uses the App's loopback debugging port to show the same three metrics in each window's upper-right corner. The strip follows each window's conversation even with the panel closed.

## Data

While running, the sidecar scans `conversations/<id>.db` (and its WAL) at startup and every 10 seconds, even with the panel closed; incomplete reads are reported in the panel and retried on the next scan.

The ledger lives in the data directory assigned by Antigravity, normally
`~/.gemini/antigravity/sidecar_data/anti-power-usage/panel/data/ledger.json`. Anti-Power and the plugin share the adjacent SQLite lock, and both recover an interrupted publish from `ledger.json.pending`.

For each live session, the ledger keeps its title, per-step usage, and daily model totals. Readable copies are merged by step number, preferring completed requests and keeping prior usage when a surviving copy is shorter or still in progress. Prompts, replies, thinking, and tool arguments are read from the database on demand.

When Anti-Power commits a deletion of the last copy, it folds recorded usage into anonymous daily model and client totals and removes the session ID, title, and parent links. Reopening Anti-Power or refreshing the session list resumes any interrupted cleanup.

Sessions removed outside Anti-Power are hidden on the next full scan and anonymized once a second full scan at least 60 seconds later confirms all copies are gone (linked or unreadable profiles pause confirmation, and the same grace period protects missing replicas during an Anti-Power delete). Restoring a file before confirmation keeps prior totals; restoring later imports it again.

Use **Update prices** in Anti-Power to download the upstream catalog; live sessions are repriced on reload (day-only legacy entries keep their original estimate across intraday rate changes), while deleted totals keep their recorded rates.

Truncated ledger JSON is rebuilt once all stores are readable. Readable JSON with invalid records reports an error, displays valid entries, and leaves the file untouched. Successful scans and cleanups also remove legacy v1 `ledger.json.broken-*` backups that could still hold session IDs or titles.

## Development

```bash
cd sidecars/panel
npm test
cd ../../../patcher
npm run build:plugin
```

Requires Node.js 22.13+ (`node:sqlite`).

See [NOTICE.md](NOTICE.md) for code adapted from antigravity-usage-monitor.
