// Anti-Power: local-file replacement for upstream backend/pricing-catalog.mjs (drops cloud sync and updates)
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { parseIsoMs } from '../trajectory-metrics.mjs';

export const MODEL_PRICING = Object.create(null);
export const PLACEHOLDER_MAP = Object.create(null);
export const catalogInfo = { version: null, updatedAt: null, digest: null, error: '' };

function normalizeModelFamily(rawFamily) {
  const f = String(rawFamily || '').trim();
  const lower = f.toLowerCase();
  if (lower === 'gemini' || lower === 'google') return 'Gemini';
  if (lower === 'claude' || lower === 'anthropic') return 'Claude';
  if (lower === 'gpt' || lower === 'openai') return 'GPT';
  if (lower === 'other') return 'Other';
  return f;
}

function isValidRates(entry) {
  if (!entry || typeof entry !== 'object') return false;
  const rates = [entry.input, entry.cache, entry.output];
  if (!rates.every((v) => typeof v === 'number' && Number.isFinite(v) && v >= 0 && v <= 10000)) {
    return false;
  }
  return !(entry.input === 0 && entry.output === 0);
}

function sanitizePhase(raw) {
  if (!isValidRates(raw)) return null;
  const bound = (value) => {
    if (value === undefined || value === null) return { ok: true };
    if (typeof value !== 'string' || !value.trim()) return { ok: false };
    const ms = parseIsoMs(value.trim());
    return ms > 0 ? { ok: true, text: value.trim(), ms } : { ok: false };
  };
  const from = bound(raw.from);
  const until = bound(raw.until);
  if (!from.ok || !until.ok || (!from.text && !until.text)) return null;
  const fromMs = from.ms ?? -Infinity;
  const untilMs = until.ms ?? Infinity;
  if (fromMs >= untilMs) return null;
  return {
    ...(from.text ? { from: from.text, fromMs } : {}),
    ...(until.text ? { until: until.text, untilMs } : {}),
    input: raw.input,
    cache: raw.cache,
    output: raw.output,
  };
}

// Validate a catalog payload; returns null when it holds nothing usable
export function sanitizeCatalog(raw) {
  if (!raw || typeof raw !== 'object') return null;
  const pricing = Object.create(null);
  for (const [key, v] of Object.entries(raw.pricing || {})) {
    if (!key.trim() || !isValidRates(v)) continue;
    if (typeof v.short !== 'string' || !v.short.trim()) continue;
    if (typeof v.family !== 'string' || !v.family.trim()) continue;
    const phases = Array.isArray(v.phases) ? v.phases.map(sanitizePhase).filter(Boolean) : [];
    pricing[key.trim()] = {
      input: v.input,
      cache: v.cache,
      output: v.output,
      short: v.short.trim(),
      family: normalizeModelFamily(v.family),
      ...(phases.length ? { phases } : {}),
    };
  }
  const placeholders = Object.create(null);
  for (const [key, v] of Object.entries(raw.placeholders || {})) {
    if (!key.trim() || typeof v?.modelId !== 'string' || typeof v?.label !== 'string') continue;
    if (!v.modelId.trim() || !v.label.trim()) continue;
    placeholders[key.trim()] = { modelId: v.modelId.trim(), label: v.label.trim() };
  }
  if (!Object.keys(pricing).length && !Object.keys(placeholders).length) return null;
  const text = (value) => (typeof value === 'string' && value.trim() ? value.trim() : null);
  return {
    catalogVersion: text(raw.catalogVersion),
    updatedAt: text(raw.updatedAt),
    pricing,
    placeholders,
  };
}

// Load catalogs in `updatedAt` order; keep the last usable rates if every price layer fails
export function loadCatalogs(files) {
  const layers = [];
  for (const file of files) {
    let raw = null;
    try {
      raw = JSON.parse(readFileSync(file, 'utf8'));
    } catch {
      continue;
    }
    const catalog = sanitizeCatalog(raw);
    if (catalog) layers.push(catalog);
  }
  if (!layers.some(({ pricing }) => Object.keys(pricing).length > 0)) {
    catalogInfo.error = catalogInfo.digest
      ? 'Pricing catalogs are temporarily unavailable. Using the last loaded prices.'
      : 'Pricing catalogs are temporarily unavailable.';
    return catalogInfo;
  }
  // A stale downloaded catalog must not override a newer bundled one; ties keep file order
  layers.sort((a, b) => (parseIsoMs(a.updatedAt) || 0) - (parseIsoMs(b.updatedAt) || 0));
  for (const key of Object.keys(MODEL_PRICING)) delete MODEL_PRICING[key];
  for (const key of Object.keys(PLACEHOLDER_MAP)) delete PLACEHOLDER_MAP[key];
  catalogInfo.version = null;
  catalogInfo.updatedAt = null;
  for (const catalog of layers) {
    Object.assign(MODEL_PRICING, catalog.pricing);
    Object.assign(PLACEHOLDER_MAP, catalog.placeholders);
    catalogInfo.version = catalog.catalogVersion || catalogInfo.version;
    catalogInfo.updatedAt = catalog.updatedAt || catalogInfo.updatedAt;
  }
  catalogInfo.digest = createHash('sha256')
    .update(JSON.stringify([MODEL_PRICING, PLACEHOLDER_MAP]))
    .digest('hex');
  catalogInfo.error = '';
  return catalogInfo;
}
