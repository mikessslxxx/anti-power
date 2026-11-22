// Sidecar API calls; the host preload adds the sidecar token to window.sidecar.fetch
import type { Detail, SessionSummary, Status, Summary } from "./types";

declare global {
  interface Window {
    sidecar?: { fetch: typeof fetch };
  }
}

async function call<T>(path: string, body: object = {}): Promise<T> {
  const request = window.sidecar?.fetch ?? fetch;
  const res = await request(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const data = await res.json().catch(() => null);
  if (!res.ok) throw new Error(data?.error || `${path}: HTTP ${res.status}`);
  if (data === null) throw new Error(`Invalid response from ${path}`);
  return data as T;
}

export const api = {
  status: () => call<Status>("/api/status"),
  sessions: () => call<{ sessions: SessionSummary[] }>("/api/sessions"),
  session: (id: string) => call<Detail | { error: string }>("/api/session", { id }),
  summary: (from?: string, to?: string) => call<Summary>("/api/summary", { from, to }),
};
