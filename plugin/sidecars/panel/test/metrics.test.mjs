import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
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
