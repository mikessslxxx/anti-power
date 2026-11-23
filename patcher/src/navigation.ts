export interface SessionTarget {
  kind: 'session';
  provider: 'agy' | 'antigravity' | null;
  id: string;
  profile: string | null;
}

export function parseNavigation(raw: string): SessionTarget | { kind: 'plugins' } | null {
  if (raw === 'antipower://plugins' || raw === 'antipower://plugins/') return { kind: 'plugins' };
  const match = /^antipower:\/\/session\/(?:(agy|antigravity)\/)?([\da-f]{8}(?:-[\da-f]{4}){3}-[\da-f]{12})(?:\?profile=([^&#]+))?$/i.exec(raw);
  if (!match) return null;
  const profile = new URLSearchParams(raw.split('?')[1]).get('profile');
  const provider = (match[1]?.toLowerCase() ?? null) as SessionTarget['provider'];
  if (profile && (provider !== 'agy' || /[\\/\u0000-\u001f]/.test(profile) || profile === '.' || profile === '..')) return null;
  return { kind: 'session', provider, id: match[2].toLowerCase(), profile };
}
