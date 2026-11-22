/**
 * @fileoverview 零业务依赖的本地 SQLite + WAL 非阻塞只读短连接原语 (`sqlite-client.mjs`)。
 *
 * 核心能力：
 * 1. 惰性加载 Node 22.5+ 内置 `node:sqlite`，自动静默实验特性告警，避免污染 CLI / MCP `stdio`；
 * 2. 组合主库 `.db` 与 `.db-wal` 的 `mtime + size` 生成复合签名（忽略易抖动的 `-shm` 共享内存头），精准感知未 checkpoint 的实时写入；
 * 3. 采用短连接 + `PRAGMA busy_timeout = 1000` + `SQLITE_BUSY` 瞬时锁重试并在 `finally` 中立即释放句柄，绝不阻塞宿主 WAL checkpoint。
 */

import { existsSync, statSync } from 'node:fs';

export const SQLITE_IN_CHUNK_SIZE = 400;

let sqliteResolved = false;
let sqliteDatabaseSyncCtor = null;

/**
 * 惰性获取 Node 内置 `node:sqlite` 的 `DatabaseSync` 构造器（兼容测试沙箱注入的 `globalThis.DatabaseSync`）。
 * 首次加载时静默过滤 Node 22 的实验特性告警；当前运行时不支持时返回 `null`。
 *
 * @returns {Function | null}
 */
export function getNodeSqliteDatabaseSync() {
  if (typeof globalThis.DatabaseSync === 'function') {
    return globalThis.DatabaseSync;
  }
  if (sqliteResolved) return sqliteDatabaseSyncCtor;
  sqliteResolved = true;
  if (typeof process === 'undefined' || typeof process.getBuiltinModule !== 'function') {
    return null;
  }
  const origEmitWarning = process.emitWarning;
  try {
    if (typeof origEmitWarning === 'function') {
      process.emitWarning = function filteredSqliteWarning(warning, ...rest) {
        const msg = typeof warning === 'string' ? warning : String(warning?.message || '');
        if (/SQLite is an experimental feature/i.test(msg)) return;
        return origEmitWarning.call(this, warning, ...rest);
      };
    }
    const mod = process.getBuiltinModule('node:sqlite');
    sqliteDatabaseSyncCtor = typeof mod?.DatabaseSync === 'function' ? mod.DatabaseSync : null;
  } catch {
    sqliteDatabaseSyncCtor = null;
  } finally {
    if (typeof origEmitWarning === 'function') {
      process.emitWarning = origEmitWarning;
    }
  }
  return sqliteDatabaseSyncCtor;
}

/**
 * 当前运行时是否具备内置 `node:sqlite` 读取能力。
 *
 * @returns {boolean}
 */
export function hasBuiltinNodeSqlite() {
  return Boolean(getNodeSqliteDatabaseSync());
}

/**
 * 计算 SQLite 主库与 `.db-wal` 预写日志的复合签名（忽略易抖动的 `-shm` 共享内存头），精准感知未 checkpoint 的实时写入。
 *
 * @param {string} dbPath
 * @returns {{ exists: boolean, mtimeMs: number, sig: string }}
 */
export function computeSqliteWalSig(dbPath) {
  if (!dbPath || typeof dbPath !== 'string' || !existsSync(dbPath)) {
    return { exists: false, mtimeMs: 0, sig: '' };
  }
  let dbStat = null;
  try {
    dbStat = statSync(dbPath);
  } catch {
    return { exists: false, mtimeMs: 0, sig: '' };
  }
  if (!dbStat) return { exists: false, mtimeMs: 0, sig: '' };

  let walStat = null;
  const walPath = `${dbPath}-wal`;
  if (existsSync(walPath)) {
    try {
      walStat = statSync(walPath);
    } catch {
      walStat = null;
    }
  }
  const dbMtimeMs = Number(dbStat.mtimeMs || 0);
  const dbSize = Number(dbStat.size || 0);
  const walMtimeMs = Number(walStat?.mtimeMs || 0);
  const walSize = Number(walStat?.size || 0);
  return {
    exists: true,
    mtimeMs: Math.max(dbMtimeMs, walMtimeMs),
    sig: `${Math.floor(dbMtimeMs)}:${dbSize}|${Math.floor(walMtimeMs)}:${walSize}`,
  };
}

export function isSqliteBusyError(err) {
  const msg = String(err?.message || err?.code || '');
  return /SQLITE_BUSY|database is locked/i.test(msg);
}

/**
 * 以只读模式打开 SQLite 数据库执行查询并在 `finally` 中立即关闭连接，避免长连接阻塞宿主 WAL checkpoint。
 * 遇到瞬时锁竞争（`SQLITE_BUSY`）时自动重试 1 次。
 *
 * @template T
 * @param {string} dbPath
 * @param {(db: any) => T} queryFn
 * @returns {T | null}
 */
export function withReadOnlySqliteDb(dbPath, queryFn) {
  const DatabaseSync = getNodeSqliteDatabaseSync();
  if (!DatabaseSync) return null;
  for (let attempt = 0; attempt < 2; attempt++) {
    let db = null;
    try {
      db = new DatabaseSync(dbPath, { readOnly: true });
      try {
        db.exec('PRAGMA busy_timeout = 1000;');
      } catch {
        // 兼容最小测试桩
      }
      return queryFn(db);
    } catch (err) {
      if (attempt === 0 && isSqliteBusyError(err)) {
        continue;
      }
      throw err;
    } finally {
      if (db) {
        try {
          db.close();
        } catch {
          // 忽略关闭异常
        }
      }
    }
  }
  return null;
}

/**
 * 归一化 SQLite `TEXT` 时间列（如 `'2026-11-18 08:12:00+00:00'` -> ISO-8601），剔除零值哨兵时间。
 *
 * @param {unknown} value
 * @returns {string}
 */
export function normalizeSqliteIsoTime(value) {
  const raw = String(value || '').trim();
  if (!raw || raw.startsWith('0001-01-01') || raw.startsWith('1970-01-01 00:00:00')) return '';
  return raw.replace(/^(\d{4}-\d{2}-\d{2})\s+/, '$1T').replace(/\+00:00$/, 'Z');
}
