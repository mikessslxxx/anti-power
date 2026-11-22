import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { DatabaseSync } from 'node:sqlite';
import { sessionMetrics } from '../lib/metrics.mjs';
import { cliStatusLine, formatStatusLine } from '../lib/statusline.mjs';
import { TURN, writeConversation } from './fixture.mjs';

test('context counts the latest window and retains only static tokens plus the checkpoint after compaction', () => {
  const request = { reportedContextLimitTokens: 1000, contextBreakdown: { userInputTokens: 120,
    systemPromptTokens: 80, toolResultsTokens: 0 }, inputTokens: 90, cacheReadTokens: 10, outputTokens: 30 };
  const before = sessionMetrics([request, request]).context;
  assert.deepEqual([before.used, before.limit, before.breakdown.assistantTokens], [130, 1000, 30]);
  const after = sessionMetrics([request, { isCompactionStep: true, inputTokens: 200, responseTokens: 15 }]).context;
  assert.deepEqual([after.used, after.limit, after.breakdown.checkpointTokens], [55, 1000, 15]);
  assert.equal(sessionMetrics([{ inputTokens: 8, maxContextBudgetTokens: 256000 }]).context.limit, null);
});

test('the CLI prefers a measured zero and never takes speed from a different session', (t) => {
  const base = mkdtempSync(join(tmpdir(), 'anti-power-status-'));
  t.after(() => rmSync(base, { recursive: true, force: true }));
  const id = '11111111-1111-4111-8111-111111111111';
  writeConversation(join(base, 'antigravity-cli'), id, [TURN]);
  const env = { ANTIGRAVITY_GEMINI_DIR: base };
  assert.equal(cliStatusLine({ session_id: id, context_window: { used_percentage: 0 } }, env), '133.3 t/s | 1.5s | ◎ 0%');
  assert.equal(cliStatusLine({ transcript_path: '/brain/' + id + '/transcript.jsonl' }, env), '133.3 t/s | 1.5s | ◎ —');
  assert.equal(cliStatusLine({}, env), '— t/s | — | ◎ —');
});

const parts = ['48.8 t/s', '4.4s', '◎ 19%'];

test('the CLI keeps measured timings during a new request and reads a WAL-only schema', (t) => {
  const base = mkdtempSync(join(tmpdir(), 'anti-power-timing-'));
  t.after(() => rmSync(base, { recursive: true, force: true }));
  const id = '11111111-1111-4111-8111-111111111111';
  const root = join(base, 'antigravity-cli');
  const dir = join(root, 'conversations');
  mkdirSync(dir, { recursive: true });
  const path = join(dir, id + '.db');
  const writer = new DatabaseSync(path);
  const render = () => cliStatusLine({ conversation_id: id, agent_state: 'streaming',
    context_window: { used_percentage: 6 } }, { ANTIGRAVITY_GEMINI_DIR: base, LANG: 'en' });
  try {
    writer.exec('PRAGMA journal_mode=WAL');
    let opens = 0;
    globalThis.DatabaseSync = class extends DatabaseSync {
      constructor(...args) { super(...args); opens++; }
    };
    assert.equal(render(), 'Gen… | — | ◎ 6%');
    assert.equal(opens, 0);
    writer.exec('BEGIN; SELECT count(*) FROM sqlite_schema');
    writeConversation(root, id, [TURN, { ...TURN, atMs: TURN.atMs + 10_000, ttftMs: 3000 }]);
    assert.equal(readFileSync(path).readUInt32BE(40), 0);
    assert.equal(render(), '133.3 t/s | 3.0s | ◎ 6%');
    writer.exec('COMMIT; UPDATE steps SET status = 2 WHERE idx = 3');
    assert.equal(render(), '133.3 t/s | 3.0s | ◎ 6%');
    writer.exec('DELETE FROM gen_metadata WHERE idx = 1');
    assert.equal(render(), '133.3 t/s | 1.5s | ◎ 6%');
  } finally {
    delete globalThis.DatabaseSync;
    writer.close();
  }
});

