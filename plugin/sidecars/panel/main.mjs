// Sidecar entry: serves the panel and its JSON API inside Antigravity
import { existsSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Response, SidecarApp } from 'sidecar_sdk';
import { UsageIndex } from './lib/index.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const PLUGIN_ROOT = join(HERE, '..', '..');
const HOME = homedir();
const DATA_DIR =
  process.env.ANTIGRAVITY_EXECUTABLE_DATA_DIR ||
  join(HOME, '.gemini', 'antigravity', 'sidecar_data', 'anti-power-usage', 'panel', 'data');

// The host SDK permits unauthenticated requests when no UI token was supplied
if (!process.env.ANTIGRAVITY_SIDECAR_UI_TOKEN?.trim()) {
  throw new Error('Missing Antigravity sidecar UI token');
}

const index = new UsageIndex({
  home: HOME,
  ledgerFile: join(DATA_DIR, 'ledger.json'),
  catalogFiles: [
    join(PLUGIN_ROOT, 'pricing-catalog.json'),
    join(DATA_DIR, 'pricing-catalog.json'),
    join(PLUGIN_ROOT, 'pricing-catalog.download.json'),
  ],
});

const NO_STORE = { 'Cache-Control': 'no-store' };
const MISSING_UI =
  '<!DOCTYPE html><meta charset="utf-8"><p>Anti-Power Usage: the panel UI is missing. Reinstall the plugin from Anti-Power.</p>';
// The UI build emits fixed file names, read on each request so a reinstall takes effect without a restart
const asset = (name, contentType) => () => {
  const file = join(HERE, 'web', name);
  if (!existsSync(file)) {
    return new Response(name === 'index.html' ? MISSING_UI : '', {
      contentType: name === 'index.html' ? 'text/html; charset=utf-8' : contentType,
      status: name === 'index.html' ? 200 : 404,
      headers: NO_STORE,
    });
  }
  return new Response(readFileSync(file), { contentType, headers: NO_STORE });
};

const app = new SidecarApp();
app.page('/', asset('index.html', 'text/html; charset=utf-8'));
app.page('/app.js', asset('app.js', 'application/javascript; charset=utf-8'));
app.page('/app.css', asset('app.css', 'text/css; charset=utf-8'));
// POST only: the SDK checks the sidecar token on non-GET requests, and these return conversation content
const json = (value) => new Response(JSON.stringify(value), {
  contentType: 'application/json; charset=utf-8',
  headers: NO_STORE,
});
app.api('/api/status', () => json(index.status()), 'POST');
app.api('/api/sessions', () => json(index.listSessions()), 'POST');
app.api('/api/session', (data) => json(index.session(data)), 'POST');
app.api('/api/summary', (data) => json(index.summary(data)), 'POST');
await app.run();
index.startCollecting();
