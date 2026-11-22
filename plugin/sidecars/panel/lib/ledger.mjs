// Persistent usage ledger: per-day/model buckets for live sessions, folded into
// anonymous `deleted` buckets once a session database is gone
import { closeSync, mkdirSync, openSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync, fsyncSync } from 'node:fs';
import { getNodeSqliteDatabaseSync } from '../vendor/usage-monitor/backend/sqlite-client.mjs';
import { basename, dirname, isAbsolute, join, relative, sep } from 'node:path';
import { checkDirectory, regularFile } from './stores.mjs';

export const LEDGER_VERSION = 1;
export const COUNTERS = [
  'steps',
  'inputTokens',
  'cacheReadTokens',
  'outputTokens',
  'thinkingTokens',
  'unpricedSteps',
];

export function emptyLedger() {
  return { version: LEDGER_VERSION, sessions: {}, deleted: [] };
}

export function emptyTotals() {
  return Object.fromEntries([...COUNTERS, 'cost'].map((key) => [key, 0]));
}

export function addTotals(target, source) {
  for (const key of COUNTERS) {
    target[key] += Number(source?.[key]) || 0;
  }
  target.cost += Number(source?.cost) || 0;
  return target;
}

function bucketKey(bucket) {
  return `${bucket.day}\u0000${bucket.model}\u0000${bucket.client}`;
}

export function mergeBuckets(list, additions) {
  const byKey = new Map(list.map((bucket) => [bucketKey(bucket), bucket]));
  for (const bucket of additions) {
    const key = bucketKey(bucket);
    const existing = byKey.get(key);
    if (existing) {
      addTotals(existing, bucket);
    } else {
      const copy = {
        day: bucket.day,
        model: bucket.model,
        label: bucket.label,
        client: bucket.client,
        ...addTotals(emptyTotals(), bucket),
      };
      list.push(copy);
      byKey.set(key, copy);
    }
  }
  return list;
}

// Move a session's totals into the deleted buckets; returns whether it existed
export function foldSession(ledger, id) {
  const session = ledger.sessions[id];
  if (!session) return false;
  if (!Array.isArray(session.buckets)) throw new Error('Invalid usage buckets');
  mergeBuckets(ledger.deleted, session.buckets);
  delete ledger.sessions[id];
  for (const child of Object.values(ledger.sessions)) {
    if (child.parentId === id) child.parentId = '';
  }
  return true;
}

