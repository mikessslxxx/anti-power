import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Script } from 'node:vm';
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

test('the packaged panel loads its assets as classic same-pane resources', () => {
  const html = readFileSync(join(PANEL, 'web/index.html'), 'utf8');
  const scripts = [...html.matchAll(/<script\b[^>]*>/gi)].map(([tag]) => tag);
  assert.deepEqual(scripts.map((tag) => tag.match(/src="([^"]+)"/)?.[1]), ['/preload.js', '/app.js']);
  assert.doesNotMatch(html, /\bcrossorigin\b|type=["']module["']/i);
  assert.match(html, /<link\b[^>]*href="\/app\.css"/);
  assert.match(html, /id="app"[\s\S]*<script\b[^>]*src="\/app\.js"/);
  new Script(readFileSync(join(PANEL, 'web/app.js'), 'utf8'));
});
