// Shapes returned by the sidecar API in plugin/sidecars/panel/lib/index.mjs

export interface Totals {
  steps: number;
  inputTokens: number;
  cacheReadTokens: number;
  outputTokens: number;
  thinkingTokens: number;
  unpricedSteps: number;
  cost: number;
}

export type Client = "app" | "cli";

export interface SessionSummary {
  id: string;
  store: string;
  client: Client;
  title: string;
  parentId: string;
  lastActiveMs: number;
  models: string[];
  totals: Totals;
}

export interface Summary {
  total: Totals;
  deleted: Totals;
  sessions: { id: string; title: string; client: Client; totals: Totals }[];
  byDay: (Totals & { day: string })[];
  byModel: (Totals & { model: string; label: string })[];
  byClient: (Totals & { client: Client })[];
}

export interface ToolCall {
  name: string;
  arguments: string;
  truncated: boolean;
}

export interface Request {
  stepIndex: number;
  createdAtMs: number;
  model: { id: string; label: string; short: string };
  responseModel?: string;
  isCompactionStep: boolean;
  isComplete: boolean;
  inputTokens: number;
  cacheReadTokens: number;
  outputTokens: number;
  thinkingTokens: number;
  responseTokens: number;
  ttftSec: number | null;
  totalDurationSec: number | null;
  totalTokPerSec: number | null;
  speedSampleEligible: boolean;
  speedReason: string | null;
  estimatedContextTokens?: number;
  maxContextBudgetTokens?: number;
  contextBreakdown?: Record<string, number> | null;
  cost: number | null;
  rates: { input: number; cache: number; output: number } | null;
  cacheHitRate: number | null;
  cacheSavings: number | null;
  content: { response: string; thinking: string; toolCalls: ToolCall[] };
}

export interface Turn {
  index: number;
  prompt: string;
  atMs: number | null;
  requests: Request[];
  errors: { stepIndex: number; message: string }[];
  totals: Totals;
}

export interface Detail {
  id: string;
  store: string;
  client: Client;
  title: string;
  parentId: string;
  turns: Turn[];
  totals: Totals;
  speed: { avgTokPerSec: number | null; sampleCount: number };
  ttft: { avgSec: number | null; sampleCount: number };
}

export interface SessionMetrics {
  speed: number | null;
  ttft: number | null;
  context: { used: number | null; limit: number | null; breakdown: Record<string, number> | null };
}

export interface Status {
  metrics: SessionMetrics | null;
  partial: { stores: number; sessions: number } | null;
  rebuilt: boolean;
  sqlite: boolean;
  catalog: { version: string | null; updatedAt: string | null };
  sessions: number;
  error: string;
}
