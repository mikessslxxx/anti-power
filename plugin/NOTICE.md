# Third-party code

Anti-Power Usage includes code adapted from
[antigravity-usage-monitor](https://github.com/yuhuan-dev/antigravity-usage-monitor),
licensed under the GNU General Public License v3.0. Anti-Power uses the same license. A copy is included in the repository root and in the installed plugin folder.

Upstream revision: `f53f27927f225b4acd91a24208e88c892784b67c` (v1.5.2)

| File | Origin |
|---|---|
| `sidecars/panel/vendor/usage-monitor/trajectory-metrics.mjs` | Copied unchanged |
| `sidecars/panel/vendor/usage-monitor/backend/proto-wire.mjs` | Copied unchanged |
| `sidecars/panel/vendor/usage-monitor/backend/sqlite-client.mjs` | Copied unchanged |
| `sidecars/panel/vendor/usage-monitor/backend/model-pricing.mjs` | Copied unchanged |
| `sidecars/panel/vendor/usage-monitor/backend/cli-sqlite-reader.mjs` | Trajectory and annotation readers only (omits summary-index cache); returns full text on demand and distinguishes empty trajectories from read errors |
| `sidecars/panel/vendor/usage-monitor/backend/pricing-catalog.mjs` | Rewritten: keeps the catalog format and validation, drops cloud sync and plugin updates |
| `sidecars/panel/vendor/usage-monitor/session-context.mjs` | Current context and compaction calculations from `telemetry-builder.mjs` and `telemetry-display-rules.mjs`, using reported window limits |
| `pricing-catalog.json` | Pricing and model placeholder data, without the upstream release links |

To pick up upstream fixes, diff the copied files against the same paths in the upstream repository.

## Vue

The bundled panel includes the Vue runtime under the following license.

The MIT License (MIT)

Copyright (c) 2018-present, Yuxi (Evan) You

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
