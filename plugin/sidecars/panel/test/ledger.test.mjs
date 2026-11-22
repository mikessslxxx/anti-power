import assert from 'node:assert/strict';
import { mkdtempSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { UsageIndex } from '../lib/index.mjs';
import { emptyLedger, foldSession, loadLedger, saveLedger, withLedgerLock } from '../lib/ledger.mjs';

test('a damaged ledger stays in place until a replacement has been rebuilt', () => {
  const dir = mkdtempSync(join(tmpdir(), 'apu-ledger-'));
  const file = join(dir, 'ledger.json');
  assert.deepEqual(loadLedger(file), emptyLedger());

  writeFileSync(file, '{"version": 1, "sessions": [');
  assert.equal(loadLedger(file), null);
  assert.equal(readFileSync(file, 'utf8'), '{"version": 1, "sessions": [');
  assert.deepEqual(readdirSync(dir), ['ledger.json']);
});

// Anti-Power's Rust fold reads the same fixture; see patcher/src-tauri/src/commands/usage_ledger.rs
test('folding matches the shared fixture', () => {
  const fixture = JSON.parse(
    readFileSync(new URL('./fixtures/ledger-fold.json', import.meta.url), 'utf8')
  );
  const ledger = structuredClone(fixture.input);
  assert.equal(foldSession(ledger, fixture.removedId), true);
  assert.equal(foldSession(ledger, fixture.removedId), false);
  assert.deepEqual(ledger, fixture.expected);
});

test('an unsupported ledger version stays untouched', () => {
  const dir = mkdtempSync(join(tmpdir(), 'apu-ledger-'));
  const path = join(dir, 'ledger.json');
  const future = '{"version":2,"sessions":{},"deleted":[]}';
  writeFileSync(path, future);
  const backup = path + '.broken-42';
  writeFileSync(backup, 'recovery data');
  const index = new UsageIndex({ home: dir, ledgerFile: path });
  assert.match(index.status().error, /Unsupported usage ledger version/);
  assert.equal(readFileSync(backup, 'utf8'), 'recovery data');
  assert.equal(readFileSync(path, 'utf8'), future);
});

test('invalid version-one records preserve recovery files and the readable history', () => {
  const dir = mkdtempSync(join(tmpdir(), 'apu-ledger-'));
  const file = join(dir, 'ledger.json');
  const bucket = { day: '2026-11-18', model: 'carbon', client: 'app', steps: 2, cost: 0.5 };
  const ledger = { ...emptyLedger(), deleted: [null, bucket] };
  const original = JSON.stringify(ledger);
  writeFileSync(file, original);
  const backup = file + '.broken-100';
  writeFileSync(backup, JSON.stringify({ ...ledger, version: 2 }));
  const index = new UsageIndex({ home: dir, ledgerFile: file, minIntervalMs: 0 });
  assert.match(index.status().error, /Invalid usage ledger records/);
  assert.equal(index.summary().deleted.cost, 0.5);
  assert.equal(readFileSync(file, 'utf8'), original);
  writeFileSync(file, JSON.stringify({ ...ledger, deleted: [bucket] }));
  assert.equal(index.status().error, '');
  assert.equal(JSON.parse(readFileSync(backup, 'utf8')).version, 2);
});

test('locked recovery discards only uncommitted write files', () => {
  const dir = mkdtempSync(join(tmpdir(), 'apu-ledger-'));
  const path = join(dir, 'ledger.json');
  writeFileSync(path + '.tmp-1234', 'old title');
  writeFileSync(path + '.tmp-notes', 'user file');
  writeFileSync(join(dir, '.anti-power-ledger-a1b2c3d4e5f6.tmp'), 'interrupted Rust write');
  writeFileSync(join(dir, '.tmpABC123'), JSON.stringify(emptyLedger()));
  writeFileSync(join(dir, '.tmpABC456'), 'unrelated tempfile');
  withLedgerLock(path, () => saveLedger(path, emptyLedger()));
  assert.deepEqual(readdirSync(dir).sort(), ['.tmpABC456', 'ledger.json', 'ledger.json.lock', 'ledger.json.tmp-notes']);
});
