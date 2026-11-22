// Locate Antigravity session stores and the conversation databases inside them
import { lstatSync, readdirSync, mkdirSync } from 'node:fs';
import { join, relative, sep } from 'node:path';

const SESSION_DB = /^([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.db$/i;

function isMissing(error) {
  return error?.code === 'ENOENT';
}

// Roots come from the host home and directory entries, never from an API request
export function checkDirectory(home, dir, create = false) {
  let path = home;
  for (const part of relative(home, dir).split(sep)) {
    path = join(path, part);
    let stat = lstatSync(path, { throwIfNoEntry: false });
    if (create && !stat) {
      try {
        mkdirSync(path, { mode: 0o700 });
      } catch (error) {
        if (error.code !== 'EEXIST') throw error;
      }
      stat = lstatSync(path);
    }
    if (!stat) throw Object.assign(new Error('Missing directory: ' + path), { code: 'ENOENT' });
    if (!stat.isDirectory()) throw new Error('Expected a directory: ' + path);
  }
}

export function regularFile(path, optional = false) {
  try {
    const stat = lstatSync(path, { bigint: true });
    if (!stat.isFile()) throw new Error('Expected a regular file: ' + path);
    return stat;
  } catch (error) {
    if (optional && isMissing(error)) return null;
    throw error;
  }
}

// Repeat path checks on detail reads, which may use a cached directory listing
export function sessionFile(store, name) {
  const dir = join(store.root, 'conversations');
  checkDirectory(store.home, dir);
  const dbPath = join(dir, name);
  const db = regularFile(dbPath);
  const wal = regularFile(dbPath + '-wal', true);
  regularFile(dbPath + '-shm', true);
  regularFile(dbPath + '-journal', true);
  const id = name.slice(0, -3).toLowerCase();
  let annotation;
  try {
    const annotations = join(store.root, 'annotations');
    checkDirectory(store.root, annotations);
    annotation = regularFile(join(annotations, id + '.pbtxt'), true);
  } catch (error) {
    if (!isMissing(error)) throw error;
  }
  const version = (stat) => stat ? [stat.ino, stat.ctimeNs, stat.size].join(':') : '';
  // Keep the original database fingerprint as the prefix for pre-upgrade ledgers
  const legacySig = [db, wal].map((s) => s ? [s.mtimeMs, s.size].join(':') : '0:0').join('|');
  return {
    id, name, dbPath,
    sig: [legacySig, version(db), version(wal), version(annotation)].join('|'),
    mtimeMs: Math.max(Number(db.mtimeNs), Number(wal?.mtimeNs ?? 0n)) / 1e6,
    hasAnnotation: Boolean(annotation),
  };
}

// App store, CLI store, and CC Switch profile CLI stores
export function listStores(home) {
  const gemini = join(home, '.gemini');
  const stores = [
    { home, key: 'app', client: 'app', root: join(gemini, 'antigravity') },
    { home, key: 'cli', client: 'cli', root: join(gemini, 'antigravity-cli') },
  ];
  const profiles = join(home, '.cc-switch', 'agy');
  try {
    checkDirectory(home, profiles);
    for (const entry of readdirSync(profiles, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      // Include linked profiles so the directory check reports an incomplete scan
      if (!entry.isDirectory() && !entry.isSymbolicLink()) continue;
      const { name } = entry;
      const root = join(profiles, name, 'antigravity-cli');
      stores.push({ home, key: `cc-switch:${name}`, client: 'cli', root });
    }
  } catch (error) {
    return { stores, profilesReadable: isMissing(error) };
  }
  return { stores, profilesReadable: true };
}

// `ok` is false when an existing store cannot be listed, so callers do not treat its sessions as deleted
export function listSessionFiles(store) {
  const dir = join(store.root, 'conversations');
  let names;
  try {
    checkDirectory(store.home, store.root);
    if (readdirSync(store.root).some((name) => /^\.anti-power-clean-[a-zA-Z0-9]+$/.test(name))) {
      return { ok: false, sessions: [] };
    }
    if (lstatSync(dir).isSymbolicLink()) return { ok: false, sessions: [] };
    names = readdirSync(dir);
  } catch (error) {
    return { ok: isMissing(error), sessions: [] };
  }
  const sessions = [];
  let ok = true;
  for (const name of names) {
    const match = SESSION_DB.exec(name);
    if (!match) continue;
    try {
      sessions.push(sessionFile(store, name));
    } catch {
      ok = false;
    }
  }
  return { ok, sessions };
}
