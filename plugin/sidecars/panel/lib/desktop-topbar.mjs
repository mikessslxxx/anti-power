// Publish a read-only metric strip to the desktop App launched with loopback debugging
import { readFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join } from 'node:path';

const ROUTE = /^\/c\/([\da-f]{8}(?:-[\da-f]{4}){3}-[\da-f]{12})(?:\/|$)/i;
const POLL_MS = 3000;

function portFile() {
  let config = process.env.XDG_CONFIG_HOME || join(homedir(), '.config');
  if (process.platform === 'darwin') config = join(homedir(), 'Library', 'Application Support');
  if (process.platform === 'win32') config = process.env.APPDATA || join(homedir(), 'AppData', 'Roaming');
  return join(config, 'Antigravity', 'DevToolsActivePort');
}

function metricText(metrics) {
  const value = (n, suffix = '') => Number.isFinite(n) ? n.toFixed(n >= 100 ? 0 : 1) + suffix : '—';
  const { used, limit } = metrics?.context ?? {};
  const pct = Number.isFinite(used) && limit > 0 ? Math.round(100 * Math.min(1, used / limit)) + '%' : '—';
  return value(metrics?.speed) + ' t/s  ·  ' + value(metrics?.ttft, 's') + '  ·  ◎ ' + pct;
}

// Serialized into the desktop renderer via Runtime.evaluate (no outer closures)
function renderTopbar(origin, path, text, placeholder) {
  const config = window.__APP_CONFIG__;
  if (location.origin !== origin || config?.productName?.toLowerCase() !== 'antigravity' ||
      config.isRemote || config.isRemoteControl || config.isSsh || config.remoteAuthority || config.sshHost) return;

  const toggle = document.querySelector('[data-testid="toggle-aux-sidebar"]');
  // The closed side panel's toggle floats above the pane toolbar, which reserves space for it
  let toolbar = toggle?.parentElement;
  if (toggle?.getAttribute('aria-expanded') !== 'true') {
    const panes = Array.from(document.querySelectorAll('[data-testid="pane-titlebar-toolbar"]'))
      .map(el => ({ el, rect: el.getBoundingClientRect() }))
      .filter(({ rect }) => rect.width > 0);
    toolbar = panes.sort((a, b) => b.rect.right - a.rect.right)[0]?.el ?? toolbar;
  }
  if (!toolbar) return;

  let strip = document.getElementById('anti-power-usage-topbar');
  if (!strip) {
    strip = document.createElement('span');
    strip.id = 'anti-power-usage-topbar';
    strip.title = /^zh/i.test(navigator.language)
      ? '输出速度 · 首字延迟 · 上下文占用' : 'Output speed · Time to first token · Context usage';
    strip.style.cssText = 'display:inline-block;min-width:0;overflow:hidden;text-overflow:ellipsis;' +
      'white-space:nowrap;margin-inline:4px;padding:2px 7px;border-radius:6px;' +
      'background:var(--secondary,transparent);color:inherit;font:500 12px/18px system-ui;' +
      'font-variant-numeric:tabular-nums;-webkit-app-region:no-drag';
    strip.resetMetrics = () => { strip.textContent = placeholder; };
    window.navigation?.addEventListener('currententrychange', strip.resetMetrics);
  }
  if (strip.parentElement !== toolbar) toolbar.prepend(strip);
  const next = location.pathname === path ? text : placeholder;
  if (strip.textContent !== next) strip.textContent = next;
  // Auto-remove if the sidecar stops polling
  clearTimeout(strip.expiry);
  strip.expiry = setTimeout(() => {
    window.navigation?.removeEventListener('currententrychange', strip.resetMetrics);
    strip.remove();
  }, 15_000);
}

async function publish(target, port, metricsFor) {
  if (target.type !== 'page' || !/^[a-f\d]{32}$/i.test(target.id)) return;
  const url = new URL(target.url);
  if (url.protocol !== 'https:' || !['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname) || !url.port) return;
  const id = url.pathname.match(ROUTE)?.[1].toLowerCase();
  const placeholder = metricText(null);
  const args = [url.origin, url.pathname, id ? metricText(metricsFor(id)) : placeholder, placeholder];
  const expression = '(' + renderTopbar.toString() + ')(...' + JSON.stringify(args) + ')';
  const socket = new WebSocket('ws://127.0.0.1:' + port + '/devtools/page/' + target.id);
  await new Promise(resolve => {
    const finish = () => {
      clearTimeout(timer);
      socket.close();
      resolve();
    };
    const timer = setTimeout(finish, 2000);
    socket.onopen = () => socket.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression } }));
    socket.onmessage = event => {
      try {
        if (JSON.parse(event.data)?.id !== 1) return;
      } catch {
        // Malformed reply; fall through to finish()
      }
      finish();
    };
    socket.onerror = finish;
    socket.onclose = () => { clearTimeout(timer); resolve(); };
  });
}

export async function updateDesktopTopbar(metricsFor, file = portFile()) {
  let active;
  try {
    active = await readFile(file, 'utf8');
  } catch (error) {
    if (error.code === 'ENOENT') return;
    throw error;
  }
  const [portText, browserPath] = active.trim().split(/\r?\n/);
  const port = Number(portText);
  if (!/^\d+$/.test(portText) || port < 1 || port > 65535 || !browserPath?.startsWith('/devtools/browser/')) return;
  const endpoint = 'http://127.0.0.1:' + port;
  const options = { redirect: 'error', signal: AbortSignal.timeout(2000) };
  const version = await (await fetch(endpoint + '/json/version', options)).json();
  if (new URL(version.webSocketDebuggerUrl).pathname !== browserPath) return;
  const targets = await (await fetch(endpoint + '/json/list', options)).json();
  await Promise.allSettled(targets.map(target => publish(target, port, metricsFor)));
}

export function startDesktopTopbar(metricsFor) {
  async function tick() {
    try {
      await updateDesktopTopbar(metricsFor);
    } catch {
      // The App can close, restart or leave a stale port file between polls
    } finally {
      setTimeout(tick, POLL_MS).unref();
    }
  }
  void tick();
}
