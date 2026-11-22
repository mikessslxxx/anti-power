import type { Totals } from "./types";
import { locale } from "./i18n";

export function compact(n: number | null | undefined): string {
  const v = Number(n) || 0;
  const abs = Math.abs(v);
  if (abs >= 1e9) return `${(v / 1e9).toFixed(2)}B`;
  if (abs >= 1e6) return `${(v / 1e6).toFixed(2)}M`;
  if (abs >= 1e4) return `${(v / 1e3).toFixed(1)}K`;
  return Math.round(v).toLocaleString(locale);
}

export function money(v: number | null | undefined): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return "—";
  if (v === 0) return "$0";
  if (Math.abs(v) < 0.01) return `$${v.toFixed(4)}`;
  if (Math.abs(v) < 100) return `$${v.toFixed(2)}`;
  return `$${Math.round(v).toLocaleString(locale)}`;
}

export function seconds(v: number | null | undefined): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return "—";
  return v >= 100 ? `${Math.round(v)}s` : `${v.toFixed(1)}s`;
}

export function rate(v: number | null | undefined): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return "—";
  return v >= 100 ? String(Math.round(v)) : v.toFixed(1);
}

export function percent(v: number | null | undefined): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return "—";
  return `${Math.round(v * 100)}%`;
}

export function dateTime(ms: number | null | undefined): string {
  if (!ms) return "—";
  return new Date(ms).toLocaleString(locale, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function cacheHit(t: Totals): number | null {
  const all = t.inputTokens + t.cacheReadTokens;
  return all > 0 ? t.cacheReadTokens / all : null;
}

// Local calendar day, matching the sidecar's day buckets
export function localDay(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

export function daysAgo(n: number): string {
  const date = new Date();
  date.setDate(date.getDate() - n);
  return localDay(date);
}
