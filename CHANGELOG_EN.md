# Changelog

[中文](CHANGELOG.md) | English

---

## v5.4.0 (2026-11-24)

### Interface
- Redesign the window with a toolbar above two cards and a navy, rust, and bronze palette across dark and light themes
- Switch session sources with a segmented control showing installed App and CLI versions, and list each session's source, time, and project under its title; add a toggle to hide index-only sessions (full-ID search still shows matches)
- Reveal the project directory and session file in the file manager from session details

### Conversation view
- Render assistant replies as Markdown (headings, lists, task lists, tables, code blocks); show raw HTML as text and never load remote images
- Render local file links as chips that reveal in the file manager, and open web links in the system browser
- Show user messages as right-aligned bubbles and group consecutive replies; collapse long messages by default and offer a jump to the latest message when scrolled away

### Session cleanup
- Clear matching index-only records in other sources after deleting a complete session, and clear confirmed leftover indexes that have no session files, attachments, or migration archive in any local source
- Refuse to delete source leftovers after a full Usage Monitor migration so migrated sessions keep their attachments; archives from failed moves do not block deletion, and custom `--data-dir` paths are not checked

---

## v5.3.0 (2026-11-22)

### Usage Monitor
- Install or uninstall the latest official [Usage Monitor](https://github.com/yuhuan-dev/antigravity-usage-monitor) release from GitHub via the title bar dialog; enable it under **Customizations → Installed** in Antigravity to view cache hit rates and 5-hour/weekly quotas
- Add two Anti-Power buttons to the official plugin on install: the panel header opens the current local session, and Settings opens the **Usage Monitor** dialog; the built-in ZIP updater reapplies them
- Keep settings and usage history in Antigravity's data directory on uninstall; back up the plugin folder and report its path when it contains a Git checkout or local plugin data
- When switching from v5.1–5.2, uninstall Anti-Power Usage with the older Anti-Power first to restore its CLI status line; the existing ledger stays on disk and is not imported

### Session cleanup
- Add an optional **Usage Monitor session history** cleanup that clears cached titles and prompts for sessions with no local copy left while preserving token counts and costs, removing the entry when the installed plugin has a matching independent usage record

---

## v5.2.1 (2026-11-22)

### Anti-Power Usage
- Fix the missing title-bar usage strip in the Antigravity App: each window's upper-right corner shows the current conversation's output speed, time to first token, and context usage, following conversation switches and staying visible with the side panel closed
- Keep the current conversation's latest measured speed and first-token latency in the CLI status line, showing the activity state until speed is available; skip uninitialized SQLite files to avoid lock contention

---

## v5.2.0 (2026-11-22)

### Anti-Power Usage
- Fix a blank side panel in the Antigravity App; narrow panels show session and request details as labeled fields, and charts resize with the panel
- Show the latest request's speed, time to first token, and context utilization in the session strip; expand for used tokens, window limit, and category breakdown (accounting for retained content and summary after compaction)
- Center the same three metrics in the Antigravity CLI footer between the shortcut hint and the active mode, model, and effort; on macOS and Linux, installing configures an existing CLI profile unless another or disabled status line is set (opt in to replace, restored on uninstall)

### Client versions and launch options
- Show detected Antigravity App and Antigravity CLI versions in the session source selector
- Add **Launch Antigravity with debugging** on macOS: quit the app and launch with a random loopback debugging port for desktop overlays (the side panel and CLI status line work without it)

---

## v5.1.0 (2026-11-22)

### Usage Plugin
- Bundle **Anti-Power Usage**: install, upgrade, uninstall, or update prices from the title bar dialog, then enable under **Customizations → Installed** in the Antigravity App; uninstalling preserves recorded usage
- Collect local requests in the background and view token usage, cache hit rate, and estimated cost by date, model, source, or session; open a session to inspect each turn's prompt, reply, thinking, tool calls, time to first token, and output speed (content is read on demand from local stores)
- Count multiple local copies of a session once, and fold recorded usage and cost into anonymous "Deleted sessions" totals after the last copy is deleted (or confirmed missing for 60 seconds)

### Session Cleanup
- **CC Switch sync records** also clears leftover sync cursors for sessions already gone from local stores across the main database and backups (preserving usage and cost history), and works while CC Switch is open (refresh after it exits to finish verification)
- Reopening Anti-Power or refreshing the session list retries any unfinished window state, usage ledger, cache, or sync cleanup

### License
- Switch to GPL-3.0-only starting with v5.1.0; the MIT notice for v5.0.1 and earlier stays in `LICENSE-MIT`, and upstream plugin credits are in `plugin/NOTICE.md`

---

## v5.0.1 (2026-11-20)

### New Features
- Deleting a session also removes its plugin `sidecar_data/` events, Usage Monitor cache entry, and matching log lines as part of the same recoverable cleanup
- Also removes the session's window layout and side panel state from the Antigravity App's `app_storage.json`
- Two new options, off by default and remembered:
  - **CC Switch sync records**: removes session file sync progress from `cc-switch.db` and `backups/*.db` under `~/.cc-switch/`, keeping usage and cost history (quit CC Switch first)
  - **Antigravity App browser cache**: empties `Cache/` and `Code Cache/`, which can hold session IDs and are rebuilt automatically
- If app data or CC Switch cleanup fails, the session stays deleted and the result lists what was not cleaned

---

## v5.0.0 (2026-11-19)

### New Features
- Anti-Power is now a local session manager for the Antigravity App and the Antigravity CLI (`agy`): browse, search, and delete single or multiple sessions
- Scoped cleanup removes conversation data, logs, annotations, presence lock, summary database rows, full-text index (with FTS5 rebuild, database compaction, and WAL checkpoint), protobuf summary index entries, recent-session/summary caches, and Antigravity CLI prompt history entries for one source and session ID, so CC Switch no longer lists the session
- Automatic recovery for interrupted cleanups: reopening the app or refreshing the list finalizes committed deletions and restores uncommitted ones
- Lists and cleans index-only leftovers, including records present only in the protobuf summary index
- Batch deletion cleans each store in one pass, and one failed session does not block the rest of the batch
- Detects CC Switch profile directories for the Antigravity CLI

### Removed
- Sidebar and Manager patches (Markdown copy, LaTeX, Mermaid, table colors), the patch installer, and the scripts. Google Antigravity now handles these natively; use v4.0.0 if you need the patches
- Session browsing and cache cleanup for Claude Code, Codex, Gemini CLI, OpenCode, and OpenClaw

### Dependencies
- Upgraded to Tauri 2.12, Vite 8, TypeScript 6, vue-i18n 11, rusqlite 0.43, and sysinfo 0.40
- Removed Tailwind CSS in favor of a minimal base style reset
- Requires Node.js 22.12+ and Rust 1.95+

### Documentation
- Synced README / Changelog / Release Notes to v5.0.0; verified against Antigravity App 2.33.0 and Antigravity CLI 1.5.12 on macOS

---

## v4.0.0 (2026-05-23)

### New Features
- Detects new `Antigravity IDE` install paths and data directory names on Windows, macOS, and Linux, including in install and cleanup scripts
- Updated the supported version to `Antigravity IDE v2.0.6`

### Improvements
- Improved Sidebar/Manager message content discovery for newer DOM structures and reduced accidental binding inside inputs, buttons, and control areas
- Copying a selection that contains KaTeX/MathJax formulas now restores their LaTeX source in both Sidebar variants and the Manager
- Cleanup now also covers `.gemini/antigravity-ide` conversation cache directories

### Fixes
- Fixed overly strict install validation for newer IDE builds without the legacy `extensions/antigravity` directory

### Documentation
- Synced README/Changelog/Release Notes to v4.0.0
- Updated the supported Antigravity IDE version in README to v2.0.6

---

## v3.3.2 (2026-03-13)

### Fixes
- Fixed abnormal Manager UI rendering on the latest Antigravity (icons showing raw ligature names and layout issues) by syncing the patched `workbench-jetski-agent.html` CSP/Trusted Types allowlists to upstream
- Synced modern Sidebar `workbench.html` CSP allowlists to upstream to prevent similar font/icon resource regressions

### Documentation
- Synced README/Changelog/Release Notes to v3.3.2

---

## v3.3.1 (2026-03-10)

### Fixes
- Fixed the code-block font-size regression in the modern Sidebar, Manager, and legacy Sidebar. When the panel custom font size is smaller than the editor font size, code blocks now respect the panel font size instead of being locked to the larger editor size

### Documentation
- Synced README/Changelog/Release Notes to v3.3.1
- Updated supported Antigravity version in README to v1.20.5

---

## v3.3.0 (2026-03-09)

### New Features
- Added a Session Viewer to the installer for viewing and deleting local conversations from Claude Code, Codex, Gemini CLI, OpenCode, and OpenClaw
- Added batch deletion for selected sessions

### Improvements
- Added OpenCode and OpenClaw support to the cleanup tool
- Code block font size in the Sidebar and Manager now follows the panel font size and editor font size for better readability under custom size settings
- Expanded highlighted-code style coverage so legacy Sidebar, modern Sidebar, and Manager keep consistent code rendering

### Documentation
- Synced README/Changelog/Release Notes to v3.3.0
- Updated supported Antigravity version in README to v1.20.4

---

## v3.2.3 (2026-03-02)

### Improvements
- Windows cleaning now auto-resolves the `sqlite3` executable, with support for common install locations beyond PATH (Chocolatey/Git for Windows/Scoop/WinGet)
- On startup, the installer now normalizes the saved Antigravity path and refreshes install state first, reducing path-state mismatch issues

### Fixes
- Preserved the user-saved path when auto-detection fails, preventing the path field from being cleared on launch

### Documentation
- Synced README/Changelog/Release Notes to v3.2.3

---

## v3.2.2 (2026-02-27)

### Improvements
- Added inline Markdown semantic preservation in copy extraction, including bold/italic/strikethrough/links/inline code
- Added conversion rules for paragraphs/blockquotes/horizontal rules/explicit line breaks to improve readability for complex copied content
- Disabled raw-text fast path when inline formatting exists to avoid losing formatting
- Updated blank-line handling to collapse consecutive empty lines to at most one while preserving paragraph and heading structure

### Documentation
- Synced README/Changelog/Release Notes to v3.2.2
- Updated supported Antigravity version in README to v1.19.6

---

## v3.2.1 (2026-02-26)

### Improvements
- LaTeX rendering now supports re-render on streaming incremental updates, avoiding stale output after content changes
- Restored `_`/`__` inside `$...$` formulas when Markdown emphasis tags split underscore tokens
- Mermaid/KaTeX load failures are no longer cached, so later scans can retry automatically
- Deduplicated copy-success timers to reduce button flicker on rapid clicks
- Removed Sidebar/Manager Mermaid debug logs to reduce console noise

### Fixes
- Fixed Sidebar Mermaid Trusted Types policy name to match the `sidebarPanel` CSP allowlist
- Fixed Unix temp script/temp directory naming collision risks for better repeated install stability
- Auto-clean `.bak` backup files after uninstall restore
- Added `~/.config` fallback when resolving config path for better Linux compatibility

### Documentation
- Synced README/Changelog/Release Notes to v3.2.1

---

## v3.2.0 (2026-02-24)

### New Features
- Adapted to the new Antigravity sidebar entry, with automatic legacy/modern patch path switching based on `ideVersion` in `product.json`

### Improvements
- Added Antigravity version display in the installer UI
- Confirmation modal now shows file changes dynamically based on sidebar entry mode
- Aligned modern sidebar styles with Manager styles and fixed copy-button display details
- Fixed patch status state sync to avoid incorrectly enabling the other module in single-module installs

### Documentation
- Synced README/Changelog/Release Notes to v3.2.0
- Updated supported Antigravity version in README to v1.18.4

---

## v3.1.0 (2026-02-06)

### New Features
- Cleaning tool now supports Windows, including conversation cache cleanup for Antigravity, Gemini CLI, Codex and Claude Code.

### Improvements
- Backend command error handling is now localized to match the current UI language.
- Frontend now passes locale to backend commands for consistent cross-platform error messages.

### Documentation
- Synced README/Changelog/Release Notes to v3.1.0

---

## v3.0.1 (2026-02-05)

### New Features
- Cleaning tool now supports Gemini CLI, Codex and Claude Code, with only Antigravity selected by default.
- Cleaning tool now adds an enable switch to prevent accidental triggering.
- Added English scripts `anti-power.en.sh`/`anti-clean.en.sh`

### Fixes
- Fixed Chinese output when running scripts in English mode

### Documentation
- Manual install guide updated to include English scripts
- Synced README/Changelog/Release Notes to v3.0.1

---

## v3.0.0 (2026-02-04)

### New Features
- UI language switch (Chinese/English)
- Light/dark theme toggle
- Collapsible feature cards for sidebar/Manager options

### Improvements
- Installer layout redesigned with a responsive two-column view; clean tool placement adapts to screen size
- Visual refresh across typography, color tokens, buttons, cards, modals, and animations

### Fixes
- Fixed the execution logic of cleaning tools (macOS/Linux) to ensure secondary confirmation

### Documentation
- Synced README/Changelog/Release Notes to v3.0.0
- README: update supported Antigravity version to v1.16.5 and standardize macOS app name to `Anti-Power.app`

---

## v2.4.0 (2026-02-02)

### New Features
- Added support for macOS universal builds
- Added `icon-gen` icon generation script
- Added `npm run clean` command
- Dependency updates

### Documentation
- Synced cross-platform installation instructions
- Fixed legacy documentation issues

---

## v2.3.4 (2026-02-01)

> Community contribution: Thanks to [@mikessslxxx](https://github.com/mikessslxxx) for PR #18

### New Features
- macOS/Linux cross-platform support
- Cross-platform path normalization and detection
- Unix privileged installation flow (sudo/pkexec)
- GitHub Actions automated build and release

### Improvements
- Added Linux support to path detection
- Frontend adapts to macOS Resources path
- Adjusted window decorations to fit the macOS system title bar

---

## v2.3.3 (2026-01-31)

### Fixes
- Fix critical bug where math formula rendering fails
- Temporarily restored the stable v2.0.1 formula rendering logic
- Add formula rendering mode switch (Classic/Deferred)

---

## v2.3.2 (2026-01-31)

> Community contribution: Thanks to [@mikessslxxx](https://github.com/mikessslxxx)

### Improvements
- Math formula rendering optimization
- Copy button style optimization

### New Features
- Copy button offers more customization options (style, position, etc.)

### Fixes
- Fix "Installation appears corrupted" prompt caused by Manager patch

---

## v2.3.1 (2026-01-31)

> Community contribution: Thanks to [@Sophomoresty](https://github.com/Sophomoresty) for the PR

### New Features
- Added an always-visible copy button in the bottom-right corner
- Developer documentation (bilingual: English and Chinese)

### Improvements
- KaTeX CSS and JS parallel loading for faster initial formula rendering
- Top-right hover button moved up to avoid text obstruction
- Code blocks use monospace font stack (Cascadia Code, Fira Code, etc.)
- Remove excess empty lines in copied content

### Fixes
- Correctly identify code blocks and language identifiers in nested lists
- Inline code (`pre.inline`) correctly extracted as backtick format
- Filter SVG tags to avoid copying icon code

### Changes
- Removed copy button from good/bad feedback area (reduce UI clutter)
- Manager features (mermaid, math) enabled by default

---

## v2.3.0 (2026-01-30)

### New Features
- Copying preserves Markdown formatting, including headings, lists, bold, italics, and links

---

## v2.2.0 (2026-01-21)

> Community contribution: Thanks to [@mikessslxxx](https://github.com/mikessslxxx)

### New Features
- Manager window Mermaid rendering
- Manager window math formula rendering
- Manager window conversation width adjustment
- Manager window font size adjustment

---

## v2.1.0 (2026-01-19)

### New Features
- Sidebar font size adjustment
- Manager window one-click copy

### Improvements
- Mermaid rendering error hint optimization

---

## v2.0.1 (2026-01-14)

### Improvements
- Performance optimization

---

## v2.0.0 (2026-01-14)

### New Features
- New Tauri installation tool
- Support for toggling individual features

---

## v1.2.1 (2026-01-13)

### Fixes
- Bug fixes

---

## v1.2.0 (2026-01-13)

### New Features
- Mermaid flowchart rendering

---

## v1.1.0 (2026-01-13)

> Community contribution: Thanks to [@mikessslxxx](https://github.com/mikessslxxx)

### New Features
- Math formula rendering (KaTeX)

---

## v1.0.0 (2026-01-13)

### Initial Release
- One-click copy button
- Table color fix (dark theme)
