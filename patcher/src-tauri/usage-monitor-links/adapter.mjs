// The host opens only typed local navigation targets. Standalone browsers use their own URL handler
import { execFile } from 'node:child_process';
import { cp, lstat, mkdir, readFile, realpath, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

function navigationUrl(raw) {
  if (raw === 'antipower://plugins' || raw === 'antipower://plugins/') return 'antipower://plugins';
  if (typeof raw !== 'string' || !/^antipower:\/\/session\/(?:(?:agy|antigravity)\/)?[a-f\d]{8}(?:-[a-f\d]{4}){3}-[a-f\d]{12}(?:\?profile=[^&#]+)?$/i.test(raw)) return null;
  const url = new URL(raw);
  const profile = url.searchParams.get('profile');
  if (profile && (!url.pathname.startsWith('/agy/') || /[\\/\x00-\x1f]/.test(profile) || profile === '.' || profile === '..')) return null;
  // Rebuild the URL rather than passing user-provided text to the OS
  const target = new URL('antipower://session' + url.pathname.toLowerCase());
  if (profile) target.searchParams.set('profile', profile);
  return target.href;
}

export async function openTarget(data, local, run = promisify(execFile), platform = process.platform) {
  function reply(status, body) {
    return { status, contentType: 'application/json', body: JSON.stringify(body) };
  }
  if (!local) return reply(403, { error: 'Open Anti-Power from a local panel' });
  const url = navigationUrl(data?.url);
  if (!url) return reply(400, { error: 'Invalid Anti-Power link' });
  try {
    if (platform === 'darwin') await run('/usr/bin/open', [url]);
    else if (platform === 'win32') await run('rundll32.exe', ['url.dll,FileProtocolHandler', url]);
    else await run('xdg-open', [url]);
    return reply(200, { ok: true });
  } catch (err) {
    return reply(500, { error: err.message });
  }
}

// Patch the upstream updater's staging tree using the same hooks as the desktop installer
export async function prepareUpdate(extractedRoot) {
  const assets = dirname(fileURLToPath(import.meta.url));
  const root = await realpath(extractedRoot);
  const panel = join(root, 'sidecars', 'panel');
  const manifest = JSON.parse(await readFile(join(root, 'plugin.json'), 'utf8'));
  if (manifest.name !== 'usage-monitor' || await realpath(panel) !== panel) throw Error('Invalid Usage Monitor archive');
  const hooks = JSON.parse(await readFile(join(assets, 'hooks.json'), 'utf8'));
  for (const hook of hooks) {
    const file = resolve(panel, hook.path);
    if (await realpath(file) !== file || !(await lstat(file)).isFile()) throw Error('Invalid Usage Monitor file: ' + hook.path);
    const source = await readFile(file, 'utf8');
    const matches = [...source.matchAll(new RegExp(hook.before || hook.after, 'g'))];
    if (matches.length !== 1) throw Error('Incompatible Anti-Power link hook: ' + hook.path);
    const code = hook.code === '@panel' ? await readFile(join(assets, 'panel.js'), 'utf8') : hook.code;
    const pos = matches[0].index + (hook.after ? matches[0][0].length : 0);
    await writeFile(file, source.slice(0, pos) + '\n' + code + '\n' + source.slice(pos));
  }
  const dest = join(panel, 'anti-power');
  await mkdir(dest);
  for (const name of ['hooks.json', 'panel.js', 'adapter.mjs']) {
    await cp(join(assets, name), join(dest, name), { force: false, errorOnExist: true });
  }
}