test('only recent upstream timings for this exact session can fill missing CLI readings', (t) => {
  const base = mkdtempSync(join(tmpdir(), 'anti-power-snapshot-'));
  t.after(() => rmSync(base, { recursive: true, force: true }));
  const id = '11111111-1111-4111-8111-111111111111';
  const dir = join(base, 'antigravity', 'sidecar_data', 'usage-monitor', 'panel', 'data');
  mkdirSync(dir, { recursive: true });
  const save = (cascadeId, updatedAtMs) => writeFileSync(join(dir, 'cli-status-snapshot.json'),
    JSON.stringify({ updatedAtMs, activeConversation: { cascadeId, latestTokPerSec: 34, latestTtftSec: 0 } }));
  const render = () => cliStatusLine({ conversation_id: id, agent_state: 'idle',
    context_window: { used_percentage: 6 } }, { ANTIGRAVITY_GEMINI_DIR: base, LANG: 'zh_CN.UTF-8' });
  save(id, Date.now() - 21_000);
  assert.equal(render(), '空闲 | — | ◎ 6%');
  save('22222222-2222-4222-8222-222222222222', Date.now());
  assert.equal(render(), '空闲 | — | ◎ 6%');
  save(id, Date.now());
  assert.equal(render(), '34.0 t/s | 0.0s | ◎ 6%');
  writeConversation(join(base, 'antigravity-cli'), id, [TURN]);
  assert.equal(render(), '133.3 t/s | 1.5s | ◎ 6%');
});

const host = {
  cycle_mode: 'auto', model: { display_name: 'Gemini 4 Carbon (Medium)', effort: 'EFFORT_MEDIUM' },
};

test('a wide footer centers the metrics and preserves the shortcut, mode, model and effort', () => {
  const line = formatStatusLine({ ...host, terminal_width: 120, vim: { mode: 'normal' } }, parts);
  const center = parts.join(' | ');
  assert.ok(line.startsWith('NORMAL · ? for shortcuts'));
  assert.equal(line.indexOf(center), Math.floor((119 - center.length) / 2));
  assert.ok(line.endsWith('auto · Gemini 4 Carbon · medium'));
  assert.equal(line.length, 119);
});

test('narrow footers retain the model and use terminal cells for wide and combining characters', () => {
  for (const width of [40, 50, 65, 80]) {
    const line = formatStatusLine({ ...host, terminal_width: width }, parts);
    assert.ok(line.length < width);
    assert.ok(line.includes('Gemini 4 Carbon'));
    assert.ok(line.includes('48.8 t/s'));
  }
  const input = { terminal_width: 10, model: { display_name: '模模模型e\u0301XYZ' } };
  assert.equal(formatStatusLine(input, parts), '模模模型…');
  assert.equal(formatStatusLine({ ...input, terminal_width: 11 }, parts), '模模模型e\u0301…');
});

test('the CLI uses its selected profile for the model fallback and preserves a stacked native footer', (t) => {
  const base = mkdtempSync(join(tmpdir(), 'anti-power-footer-'));
  t.after(() => rmSync(base, { recursive: true, force: true }));
  const cli = join(base, 'antigravity-cli');
  mkdirSync(cli);
  const file = join(cli, 'settings.json');
  const env = { ANTIGRAVITY_GEMINI_DIR: base };
  const input = { terminal_width: 100, context_window: { used_percentage: 0 } };
  writeFileSync(file, JSON.stringify({ model: 'Gemini 4 Carbon (High)' }));
  assert.ok(cliStatusLine(input, env).endsWith('Gemini 4 Carbon · high'));
  writeFileSync(file, JSON.stringify({ statusLine: { stack_with_default: true } }));
  const line = cliStatusLine(input, env);
  const metrics = '— t/s | — | ◎ 0%';
  assert.equal(line, ' '.repeat(Math.floor((99 - metrics.length) / 2)) + metrics);
});
