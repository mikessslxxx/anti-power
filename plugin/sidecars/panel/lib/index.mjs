// Session index: keeps the ledger in step with the session stores and answers API queries
import { renameSync, statSync } from 'node:fs';
import { hasBuiltinNodeSqlite } from '../vendor/usage-monitor/backend/sqlite-client.mjs';
import { catalogInfo, loadCatalogs } from '../vendor/usage-monitor/backend/pricing-catalog.mjs';
import { addTotals, emptyTotals, emptyLedger, foldSession, loadLedger, removeLedgerCopies, saveLedger, withLedgerLock } from './ledger.mjs';
import { listSessionFiles, listStores, regularFile } from './stores.mjs';
import { ACCOUNTING_VERSION, ledgerEntry, readCopies, sessionDetail } from './usage.mjs';

import { sessionMetrics } from './metrics.mjs';

const DAY = /^\d{4}-\d{2}-\d{2}$/;

// External removals require a second complete observation after the grace period
const MISSING_GRACE_MS = 60_000;

// Keep the identity of a missing replica through the same confirmation period as its session
function trackCopies(entry, copies, complete, now) {
  const seen = new Set(copies.map(({ store }) => store.key));
  const known = new Set(seen);
  const missing = {};
  let prior = entry.knownCopies;
  if (!prior) {
    const signature = entry.copies ? JSON.parse(entry.copies) : [];
    prior = [entry.store, ...signature.map(([key]) => key)].filter(Boolean);
  }
  for (const key of prior) {
    if (seen.has(key)) continue;
    let since = entry.missingCopies?.[key];
    if (!complete) since = null;
    else if (since == null || since > now) since = now;
    if (since !== null && now - since >= MISSING_GRACE_MS) continue;
    known.add(key);
    missing[key] = since;
  }
  const keys = [...known].sort();
  const absent = Object.keys(missing).length ? missing : undefined;
  if (JSON.stringify(keys) === JSON.stringify(entry.knownCopies) &&
      JSON.stringify(absent) === JSON.stringify(entry.missingCopies)) return false;
  entry.knownCopies = keys;
  if (absent) entry.missingCopies = absent;
  else delete entry.missingCopies;
  return true;
}

export class UsageIndex {
  #home;
  #ledgerFile;
  #minIntervalMs;
  #ledger;
  #files = new Map();
  #metrics = new Map();
  #lastScanMs = 0;
  #lastError = '';
  #partial = null;
  #rebuilt = false;
  #catalogFiles;
  #catalogSig = null;
  #now;
  #unsaved = new Map();

  // `catalogFiles` are reloaded whenever one of them changes on disk
  constructor({ home, ledgerFile, catalogFiles = [], minIntervalMs = 2000, now = Date.now }) {
    this.#home = home;
    this.#ledgerFile = ledgerFile;
    this.#catalogFiles = catalogFiles;
    this.#minIntervalMs = minIntervalMs;
    this.#now = now;
    this.#ledger = emptyLedger();
  }

  startCollecting() {
    this.refresh({ force: true });
    return setInterval(() => this.refresh(), 10_000).unref();
  }

