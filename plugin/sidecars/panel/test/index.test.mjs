import assert from 'node:assert/strict';
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { copyFileSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, renameSync, rmSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { DatabaseSync } from 'node:sqlite';
import { catalogInfo, loadCatalogs } from '../vendor/usage-monitor/backend/pricing-catalog.mjs';
import { computeSqliteWalSig } from '../vendor/usage-monitor/backend/sqlite-client.mjs';
import { foldSession, loadLedger, saveLedger, withLedgerLock } from '../lib/ledger.mjs';
import { UsageIndex } from '../lib/index.mjs';
import { TURN, TURN_COST, writeConversation } from './fixture.mjs';

const BUNDLED_CATALOG = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..', 'pricing-catalog.json');
loadCatalogs([BUNDLED_CATALOG]);

const A = '00000000-0000-4000-8000-00000000000a';
const B = '00000000-0000-4000-8000-00000000000b';
const DAY_MS = 24 * 60 * 60 * 1000;
const close = (actual, expected) =>
  assert.ok(Math.abs(actual - expected) < 1e-9, `${actual} is not ${expected}`);

function setup(turns = [TURN, { ...TURN, atMs: TURN.atMs + DAY_MS, prompt: 'second' }]) {
  let time = Date.now();
  const home = mkdtempSync(join(tmpdir(), 'apu-home-'));
  const app = join(home, '.gemini', 'antigravity');
  const profile = join(home, '.cc-switch', 'agy', 'work', 'antigravity-cli');
  writeConversation(app, A, turns);
  writeConversation(profile, B, [{ ...TURN, prompt: 'other session' }]);
  const ledgerFile = join(home, 'data', 'ledger.json');
  const options = { home, ledgerFile, minIntervalMs: 0, now: () => time };
  const index = new UsageIndex(options);
  return { home, app, profile, ledgerFile, index, options, advance: (ms) => { time += ms; } };
}

function writePhasedCatalog(file, phaseStart) {
  const rate = { input: 4, cache: 0.1, output: 10 };
  writeFileSync(file, JSON.stringify({
    updatedAt: '2099-01-01T00:00:00Z',
    pricing: { 'gemini-4-carbon-medium': { ...rate, short: 'carbon', family: 'Gemini',
      phases: [{ ...rate, input: 6, from: new Date(phaseStart).toISOString() }] } },
    placeholders: {},
  }));
}

function saveBucketOnlyLedger(ledgerFile, file) {
  const { store, client, title, parentId, lastActiveMs, buckets } =
    JSON.parse(readFileSync(ledgerFile, 'utf8')).sessions[A];
  const { sig } = computeSqliteWalSig(file);
  saveLedger(ledgerFile, {
    version: 1,
    sessions: { [A]: { store, client, sig, title, parentId, lastActiveMs, buckets, catalog: catalogInfo.version } },
    deleted: [],
  });
  return { lastActiveMs, buckets };
}

test('records each request with content, tokens, speed, and cost', () => {
  const { index } = setup();
  const { sessions } = index.listSessions();
  assert.deepEqual(
    sessions.map((s) => [s.id, s.store, s.client, s.totals.steps]),
    [
      [A, 'app', 'app', 2],
      [B, 'cc-switch:work', 'cli', 1],
    ]
  );
  close(sessions[0].totals.cost, 2 * TURN_COST);
  assert.deepEqual(sessions[0].models, ['Gemini 4 Carbon (Medium)']);

  const detail = index.session({ id: A });
  assert.equal(detail.turns.length, 2);
  assert.deepEqual(detail.turns.map((t) => t.prompt), [TURN.prompt, 'second']);
  const [request] = detail.turns[0].requests;
  assert.deepEqual(request.content, {
    response: TURN.response,
    thinking: TURN.thinking,
    toolCalls: [],
  });
  assert.equal(request.inputTokens, 10_000);
  assert.equal(request.cacheReadTokens, 50_000);
  assert.equal(request.outputTokens, 600);
  close(request.ttftSec, 1.5);
  close(request.totalTokPerSec, 600 / 4.5);
  close(request.cost, TURN_COST);
  assert.deepEqual(request.rates, { input: 2, cache: 0.1, output: 10 });
  close(request.cacheHitRate, 50_000 / 60_000);
  close(request.cacheSavings, (50_000 / 1e6) * (2 - 0.1));
  close(detail.speed.avgTokPerSec, 600 / 4.5);
  assert.equal(index.session({ id: 'missing' }).error, 'not_found');

  const day = new Date(TURN.atMs);
  const first = `${day.getFullYear()}-${String(day.getMonth() + 1).padStart(2, '0')}-${String(day.getDate()).padStart(2, '0')}`;
  const summary = index.summary({ from: first, to: first });
  assert.equal(summary.total.steps, 2);
  assert.deepEqual(summary.byClient.map((c) => [c.client, c.steps]), [['app', 1], ['cli', 1]]);
  assert.equal(index.summary().total.steps, 3);
});

test('a deleted session leaves only an anonymous deleted row', () => {
  const { app, ledgerFile, index, advance } = setup();
  index.listSessions();
  rmSync(join(app, 'conversations', `${A}.db`));

  const { sessions } = index.listSessions();
  assert.deepEqual(sessions.map((s) => s.id), [B]);
  assert.equal(index.session({ id: A }).error, 'not_found');
  const summary = index.summary();
  assert.equal(summary.deleted.steps, 2);
  close(summary.deleted.cost, 2 * TURN_COST);
  close(summary.total.cost, 3 * TURN_COST);
  assert.deepEqual(summary.sessions.map((s) => s.id), [B]);

  advance(60_000);
  index.refresh();
  const saved = readFileSync(ledgerFile, 'utf8');
  assert.ok(!saved.includes(A));
  assert.ok(!saved.includes(TURN.prompt));
});

test('sessions survive a store or database that cannot be read', () => {
  const { app, index } = setup();
  const before = index.listSessions().sessions.find((s) => s.id === A);

  // A corrupt database keeps the last known totals
  writeFileSync(join(app, 'conversations', `${A}.db`), 'not a database');
  assert.deepEqual(index.listSessions().sessions.find((s) => s.id === A), before);
  assert.equal(index.session({ id: A }).error, 'unreadable');

  // A store that cannot be listed does not count as deleted
  rmSync(join(app, 'conversations'), { recursive: true });
  writeFileSync(join(app, 'conversations'), '');
  assert.ok(index.listSessions().sessions.some((s) => s.id === A));
  assert.equal(index.summary().deleted.steps, 0);
});

test('a newer downloaded catalog reprices sessions, and an older one is ignored', (t) => {
  const home = mkdtempSync(join(tmpdir(), 'apu-home-'));
  writeConversation(join(home, '.gemini', 'antigravity'), A, [TURN]);
  const downloaded = join(home, 'pricing-catalog.download.json');
  const base = JSON.parse(readFileSync(BUNDLED_CATALOG, 'utf8'));
  t.after(() => loadCatalogs([BUNDLED_CATALOG]));
  const catalog = (updatedAt, input) => ({
    ...base,
    updatedAt,
    pricing: { ...base.pricing, 'gemini-4-carbon-medium': { input, cache: 0.1, output: 10, short: 'carbon-4', family: 'Gemini' } },
  });
  writeFileSync(downloaded, JSON.stringify(catalog('2000-01-01T00:00:00Z', 100)));
  const index = new UsageIndex({
    home,
    ledgerFile: join(home, 'ledger.json'),
    catalogFiles: [BUNDLED_CATALOG, downloaded],
    minIntervalMs: 0,
  });
  close(index.listSessions().sessions[0].totals.cost, TURN_COST);

  writeFileSync(downloaded, JSON.stringify(catalog('2999-01-01T00:00:00Z', 4)));
  // 10k new input now costs $4 per million instead of $2
  close(index.listSessions().sessions[0].totals.cost, TURN_COST + 0.02);
  writeFileSync(downloaded, JSON.stringify(catalog('2999-01-01T00:00:00Z', 6)));
  close(index.summary().total.cost, TURN_COST + 0.04);
  close(index.session({ id: A }).totals.cost, TURN_COST + 0.04);
});

test('a hot catalog outage uses the last prices until a catalog is restored', (t) => {
  const { home, app, options, advance } = setup([TURN]);
  const catalog = join(home, 'catalog.json');
  copyFileSync(BUNDLED_CATALOG, catalog);
  t.after(() => loadCatalogs([BUNDLED_CATALOG]));
  const index = new UsageIndex({ ...options, catalogFiles: [catalog] });
  close(index.summary().total.cost, 2 * TURN_COST);
  const loadedCatalog = index.status().catalog;

  // A placeholder-only catalog has no usable prices
  writeFileSync(catalog, JSON.stringify({ placeholders: {
    MODEL_PLACEHOLDER_M437: { modelId: 'unpriced-model', label: 'Unknown' },
  } }));
  writeConversation(app, '00000000-0000-4000-8000-00000000000c', [TURN]);
  assert.match(index.status().error, /Using the last loaded prices/);
  assert.deepEqual(index.status().catalog, loadedCatalog);
  close(index.summary().total.cost, 3 * TURN_COST);
  rmSync(join(app, 'conversations', A + '.db'));
  index.refresh();
  advance(60_000);
  close(index.summary().deleted.cost, TURN_COST);
  copyFileSync(BUNDLED_CATALOG, catalog);
  assert.equal(index.status().error, '');
});

test('a temporary absence survives restart and a pending cleanup stage', () => {
  const { options, app, index, advance } = setup();
  const file = join(app, 'conversations', A + '.db');
  index.refresh();
  renameSync(file, file + '.moving');
  assert.deepEqual(index.listSessions().sessions.map((s) => s.id), [B]);
  advance(45_000);
  const second = new UsageIndex(options);
  assert.equal(second.summary().total.steps, 3);
  const stage = join(app, '.anti-power-clean-123456abcdef');
  mkdirSync(stage);
  advance(60_000);
  second.refresh();
  renameSync(file + '.moving', file);
  rmSync(stage, { recursive: true });
  assert.equal(second.summary().total.steps, 3);
  assert.equal(second.summary().deleted.steps, 0);
});

test('a stale instance accepts an external fold before its next save', () => {
  const { home, app, index, ledgerFile, options } = setup();
  const second = new UsageIndex(options);
  index.refresh();
  second.refresh();
  rmSync(join(app, 'conversations', A + '.db'));
  const ledger = JSON.parse(readFileSync(ledgerFile, 'utf8'));
  foldSession(ledger, A);
  saveLedger(ledgerFile, ledger);
  const other = join(home, '.gemini', 'antigravity-cli');
  mkdirSync(other);
  writeFileSync(join(other, 'conversations'), '');
  writeConversation(app, '00000000-0000-4000-8000-00000000000c', [TURN]);
  assert.equal(second.summary().deleted.steps, 2);
  assert.equal(second.summary().total.steps, 4);
  assert.ok(!readFileSync(ledgerFile, 'utf8').includes(A));
});

test('changing only an annotation updates the searchable title', () => {
  const { index, app } = setup();
  index.refresh();
  mkdirSync(join(app, 'annotations'));
  writeFileSync(join(app, 'annotations', A + '.pbtxt'), 'title: "New title"');
  assert.equal(index.listSessions().sessions.find((s) => s.id === A).title, 'New title');
});

test('a failed write preserves new observations across another writer folding a different session', (t) => {
  const rename = fs.renameSync;
  let blocked;
  t.mock.method(fs, 'renameSync', (from, to) => {
    if (to === blocked) throw new Error('simulated ledger write failure');
    return rename(from, to);
  });
  syncBuiltinESMExports();
  t.after(() => { t.mock.restoreAll(); syncBuiltinESMExports(); });
  for (const suffix of ['.pending', '']) {
    const { app, profile, index, ledgerFile, advance } = setup();
    rmSync(join(profile, 'conversations', B + '.db'));
    assert.equal(index.summary().total.steps, 2);
    writeConversation(profile, B, [TURN]);
    blocked = ledgerFile + suffix;
    assert.match(index.status().error, /simulated ledger write failure/);
    assert.equal(index.summary().total.steps, 3);
    blocked = undefined;
    rmSync(join(app, 'conversations', A + '.db'));
    rmSync(join(profile, 'conversations', B + '.db'));
    withLedgerLock(ledgerFile, () => {
      const latest = loadLedger(ledgerFile);
      foldSession(latest, A);
      saveLedger(ledgerFile, latest);
    });
    assert.equal(index.summary().total.steps, 3);
    advance(60_000);
    assert.equal(index.summary().deleted.steps, 3);
    const saved = readFileSync(ledgerFile, 'utf8');
    assert.ok(!saved.includes(A) && !saved.includes(B));
    assert.ok(!fs.existsSync(ledgerFile + '.pending'));
  }
});

test('a surviving fork stops exposing the deleted parent on scans and detail reads', () => {
  const { app, profile, ledgerFile, index, advance } = setup();
  const copy = new DatabaseSync(join(profile, 'conversations', B + '.db'));
  try {
    const reference = Buffer.concat([Buffer.from([50, 36]), Buffer.from(A)]);
    copy.prepare('INSERT INTO parent_references VALUES (0, ?)').run(reference);
  } finally {
    copy.close();
  }
  assert.equal(index.session({ id: B }).parentId, A);
  rmSync(join(app, 'conversations', A + '.db'));
  index.refresh();
  advance(60_000);
  assert.equal(index.listSessions().sessions[0].parentId, '');
  assert.equal(index.session({ id: B }).parentId, '');
  assert.ok(!readFileSync(ledgerFile, 'utf8').includes(A));
});

test('a shorter copy preserves legacy totals and the price for a day with changing rates', (t) => {
  const { home, app, profile, index, ledgerFile, options } = setup();
  index.refresh();
  const file = join(app, 'conversations', A + '.db');
  const { lastActiveMs, buckets } = saveBucketOnlyLedger(ledgerFile, file);
  writeConversation(profile, A, [TURN]);
  rmSync(file);

  const downloaded = join(home, 'download.json');
  t.after(() => loadCatalogs([BUNDLED_CATALOG]));
  const phaseStart = new Date(lastActiveMs);
  phaseStart.setHours(12, 0, 0, 0);
  writePhasedCatalog(downloaded, phaseStart);
  const restarted = new UsageIndex({ ...options, catalogFiles: [BUNDLED_CATALOG, downloaded] });
  const summary = restarted.summary();
  assert.equal(summary.total.steps, 3);
  close(summary.total.cost, 3 * TURN_COST + 0.04);
  close(summary.byDay.find((day) => day.day === buckets[1].day).cost, TURN_COST);
  assert.equal(restarted.listSessions().sessions.find((s) => s.id === A).lastActiveMs, lastActiveMs);
});

test('a single-source legacy ledger covers incomplete requests from a shorter replica only once', () => {
  const { app, profile, index, ledgerFile, options } = setup();
  index.refresh();
  const file = join(app, 'conversations', A + '.db');
  const { buckets } = saveBucketOnlyLedger(ledgerFile, file);
  assert.deepEqual(buckets.map((bucket) => bucket.client), ['app', 'app']);
  const incomplete = { ...TURN, model: null, omitRequestTime: true };
  writeConversation(profile, A, [incomplete]);
  rmSync(file);
  const restarted = new UsageIndex(options);
  const totals = () => restarted.listSessions().sessions.find((s) => s.id === A).totals;
  assert.equal(totals().steps, 2);
  assert.equal(totals().outputTokens, 1200);
  assert.equal(totals().unpricedSteps, 0);
  close(totals().cost, 2 * TURN_COST);
  assert.equal(restarted.summary().byDay.some((bucket) => bucket.day === 'unknown'), false);

  const copy = join(profile, 'conversations', A + '.db');
  rmSync(copy);
  writeConversation(profile, A, [
    incomplete,
    { ...TURN, atMs: TURN.atMs + DAY_MS },
    { ...TURN, atMs: TURN.atMs + 2 * DAY_MS, model: null },
    { ...incomplete, atMs: TURN.atMs + 3 * DAY_MS },
  ]);
  assert.equal(totals().steps, 4);
  assert.equal(totals().unpricedSteps, 2);
  close(totals().cost, 2 * TURN_COST);
  assert.deepEqual(new UsageIndex(options).listSessions().sessions.find((s) => s.id === A).totals, totals());
});

test('legacy migration matches partial requests that know different fields', () => {
  const low = { ...TURN, model: 1438 };
  const { app, profile, index, ledgerFile, options } =
    setup([TURN, low, TURN, { ...low, atMs: TURN.atMs + DAY_MS }]);
  index.refresh();
  const file = join(app, 'conversations', A + '.db');
  const { buckets } = saveBucketOnlyLedger(ledgerFile, file);
  writeConversation(profile, A, [TURN, { ...low, model: null }, { ...TURN, omitRequestTime: true }]);
  rmSync(file);

  const session = new UsageIndex(options).listSessions().sessions.find((s) => s.id === A);
  assert.equal(session.totals.steps, 4);
  assert.equal(session.totals.outputTokens, 2400);
  assert.equal(session.totals.unpricedSteps, 0);
  close(session.totals.cost, buckets.reduce((total, bucket) => total + bucket.cost, 0));
});

test('the first scan joins readable copies and falls back when the newest becomes unreadable', () => {
  const { profile, index } = setup();
  writeConversation(profile, A, [{ ...TURN, prompt: 'latest first turn' }]);
  const copy = join(profile, 'conversations', A + '.db');
  utimesSync(copy, 2_100_000_000, 2_100_000_000);
  assert.equal(index.summary().total.steps, 3);
  const detail = index.session({ id: A });
  assert.deepEqual(detail.turns.map((turn) => turn.prompt), ['latest first turn', 'second']);
  close(detail.totals.cost, 2 * TURN_COST);
  writeFileSync(copy, 'temporarily unreadable');
  assert.equal(index.session({ id: A }).turns.length, 2);
  assert.equal(index.listSessions().sessions.find((s) => s.id === A).store, 'app');
});

test('tool arguments are available in detail and stay out of the ledger', () => {
  const { app, ledgerFile, index } = setup();
  const id = '00000000-0000-4000-8000-00000000000c';
  const args = '{"query":"synthetic argument"}';
  writeConversation(app, id, [{ ...TURN, toolCalls: [{ name: 'lookup', arguments: args }] }]);
  const [request] = index.session({ id }).turns[0].requests;
  assert.deepEqual(request.content.toolCalls, [{name: 'lookup', arguments: args, truncated: false}]);
  assert.ok(!readFileSync(ledgerFile, 'utf8').includes('synthetic argument'));
});

test('background collection records and anonymizes requests without panel traffic', (t) => {
  const { app, index, ledgerFile, advance } = setup();
  const id = '00000000-0000-4000-8000-00000000000c';
  t.mock.timers.enable({ apis: ['setInterval'] });
  const timer = index.startCollecting();
  t.after(() => clearInterval(timer));
  const saved = () => JSON.parse(readFileSync(ledgerFile, 'utf8'));
  assert.equal(saved().sessions[A].buckets.length, 2);
  writeConversation(app, id, [TURN]);
  t.mock.timers.tick(10_000);
  assert.equal(saved().sessions[id].buckets[0].steps, 1);
  rmSync(join(app, 'conversations', id + '.db'));
  t.mock.timers.tick(10_000);
  advance(60_000);
  t.mock.timers.tick(10_000);
  assert.equal(saved().sessions[id], undefined);
  close(saved().deleted[0].cost, TURN_COST);
});

test('a partial replica cannot replace completed usage or its available response', () => {
  const { app, profile, options, index, advance, ledgerFile } = setup();
  const partial = { ...TURN, model: null, response: 'partial', usage: { ...TURN.usage, thinking: 0, response: 20 } };
  writeConversation(profile, A, [partial]);
  const path = join(profile, 'conversations', A + '.db');
  const db = new DatabaseSync(path);
  db.exec('UPDATE steps SET status = 1 WHERE idx = 1');
  db.close();
  utimesSync(path, 2_100_000_000, 2_100_000_000);
  const full = index.session({ id: A }).turns[0].requests[0];
  assert.equal(full.outputTokens, 600);
  assert.equal(full.content.response, TURN.response);

  const ledger = JSON.parse(readFileSync(ledgerFile, 'utf8'));
  const recorded = ledger.sessions[A].requests[1];
  delete recorded.modelKey;
  delete recorded.modelDisplayName;
  saveLedger(ledgerFile, ledger);
  rmSync(join(app, 'conversations', A + '.db'));
  const restarted = new UsageIndex(options);
  const surviving = restarted.listSessions().sessions.find((s) => s.id === A);
  assert.equal(surviving.totals.outputTokens, 1200);
  assert.equal(surviving.lastActiveMs, TURN.atMs + DAY_MS);
  const session = restarted.session({ id: A });
  assert.equal(session.totals.steps, 1);
  const detail = session.turns[0].requests[0];
  assert.equal(detail.outputTokens, 600);
  assert.equal(detail.content.response, 'partial');
  assert.equal(detail.model.id, recorded.model);
  assert.equal(detail.model.label, recorded.label);
  close(detail.cost, TURN_COST);
  rmSync(path);
  restarted.refresh();
  advance(60_000);
  const { deleted } = restarted.summary();
  assert.equal(deleted.steps, 2);
  close(deleted.cost, 2 * TURN_COST);
});

test('tied replica usage preserves its known timestamp and model', () => {
  const { app, profile, index, options, ledgerFile } = setup();
  writeConversation(profile, A, [{ ...TURN, model: null, omitRequestTime: true, response: 'newest' }]);
  const copy = join(profile, 'conversations', A + '.db');
  utimesSync(copy, 2_100_000_000, 2_100_000_000);

  function check(current) {
    const session = current.listSessions().sessions.find((s) => s.id === A);
    assert.equal(session.totals.steps, 2);
    close(session.totals.cost, 2 * TURN_COST);
    assert.equal(current.summary().byDay.some((bucket) => bucket.day === 'unknown'), false);
    const request = current.session({ id: A }).turns[0].requests[0];
    assert.equal(request.createdAtMs, TURN.atMs);
    assert.equal(request.model.id, 'gemini-4-carbon-medium');
    assert.equal(request.content.response, 'newest');
    close(request.cost, TURN_COST);
    const saved = JSON.parse(readFileSync(ledgerFile, 'utf8')).sessions[A].requests[1];
    assert.equal(saved.createdAtMs, TURN.atMs);
    assert.equal(saved.model, 'gemini-4-carbon-medium');
  }
  check(index);
  rmSync(join(app, 'conversations', A + '.db'));
  check(new UsageIndex(options));
});

test('retained requests use the new price for their original time, deleted totals keep their price', (t) => {
  const { app, home, profile, options, advance } = setup();
  const downloaded = join(home, 'download.json');
  t.after(() => loadCatalogs([BUNDLED_CATALOG]));
  const index = new UsageIndex({ ...options, catalogFiles: [BUNDLED_CATALOG, downloaded] });
  index.refresh();
  rmSync(join(profile, 'conversations', B + '.db'));
  index.refresh();
  advance(60_000);
  index.refresh();
  rmSync(join(app, 'conversations', A + '.db'));
  writeConversation(profile, A, [TURN]);
  index.refresh();
  writePhasedCatalog(downloaded, TURN.atMs + DAY_MS - 1);
  const summary = index.summary();
  close(summary.deleted.cost, TURN_COST);
  close(summary.sessions[0].totals.cost, 2 * TURN_COST + 0.06);
  assert.equal(index.listSessions().sessions[0].lastActiveMs, TURN.atMs + DAY_MS);
});

test('a linked profile pauses confirmation and its restored database is counted once', () => {
  const { home, index, profile, advance } = setup();
  const source = dirname(profile);
  const moved = join(home, 'relocated');
  index.refresh();
  renameSync(source, moved);
  symlinkSync(moved, source, 'junction');
  assert.equal(index.status().partial.stores, 1);
  advance(90_000);
  assert.equal(index.summary().deleted.steps, 0);
  rmSync(source);
  renameSync(moved, source);
  assert.equal(index.status().partial, null);
  assert.equal(index.summary().total.steps, 3);
  assert.equal(index.summary().deleted.steps, 0);
});

test('a cold scan reports unreadable copies, retries, and accepts a readable empty session', () => {
  const { app, index } = setup();
  const file = join(app, 'conversations', A + '.db');
  renameSync(file, file + '.saved');
  writeFileSync(file, 'invalid SQLite');
  writeConversation(app, '00000000-0000-4000-8000-00000000000c', []);
  assert.deepEqual(index.status().partial, { stores: 0, sessions: 1 });
  assert.equal(index.summary().total.steps, 1);
  rmSync(file);
  renameSync(file + '.saved', file);
  assert.equal(index.status().partial, null);
  assert.equal(index.summary().total.steps, 3);
});

test('rebuilding and anonymizing leave no identity in generated corruption backups', () => {
  const { index, app, ledgerFile, advance } = setup();
  index.refresh();
  const damaged = readFileSync(ledgerFile, 'utf8').slice(0, -3);
  writeFileSync(ledgerFile, damaged);
  writeFileSync(ledgerFile + '.broken-42', damaged);
  writeFileSync(ledgerFile + '.broken-notes', 'user file');
  assert.equal(index.status().rebuilt, true);
  rmSync(join(app, 'conversations', A + '.db'));
  index.refresh();
  advance(60_000);
  close(index.summary().deleted.cost, 2 * TURN_COST);
  const dir = dirname(ledgerFile);
  assert.deepEqual(readdirSync(dir).sort(), ['ledger.json', 'ledger.json.broken-notes', 'ledger.json.lock']);
  assert.ok(!readFileSync(ledgerFile, 'utf8').includes(A));
});
