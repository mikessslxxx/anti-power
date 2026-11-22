// agy supplies the active session and its native context percentage on stdin
import { readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { stripVTControlCharacters } from 'node:util';
import { readLocalSqliteTrajectory } from '../vendor/usage-monitor/backend/cli-sqlite-reader.mjs';
import { extractModelSteps, speedMetric } from '../vendor/usage-monitor/trajectory-metrics.mjs';
import { sessionContext } from '../vendor/usage-monitor/session-context.mjs';
import { checkDirectory, regularFile, sessionFile } from './stores.mjs';

const UUID = /^[\da-f]{8}(?:-[\da-f]{4}){3}-[\da-f]{12}$/i;

const segmenter = new Intl.Segmenter();
const wide = /[\u1100-\u115f\u2329-\u232a\u2e80-\ua4cf\uac00-\ud7a3\uf900-\ufaff\ufe10-\ufe19\ufe30-\ufe6f\uff01-\uff60\uffe0-\uffe6\u{20000}-\u{3fffd}]|\p{Emoji_Presentation}|\uFE0F/u;

function label(value) {
  if (typeof value !== 'string') return '';
  return stripVTControlCharacters(value).replace(/[\p{Cc}\p{Bidi_Control}]/gu, ' ').trim();
}

function columns(text) {
  let size = 0;
  for (const { segment } of segmenter.segment(text)) {
    if (!/^\p{Mark}+$/u.test(segment)) size += wide.test(segment) ? 2 : 1;
  }
  return size;
}

function shorten(text, width) {
  if (columns(text) <= width) return text;
  let result = '';
  let remaining = width - 1;
  for (const { segment } of segmenter.segment(text)) {
    remaining -= columns(segment);
    if (remaining < 0) break;
    result += segment;
  }
  return result + '…';
}

function compose(left, center, right, width) {
  const leftWidth = columns(left);
  const centerWidth = columns(center);
  const rightWidth = columns(right);
  const first = leftWidth + (left ? 2 : 0);
  const last = width - centerWidth - rightWidth - (right ? 2 : 0);
  if (last < first) return null;
  const start = Math.max(first, Math.min(Math.floor((width - centerWidth) / 2), last));
  const middle = left + ' '.repeat(start - leftWidth) + center;
  return right ? middle + ' '.repeat(width - start - centerWidth - rightWidth) + right : middle;
}

// agy displays this single line in place of the default footer
export function formatStatusLine(input, parts, settings = {}) {
  const full = parts.join(' | ');
  const reported = Number(input.terminal_width);
  if (!Number.isFinite(reported) || reported < 1) return full;
  const padding = Math.max(0, Math.floor(Number(settings.statusLine?.padding) || 0));
  const width = Math.max(1, Math.min(10000, Math.floor(reported)) - 1 - 2 * padding);
  if (settings.statusLine?.stack_with_default === true) {
    return compose('', shorten(full, width), '', width);
  }

  const vim = label(input.vim?.mode).toUpperCase();
  const left = vim ? vim + ' · ? for shortcuts' : '? for shortcuts';
  const name = label(input.model?.display_name) || label(input.model?.id) || label(settings.model);
  const suffix = name.match(/\s*\((minimal|low|medium|high|xhigh|max)\)$/i);
  const model = suffix ? name.slice(0, suffix.index).trimEnd() : name;
  const effort = (label(input.model?.effort).replace(/^EFFORT_/i, '') || suffix?.[1] || '').toLowerCase();
  const mode = label(input.cycle_mode).replace(/^(?:AGENT_MODE_|PERMISSION_MODE_)/i, '')
    .toLowerCase().replaceAll('_', '-');
  const right = [!['none', 'default', 'unspecified'].includes(mode) && mode, model, effort].filter(Boolean).join(' · ');

  // Keep the model visible as space tightens, and avoid writing the terminal's last column
  for (const [hint, metrics, info] of [
    [left, full, right], ['?', full, right], ['?', parts.join('|'), right],
    ['?', parts.join('|'), model || right], ['?', parts[0], model || right],
  ]) {
    const line = compose(hint, metrics, info, width);
    if (line !== null) return line;
  }
  return shorten(model || right || full, width);
}

// Keep the latest valid speed and TTFT independently while a new turn streams
function lastTimings(rows) {
  let speed = null;
  let ttft = null;
  for (let i = rows.length - 1; i >= 0 && (speed === null || ttft === null); i--) {
    const row = rows[i];
    if (row.isCompactionStep) continue;
    speed ??= speedMetric(row).value;
    if (ttft === null && Number.isFinite(row.ttftSec) && row.ttftSec >= 0) ttft = row.ttftSec;
  }
  return { speed, ttft };
}

// usage-monitor publishes a short-lived snapshot when its background monitor is running
function recentSnapshot(base, id) {
  const now = Date.now();
  let latest;
  for (const client of ['antigravity-cli', 'antigravity']) {
    const dir = join(base, client, 'sidecar_data', 'usage-monitor', 'panel', 'data');
    try {
      checkDirectory(base, dir);
      const file = join(dir, 'cli-status-snapshot.json');
      if (regularFile(file).size > 65536n) continue;
      const snap = JSON.parse(readFileSync(file, 'utf8'));
      const age = now - snap.updatedAtMs;
      if (snap.activeConversation?.cascadeId !== id || !Number.isFinite(snap.updatedAtMs) ||
          age < 0 || age > 20_000 || (latest && snap.updatedAtMs <= latest.updatedAtMs)) continue;
      latest = snap;
    } catch {
      // The upstream plugin and its snapshot are optional
    }
  }
  return latest?.activeConversation;
}

export function cliStatusLine(input, env = process.env, home = homedir()) {
  const transcript = String(input.transcript_path || input.transcriptPath || '').split(/[\\/]/);
  const id = [input.conversation_id, input.conversationId, input.session_id, input.sessionId,
    env.ANTIGRAVITY_CONVERSATION_ID, ...transcript.reverse()].find((value) => UUID.test(value));
  const base = resolve(env.ANTIGRAVITY_GEMINI_DIR || join(home, '.gemini'));
  let speed = null;
  let ttft = null;
  let context;
  if (id) {
    const key = id.toLowerCase();
    try {
      const file = sessionFile({ home: base, root: join(base, 'antigravity-cli') }, key + '.db');
      const data = readLocalSqliteTrajectory(file.dbPath, { cascadeId: key });
      if (data) {
        const rows = extractModelSteps(data.steps, data.generatorMetadata);
        ({ speed, ttft } = lastTimings(rows));
        context = sessionContext(rows);
      }
    } catch {
      // A new session may not have a database yet
    }
    if (speed === null || ttft === null) {
      const snapshot = recentSnapshot(base, key);
      if (Number.isFinite(snapshot?.latestTokPerSec) && snapshot.latestTokPerSec >= 0) speed ??= snapshot.latestTokPerSec;
      if (Number.isFinite(snapshot?.latestTtftSec) && snapshot.latestTtftSec >= 0) ttft ??= snapshot.latestTtftSec;
    }
  }
  const nativePct = input.context_window?.used_percentage;
  const { used, limit } = context ?? {};
  const pct = Number.isFinite(nativePct) ? nativePct : limit && used != null ? 100 * used / limit : null;
  const state = String(input.agent_state || '').toLowerCase();
  const zh = /^zh/i.test(env.LC_ALL || env.LC_MESSAGES || env.LANG || '');
  let activity = '— t/s';
  if (['running', 'streaming', 'executing', 'thinking'].includes(state)) activity = zh ? '生成中…' : 'Gen…';
  if (state === 'idle') activity = zh ? '空闲' : 'Idle';
  if (['waiting', 'awaiting_approval'].includes(state)) activity = zh ? '等待中' : 'Waiting';
  const parts = [
    speed != null ? speed.toFixed(1) + ' t/s' : activity,
    ttft != null ? ttft.toFixed(1) + 's' : '—',
    '◎ ' + (pct != null ? Math.round(Math.min(100, Math.max(0, pct))) + '%' : '—'),
  ];
  let settings;
  if (input.terminal_width > 0) {
    try {
      settings = JSON.parse(readFileSync(join(base, 'antigravity-cli', 'settings.json'), 'utf8')) || {};
    } catch {
      // The host payload is sufficient when there is no saved CLI profile
    }
  }
  return formatStatusLine(input, parts, settings);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  let input;
  try {
    input = JSON.parse(readFileSync(0, 'utf8'));
  } catch {
    // agy can close stdin before sending its first snapshot
  }
  process.stdout.write(cliStatusLine(input || {}) + '\n');
}
