import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { updateDesktopTopbar } from '../lib/desktop-topbar.mjs';

test('the desktop strip follows each window, moves with the sidebar, and expires on disconnect', async t => {
  const dir = await mkdtemp(join(tmpdir(), 'anti-power-topbar-'));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const file = join(dir, 'DevToolsActivePort');
  const port = 41234;
  const browser = '/devtools/browser/test';
  const origin = 'https://127.0.0.1:12345';
  const first = '11111111-1111-1111-1111-111111111111';
  const second = '22222222-2222-2222-2222-222222222222';
  const a = { type: 'page', id: 'a'.repeat(32), url: origin + '/c/' + first };
  const b = { type: 'page', id: 'b'.repeat(32), url: origin + '/c/' + second };
  const targets = [a, b, { ...a, type: 'iframe' }, { ...a, url: 'https://example.com/' }];
  let discovery = 0;
  t.mock.method(globalThis, 'fetch', async url => {
    discovery++;
    assert.ok(url.startsWith('http://127.0.0.1:' + port + '/json/'));
    const data = url.endsWith('/version') ? { webSocketDebuggerUrl: 'ws://127.0.0.1:' + port + browser } : targets;
    return { json: async () => data };
  });

  function renderer(target) {
    let strip;
    const pane = { getBoundingClientRect: () => ({ width: 100, right: 900 }), prepend(el) { strip = el; el.parentElement = this; } };
    const sidebar = { prepend: pane.prepend };
    const toggle = { open: false, parentElement: sidebar, getAttribute: () => String(toggle.open) };
    const navigation = new EventTarget();
    const ctx = {
      location: new URL(target.url), navigator: { language: 'en' },
      window: { navigation, __APP_CONFIG__: { productName: 'antigravity' } },
      document: {
        querySelector: () => toggle, querySelectorAll: () => [pane], getElementById: () => strip,
        createElement: () => ({ style: {}, remove() { strip = undefined; } }),
      },
      setTimeout: fn => fn, clearTimeout() {},
    };
    return { ctx, pane, sidebar, toggle, navigation, strip: () => strip };
  }
  const windows = new Map([[a.id, renderer(a)], [b.id, renderer(b)]]);
  const requests = [];
  let reply = JSON.stringify({ id: 1 });
  const originalSocket = globalThis.WebSocket;
  t.after(() => { globalThis.WebSocket = originalSocket; });
  globalThis.WebSocket = class {
    constructor(url) {
      this.window = windows.get(new URL(url).pathname.split('/').pop());
      queueMicrotask(() => this.onopen());
    }
    send(payload) {
      const packet = JSON.parse(payload);
      requests.push(packet.method);
      runInNewContext(packet.params.expression, this.window.ctx);
      this.onmessage({ data: reply });
    }
    close() {}
  };
  const ids = [];
  const update = () => updateDesktopTopbar(id => {
    ids.push(id);
    return { speed: id === first ? 71 : 0, ttft: 0, context: { used: 10, limit: 100 } };
  }, file);
  await update();
  assert.equal(discovery, 0);
  await writeFile(file, port + '\n/devtools/browser/stale');
  await update();
  assert.equal(requests.length, 0);
  await writeFile(file, port + '\n' + browser);
  await update();
  assert.deepEqual(ids.sort(), [first, second]);
  assert.equal(requests.length, 2);
  reply = 'incomplete reply';
  await update();
  reply = JSON.stringify({ id: 1 });
  const win = windows.get(a.id);
  const strip = win.strip();
  assert.equal(strip.textContent, '71.0 t/s  ·  0.0s  ·  ◎ 10%');
  assert.equal(strip.parentElement, win.pane);
  assert.match(windows.get(b.id).strip().textContent, /^0\.0 t\/s/);
  win.toggle.open = true;
  win.ctx.location = new URL(origin + '/');
  win.navigation.dispatchEvent(new Event('currententrychange'));
  const blank = '— t/s  ·  —  ·  ◎ —';
  assert.equal(strip.textContent, blank);
  await update(); // Discovery still reports the old URL
  assert.equal(strip.textContent, blank);
  assert.equal(strip.parentElement, win.sidebar);
  a.url = origin + '/';
  ids.length = 0;
  await update();
  assert.deepEqual(ids, [second]);
  assert.equal(win.strip(), strip);
  a.url = b.url;
  win.ctx.location = new URL(a.url);
  win.toggle.open = false;
  await update();
  assert.match(strip.textContent, /^0\.0 t\/s/);
  assert.equal(strip.parentElement, win.pane);
  strip.expiry();
  assert.equal(win.strip(), undefined);
});