  #reloadCatalogs() {
    if (!this.#catalogFiles.length) return;
    const sig = this.#catalogFiles
      .map((file) => {
        try {
          const { ctimeNs, size } = statSync(file, { bigint: true });
          return `${ctimeNs}:${size}`;
        } catch {
          return '-';
        }
      })
      .join('|');
    if (sig === this.#catalogSig && !catalogInfo.error) return;
    loadCatalogs(this.#catalogFiles);
    this.#catalogSig = sig;
  }

  // Re-read changed sessions and fold the ones whose database is gone
  refresh({ force = false } = {}) {
    const now = this.#now();
    if (!force && now - this.#lastScanMs < this.#minIntervalMs) return;
    this.#lastScanMs = now;
    // Sessions priced with an older catalog are re-read below
    try {
      this.#reloadCatalogs();
      withLedgerLock(this.#ledgerFile, () => this.#scan(now), this.#home);
      this.#lastError = '';
    } catch (error) {
      if (error.readableLedger) this.#ledger = error.readableLedger;
      this.#lastError = 'Cannot update the usage ledger: ' + error.message;
    }
  }

  #scan(now) {
    const disk = loadLedger(this.#ledgerFile);
    const damaged = disk === null;
    const ledger = disk ?? emptyLedger();
    let dirty = damaged;
    // Retry unsaved per-session observations whose disk entry has not changed since
    for (const [id, { base, entry }] of this.#unsaved) {
      if (base === JSON.stringify(ledger.sessions[id])) {
        ledger.sessions[id] = entry;
        dirty = true;
      }
    }
    const files = new Map();
    const { stores, profilesReadable } = listStores(this.#home);
    let unreadableStores = profilesReadable ? 0 : 1;
    for (const store of stores) {
      const listing = listSessionFiles(store);
      if (!listing.ok) unreadableStores++;
      for (const file of listing.sessions) {
        if (!files.has(file.id)) files.set(file.id, []);
        files.get(file.id).push({ store, file });
      }
    }

    const complete = unreadableStores === 0;
    let unreadableSessions = 0;
    for (const [id, copies] of files) {
      copies.sort((a, b) => b.file.mtimeMs - a.file.mtimeMs);
      const signature = JSON.stringify(copies.map(({ store, file }) => [store.key, file.sig]));
      const previous = ledger.sessions[id];
      if (previous && trackCopies(previous, copies, complete, now)) dirty = true;
      if (previous?.missingSince !== undefined) {
        delete previous.missingSince;
        dirty = true;
      }
      if (
        previous &&
        previous.accountingVersion === ACCOUNTING_VERSION && previous.copies === signature &&
        (catalogInfo.digest === null || previous.catalog === catalogInfo.digest)
      ) {
        continue;
      }
      let readFailed = false;
      const batch = readCopies(copies, false, () => { readFailed = true; });
      if (readFailed) unreadableSessions++;
      if (!batch && (!previous || catalogInfo.digest === null || previous.catalog === catalogInfo.digest)) continue;
      const entry = ledgerEntry(batch?.store, batch?.file, batch?.data, previous);
      entry.knownCopies = previous?.knownCopies ?? copies.map(({ store }) => store.key).sort();
      if (previous?.missingCopies) entry.missingCopies = previous.missingCopies;
      // A null digest lets the first usable catalog reprice requests recorded during a cold outage
      entry.catalog = catalogInfo.digest;
      if (batch && !readFailed) entry.copies = signature;
      // Retry unreadable replicas on the next scan without rewriting identical totals
      if (!entry.copies && previous && JSON.stringify(previous) === JSON.stringify(entry)) continue;
      ledger.sessions[id] = entry;
      dirty = true;
    }
    this.#partial = !complete || unreadableSessions ? {
      stores: unreadableStores, sessions: unreadableSessions,
    } : null;
    const folded = new Map();
    for (const [id, entry] of Object.entries(ledger.sessions)) {
      if (files.has(id)) continue;
      if (!complete) {
        if (entry.missingCopies && trackCopies(entry, [], false, now)) dirty = true;
        if (entry.missingSince != null) {
          entry.missingSince = null;
          dirty = true;
        }
        continue;
      }
      if (entry.missingSince == null || entry.missingSince > now) {
        entry.missingSince = now;
        dirty = true;
      } else if (now - entry.missingSince >= MISSING_GRACE_MS) {
        folded.set(id, entry);
        dirty = foldSession(ledger, id) || dirty;
      }
    }
    for (const entry of Object.values(ledger.sessions)) {
      if (entry.parentId && !ledger.sessions[entry.parentId] && !files.has(entry.parentId)) {
        entry.parentId = '';
        dirty = true;
      }
    }
    for (const id of this.#metrics.keys()) {
      if (!files.has(id)) this.#metrics.delete(id);
    }
    this.#files = files;
    this.#ledger = ledger;
    if (damaged && this.#partial) {
      throw new Error('Cannot rebuild a damaged ledger until the stores are readable');
    }
    if (dirty) {
      try {
        saveLedger(this.#ledgerFile, ledger);
        if (damaged) this.#rebuilt = true;
      } catch (error) {
        if (error.snapshotSaved) {
          this.#unsaved.clear();
        } else {
          const original = loadLedger(this.#ledgerFile)?.sessions ?? {};
          this.#unsaved.clear();
          for (const [id, entry] of [...Object.entries(ledger.sessions), ...folded]) {
            const base = JSON.stringify(original[id]);
            if (JSON.stringify(entry) !== base) this.#unsaved.set(id, { base, entry });
          }
        }
        throw error;
      }
    } else if (regularFile(this.#ledgerFile + '.pending', true)) {
      renameSync(this.#ledgerFile + '.pending', this.#ledgerFile);
    }
    this.#unsaved.clear();
    if (!this.#partial && regularFile(this.#ledgerFile, true)) {
      removeLedgerCopies(this.#ledgerFile, 'broken');
    }
  }

  metricsFor(id) {
    const copies = this.#files.get(id);
    if (!copies) return null;
    const signature = JSON.stringify(copies.map(({ store, file }) => [store.key, file.sig]));
    const cached = this.#metrics.get(id);
    if (cached?.signature === signature) return cached.metrics;
    let partial = false;
    const batch = readCopies(copies, false, () => { partial = true; });
    if (!batch) return null;
    const metrics = sessionMetrics(batch.data.rows);
    if (!partial) this.#metrics.set(id, { signature, metrics });
    return metrics;
  }

  status({ id } = {}) {
    this.refresh();
    return {
      metrics: this.metricsFor(id),
      sqlite: hasBuiltinNodeSqlite(),
      catalog: { version: catalogInfo.version, updatedAt: catalogInfo.updatedAt },
      sessions: Object.values(this.#ledger.sessions).filter((s) => s.missingSince === undefined).length,
      error: [this.#lastError, catalogInfo.error]
        .filter(Boolean)
        .map((s) => (s.endsWith('.') ? s : s + '.'))
        .join(' '),
      partial: this.#partial,
      rebuilt: this.#rebuilt,
    };
  }

  listSessions() {
    this.refresh();
    const sessions = Object.entries(this.#ledger.sessions).filter(([, s]) => s.missingSince === undefined).map(([id, entry]) => {
      const totals = entry.buckets.reduce((sum, bucket) => addTotals(sum, bucket), emptyTotals());
      const models = [...new Set(entry.buckets.map((bucket) => bucket.label))];
      return {
        id,
        store: entry.store,
        client: entry.client,
        title: entry.title ?? '',
        parentId: this.#ledger.sessions[entry.parentId]?.missingSince === undefined ? entry.parentId : '',
        lastActiveMs: entry.lastActiveMs,
        models,
        totals,
      };
    });
    sessions.sort((a, b) => b.lastActiveMs - a.lastActiveMs);
    return { sessions };
  }

  // Requests of one session with full content, grouped by turn
  session({ id } = {}) {
    this.refresh();
    const key = String(id || '').toLowerCase();
    const found = this.#files.get(key);
    if (!found) return { error: 'not_found' };
    const batch = readCopies(found, true);
    if (!batch) return { error: 'unreadable' };
    const detail = sessionDetail(key, batch.store, batch.data, this.#ledger.sessions[key]?.requests);
    if (!this.#files.has(detail.parentId)) detail.parentId = '';
    return detail;
  }

  // Totals for an inclusive day range; deleted sessions count in totals and in `deleted`
  summary({ from, to } = {}) {
    this.refresh();
    const inRange = (day) =>
      (!DAY.test(from || '') || day >= from) && (!DAY.test(to || '') || day <= to);
    const total = emptyTotals();
    const deleted = emptyTotals();
    const byDay = new Map();
    const byModel = new Map();
    const byClient = new Map();
    const sessions = [];
    const add = (map, key, extra, bucket) => {
      if (!map.has(key)) map.set(key, { ...extra, ...emptyTotals() });
      addTotals(map.get(key), bucket);
    };
    const count = (bucket) => {
      addTotals(total, bucket);
      add(byDay, bucket.day, { day: bucket.day }, bucket);
      add(byModel, bucket.model, { model: bucket.model, label: bucket.label }, bucket);
      add(byClient, bucket.client, { client: bucket.client }, bucket);
    };
    for (const [id, entry] of Object.entries(this.#ledger.sessions)) {
      const totals = emptyTotals();
      for (const bucket of entry.buckets) {
        if (!inRange(bucket.day)) continue;
        count(bucket);
        addTotals(totals, bucket);
      }
      if (entry.missingSince !== undefined) addTotals(deleted, totals);
      else if (totals.steps > 0) sessions.push({ id, title: entry.title, client: entry.client, totals });
    }
    for (const bucket of this.#ledger.deleted) {
      if (!inRange(bucket.day)) continue;
      count(bucket);
      addTotals(deleted, bucket);
    }
    const sorted = (map, compare) => [...map.values()].sort(compare);
    sessions.sort((a, b) => b.totals.cost - a.totals.cost);
    return {
      total,
      deleted,
      sessions,
      byDay: sorted(byDay, (a, b) => a.day.localeCompare(b.day)),
      byModel: sorted(byModel, (a, b) => b.cost - a.cost),
      byClient: sorted(byClient, (a, b) => b.cost - a.cost),
    };
  }
}
