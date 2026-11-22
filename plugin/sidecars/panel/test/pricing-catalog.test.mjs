import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { UsageIndex } from '../lib/index.mjs';
import { TURN, TURN_COST, writeConversation } from './fixture.mjs';

test('a cold catalog outage keeps recorded estimates and counts new usage', (t) => {
  const home = mkdtempSync(join(tmpdir(), 'apu-pricing-'));
  t.after(() => rmSync(home, { recursive: true, force: true }));
  const store = join(home, '.gemini', 'antigravity');
  const first = '00000000-0000-4000-8000-00000000000a';
  const second = '00000000-0000-4000-8000-00000000000b';
  const firstDb = join(store, 'conversations', first + '.db');
  const catalog = join(home, 'catalog.json');
  const bundled = new URL('../../../pricing-catalog.json', import.meta.url);
  const options = { home, ledgerFile: join(home, 'ledger.json'), catalogFiles: [catalog], minIntervalMs: 0 };

  // Seed the real ledger in a separate process so this sidecar starts without prices in memory
  copyFileSync(bundled, catalog);
  writeConversation(store, first, [TURN]);
  execFileSync(process.execPath, ['--input-type=module', '-e', `
    import { UsageIndex } from './lib/index.mjs';
    const { error } = new UsageIndex(JSON.parse(process.argv[1])).status();
    if (error) throw new Error(error);
  `, JSON.stringify(options)], { cwd: new URL('../', import.meta.url) });

  rmSync(catalog);
  rmSync(firstDb);
  writeConversation(store, first, [TURN, { ...TURN, atMs: TURN.atMs + 120_000 }]);
  writeConversation(store, second, [TURN]);
  let now = Date.now();
  const index = new UsageIndex({ ...options, now: () => now });
  assert.match(index.status().error, /temporarily unavailable/);
  const { total } = index.summary();
  assert.deepEqual([total.steps, total.unpricedSteps, total.cost], [3, 2, TURN_COST]);
  const detail = index.session({ id: first }).totals;
  assert.deepEqual([detail.unpricedSteps, detail.cost], [1, TURN_COST]);

  rmSync(firstDb);
  index.refresh();
  now += 60_000;
  assert.equal(index.summary().deleted.cost, TURN_COST);
  copyFileSync(bundled, catalog);
  assert.equal(index.status().error, '');
  const restored = index.summary().total;
  assert.deepEqual([restored.unpricedSteps, restored.cost], [1, 2 * TURN_COST]);
});
