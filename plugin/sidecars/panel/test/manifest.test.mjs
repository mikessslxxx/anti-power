import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const PANEL = join(dirname(fileURLToPath(import.meta.url)), '..');
const PLUGIN = join(PANEL, '..', '..');
const REPO = join(PLUGIN, '..');
const json = (path) => JSON.parse(readFileSync(path, 'utf8'));

test('the plugin shares the Anti-Power version', () => {
  const version = json(join(REPO, 'patcher', 'package.json')).version;
  assert.equal(json(join(PLUGIN, 'plugin.json')).version, version);
  assert.equal(json(join(PANEL, 'package.json')).version, version);
});

test('the sidecar runs main.mjs in the side panel', () => {
  const sidecar = json(join(PANEL, 'sidecar.json'));
  assert.deepEqual([sidecar.command, ...sidecar.args], ['node', 'main.mjs']);
  assert.equal(sidecar.ui_config.views[0].entrypoint, 'SIDECAR_UI_ENTRYPOINT_AUX_PANE');
  assert.equal(json(join(PLUGIN, 'plugin.json')).name, 'anti-power-usage');
});