function record(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

const numericFields = [...COUNTERS, 'cost'];
function validBucket(bucket) {
  return record(bucket) && ['day', 'model', 'client'].every((key) => typeof bucket[key] === 'string') &&
    (bucket.label == null || typeof bucket.label === 'string') &&
    numericFields.every((key) => !(key in bucket) || (Number.isFinite(bucket[key]) && bucket[key] >= 0));
}

const validCopyKey = (key) => typeof key === 'string' && (key === 'app' || key === 'cli' ||
  (key.startsWith('cc-switch:') && key.length > 'cc-switch:'.length));

function validSession(entry) {
  return record(entry) && Array.isArray(entry.buckets) && entry.buckets.every(validBucket) &&
    (entry.legacy === undefined || (Array.isArray(entry.legacy) && entry.legacy.every(validBucket))) &&
    (entry.requests === undefined || (record(entry.requests) && Object.entries(entry.requests).every(([step, request]) =>
      /^\d+$/.test(step) && validBucket(request) &&
      (request.createdAtMs === undefined || (Number.isFinite(request.createdAtMs) && request.createdAtMs >= 0))))) &&
    (entry.knownCopies === undefined || (Array.isArray(entry.knownCopies) && entry.knownCopies.every(validCopyKey))) &&
    (entry.missingCopies === undefined || (record(entry.missingCopies) && Object.entries(entry.missingCopies)
      .every(([key, since]) => validCopyKey(key) && (since === null || Number.isFinite(since) && since >= 0)))) &&
    (entry.title === undefined || typeof entry.title === 'string');
}

function recognizableLedger(value) {
  return value?.version === LEDGER_VERSION && record(value.sessions) && Array.isArray(value.deleted) &&
    value.deleted.every(validBucket) && Object.values(value.sessions).every(validSession);
}

function readLedgerJson(file) {
  const text = readFileSync(file, 'utf8');
  try { return JSON.parse(text); } catch { return undefined; }
}

// The caller replaces damaged data atomically after rebuilding from the stores
export function loadLedger(file) {
  let value;
  try {
    const pending = file + '.pending';
    value = readLedgerJson(regularFile(pending, true) ? pending : file);
  } catch (error) {
    if (error?.code === 'ENOENT') return emptyLedger();
    throw error;
  }
  if (value === undefined) return null;
  if (value?.version !== undefined && value.version !== LEDGER_VERSION) {
    throw new Error('Unsupported usage ledger version: ' + value.version);
  }
  if (recognizableLedger(value)) return value;
  if (value?.version !== LEDGER_VERSION || !record(value.sessions) || !Array.isArray(value.deleted)) {
    throw new Error('Invalid usage ledger structure');
  }
  // Keep the original and its backups for recovery, and show the records we can read
  const error = new Error('Invalid usage ledger records. The original ledger was preserved.');
  error.readableLedger = { version: LEDGER_VERSION,
    sessions: Object.fromEntries(Object.entries(value.sessions).filter(([, entry]) => validSession(entry))),
    deleted: value.deleted.filter(validBucket) };
  throw error;
}

export function saveLedger(file, ledger) {
  mkdirSync(dirname(file), { recursive: true });
  const tmp = `${file}.tmp-${process.pid}`;
  const fd = openSync(tmp, 'wx', 0o600);
  try {
    try {
      writeFileSync(fd, JSON.stringify(ledger) + '\n');
      fsyncSync(fd);
    } finally {
      closeSync(fd);
    }
    // Once complete, the snapshot is authoritative for both writers, even if publication fails
    renameSync(tmp, file + '.pending');
    try {
      renameSync(file + '.pending', file);
    } catch (error) {
      error.snapshotSaved = true;
      throw error;
    }
  } finally {
    rmSync(tmp, { force: true });
  }
}

// Verify generic `.tmpXXXXXX` files from older Rust builds before removing
export function removeLedgerCopies(file, kind) {
  const dir = dirname(file);
  const prefix = basename(file) + '.' + kind + '-';
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (!entry.isFile()) continue;
    const path = join(dir, entry.name);
    const numbered = entry.name.startsWith(prefix) && /^\d+$/.test(entry.name.slice(prefix.length));
    if (!numbered) {
      if (kind !== 'tmp') continue;
      if (/^\.tmp[A-Za-z0-9]{6}$/.test(entry.name)) {
        if (!recognizableLedger(readLedgerJson(path))) continue;
      } else if (!/^\.anti-power-ledger-[A-Za-z0-9]{12}\.tmp$/.test(entry.name)) continue;
    }
    if (kind === 'broken') {
      const version = readLedgerJson(path)?.version;
      if (version !== undefined && version !== LEDGER_VERSION) continue;
    }
    rmSync(path);
  }
}

// Serialize writers via an empty SQLite lock file, which the OS releases on process exit
export function withLedgerLock(file, action, home) {
  const Database = getNodeSqliteDatabaseSync();
  if (!Database) throw new Error('The usage ledger requires node:sqlite');
  const dir = dirname(file);
  const rel = home === undefined ? null : relative(home, dir);
  if (rel !== null && !isAbsolute(rel) && rel !== '..' && !rel.startsWith('..' + sep)) {
    checkDirectory(home, dir, true);
  } else {
    mkdirSync(dir, { recursive: true });
  }
  for (const suffix of ['', '-wal', '-shm', '-journal']) regularFile(file + '.lock' + suffix, true);
  const lock = new Database(file + '.lock');
  try {
    lock.exec('PRAGMA busy_timeout=5000');
    lock.exec('BEGIN IMMEDIATE');
    regularFile(file, true);
    removeLedgerCopies(file, 'tmp');
    return action();
  } finally {
    lock.close();
  }
}
