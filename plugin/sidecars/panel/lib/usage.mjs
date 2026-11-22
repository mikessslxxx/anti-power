// Per-request usage, cost, and content for one conversation database
import { sessionFile } from './stores.mjs';
import { join } from 'node:path';
import {
  readAnnotationTitleForCascade,
  readLocalSqliteTrajectory,
} from '../vendor/usage-monitor/backend/cli-sqlite-reader.mjs';
import {
  computeStepCost,
  createStepModelResolver,
} from '../vendor/usage-monitor/backend/model-pricing.mjs';
import {
  extractContextBreakdown,
  extractModelSteps,
  summarizeSpeed,
  summarizeTtft,
} from '../vendor/usage-monitor/trajectory-metrics.mjs';
import { addTotals, emptyTotals, mergeBuckets } from './ledger.mjs';
import { catalogInfo } from '../vendor/usage-monitor/backend/pricing-catalog.mjs';

export const ACCOUNTING_VERSION = 3;
const UNKNOWN_DAY = 'unknown';
// Tool arguments can hold whole files; the panel shows the start
const MAX_ARGUMENT_CHARS = 20_000;

export function localDay(ms) {
  if (!Number.isFinite(ms) || ms <= 0) return UNKNOWN_DAY;
  const date = new Date(ms);
  const pad = (n) => String(n).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function count(value) {
  const n = Number(value);
  return Number.isFinite(n) && n > 0 ? n : 0;
}

function stepIndexOf(step) {
  return step?.metadata?.sourceTrajectoryStepInfo?.stepIndex ?? step?.idx;
}

function compareUsage(candidate, current) {
  if (candidate.finished != null && current.finished != null && candidate.finished !== current.finished) {
    return Number(candidate.finished) - Number(current.finished);
  }
  for (const key of ['outputTokens', 'inputTokens', 'cacheReadTokens', 'thinkingTokens']) {
    const difference = count(candidate[key]) - count(current[key]);
    if (difference !== 0) return difference;
  }
  return 0;
}

// Equal usage can supplement the preferred copy's missing metadata
function withKnownMetadata(row, other) {
  const key = row.modelKey ?? row.model;
  const otherKey = other.modelKey ?? other.model;
  const knownKey = key && key !== 'unknown';
  const createdAtMs = count(row.createdAtMs) || count(other.createdAtMs);
  if (knownKey && otherKey && otherKey !== 'unknown' && key !== otherKey && key !== other.model) {
    return { ...row, createdAtMs };
  }
  return {
    ...row,
    createdAtMs,
    modelKey: knownKey ? key : otherKey || key,
    modelDisplayName: row.modelDisplayName || (other.modelDisplayName ?? other.label),
    responseModel: row.responseModel || other.responseModel,
    apiProvider: row.apiProvider || other.apiProvider,
  };
}

function price(row, resolve, recorded) {
  if (catalogInfo.digest === null && recorded?.model && Number.isFinite(recorded.cost)) {
    const changed = ['inputTokens', 'cacheReadTokens', 'outputTokens'].some((key) =>
      count(row[key]) !== count(recorded[key]));
    return { model: recorded.model, label: recorded.label, short: '', rates: null,
      cost: recorded.unpricedSteps === recorded.steps && !recorded.cost ? null : recorded.cost,
      unpricedSteps: changed ? recorded.steps : recorded.unpricedSteps };
  }
  const { modelId, label, shortName, isEstimatedPrice, pricing } = resolve(row);
  const rates = isEstimatedPrice ? null : pricing;
  return {
    model: modelId,
    label,
    short: shortName || '',
    rates,
    cost: rates ? computeStepCost(row, rates) : null,
  };
}

// Keep request detail tied to its own context metadata before merging replicas
function withRequestContext(rows, { steps, generatorMetadata }) {
  const chats = new Map();
  for (const { chatModel, stepIndices } of generatorMetadata) {
    if (!chatModel) continue;
    for (const index of stepIndices) chats.set(Number(index), chatModel);
  }
  for (const step of steps) {
    const inline = step.metadata?.generatorMetadata?.find((entry) => entry?.chatModel);
    if (inline) chats.set(Number(stepIndexOf(step)), inline.chatModel);
  }
  return rows.map(({ reportedContextLimitTokens, ...row }) => {
    const chat = chats.get(Number(row.stepIndex));
    const context = chat?.chatStartMetadata?.contextWindowMetadata;
    const breakdown = extractContextBreakdown(chat);
    return {
      ...row,
      contextBreakdown: breakdown,
      estimatedContextTokens: count(context?.estimatedTokensUsed) || count(breakdown?.totalTokens) || undefined,
      maxContextBudgetTokens: count(reportedContextLimitTokens) || undefined,
    };
  });
}

function readSession(store, file, fullText) {
  const trajectory = readLocalSqliteTrajectory(file.dbPath, { cascadeId: file.id, fullText, includeEmpty: true });
  if (!trajectory) return null;
  const rows = extractModelSteps(trajectory.steps, trajectory.generatorMetadata);
  const title =
    (file.hasAnnotation && readAnnotationTitleForCascade(file.id, [join(store.root, 'annotations')])) ||
    trajectory.firstOwnUserPrompt ||
    '';
  return { trajectory, rows: fullText ? withRequestContext(rows, trajectory) : rows, title };
}

// Prefer a completed observation over a partial replica, including the corresponding payload
export function readCopies(copies, fullText = false, onUnreadable = () => {}) {
  let primary;
  const steps = new Map();
  const rows = new Map();
  for (const { store, file } of copies) {
    let current;
    let data;
    try {
      current = sessionFile(store, file.name);
      data = readSession(store, current, fullText);
    } catch {
      // The store may change between directory listing and the read
    }
    if (!data) {
      onUnreadable();
      continue;
    }
    const { trajectory } = data;
    if (!trajectory.steps.length && !trajectory.generatorMetadata.length && !trajectory.isSideQuestion) continue;
    primary ??= { store, file: current, data };
    const copySteps = new Map();
    for (const step of trajectory.steps) {
      const index = stepIndexOf(step);
      copySteps.set(index, step);
      if (!steps.has(index)) steps.set(index, step);
    }
    for (const row of data.rows) {
      const step = copySteps.get(row.stepIndex);
      const finished = row.isComplete || step?.status === 'CORTEX_STEP_STATUS_DONE' ||
        step?.status === 'CORTEX_STEP_STATUS_CLEARED';
      const observation = { ...row, finished, client: store.client };
      const known = rows.get(row.stepIndex);
      const difference = known ? compareUsage(observation, known) : 1;
      if (difference > 0) {
        rows.set(row.stepIndex, observation);
        if (step) steps.set(row.stepIndex, step);
      } else if (difference === 0) {
        rows.set(row.stepIndex, withKnownMetadata(known, observation));
      }
    }
  }
  if (!primary) return null;
  const { store, file, data } = primary;
  return {
    store, file,
    data: {
      title: data.title,
      trajectory: { ...data.trajectory, steps: [...steps.values()] },
      rows: [...rows.values()].sort((a, b) => a.stepIndex - b.stepIndex),
    },
  };
}

function ratesChangeWithinDay(rates, start) {
  if (!start || !rates?.phases?.length) return false;
  const end = new Date(start);
  end.setDate(end.getDate() + 1);
  return !Number.isFinite(+start) || rates.phases.some((p) =>
    [p.fromMs, p.untilMs].some((t) => t > +start && t < +end));
}

// Keep a day-only legacy estimate when rates changed within that day
function reprice(request, resolve, recorded = request) {
  const dayStart = request.createdAtMs == null ? new Date(request.day + 'T00:00:00') : null;
  const row = {
    ...request,
    modelKey: request.modelKey ?? request.model,
    modelDisplayName: request.modelDisplayName ?? request.label,
    createdAtMs: request.createdAtMs ?? +dayStart,
  };
  const quote = price(row, resolve, recorded);
  if (ratesChangeWithinDay(quote.rates, dayStart)) return request;
  return { ...request, model: quote.model, label: quote.label, cost: quote.cost ?? 0,
    unpricedSteps: quote.unpricedSteps ?? (quote.cost === null ? request.steps : 0) };
}

// Keep just the inputs needed to compare observations and to reprice them without the source file
function requestUsage(row) {
  return {
    day: localDay(row.createdAtMs), createdAtMs: count(row.createdAtMs),
    modelKey: row.modelKey, modelDisplayName: row.modelDisplayName, responseModel: row.responseModel,
    apiProvider: row.apiProvider, finished: row.finished, client: row.client, steps: 1,
    inputTokens: count(row.inputTokens), cacheReadTokens: count(row.cacheReadTokens),
    outputTokens: count(row.outputTokens), thinkingTokens: count(row.thinkingTokens),
  };
}

function unresolvedModel(model) {
  return model === 'unknown' || (catalogInfo.digest === null && model.startsWith('MODEL_PLACEHOLDER_'));
}

function coverPartialRequest(row, slots, visited = new Set()) {
  const compatible = slots.filter((slot) => !visited.has(slot) &&
    (row.day === UNKNOWN_DAY || row.day === slot.day) &&
    (unresolvedModel(row.model) || row.model === slot.model));
  const free = compatible.find((slot) => slot.matched.length < slot.capacity);
  if (free) {
    free.matched.push(row);
    return true;
  }
  if (row.day === UNKNOWN_DAY && unresolvedModel(row.model)) return false;

  // A previous tentative match may fit another bucket, freeing this one
  for (const slot of compatible) {
    if (visited.has(slot)) continue;
    visited.add(slot);
    const index = slot.matched.findIndex((other) => coverPartialRequest(other, slots, visited));
    if (index < 0) continue;
    slot.matched[index] = row;
    return true;
  }
  return false;
}

function reconcileLegacy(priorBuckets, requests) {
  const dayModel = ({ day, model }) => day + '\u0000' + model;
  const groups = Map.groupBy(requests, dayModel);
  const priorCounts = new Map();
  const retained = [];
  const slots = [];
  for (const prior of priorBuckets) {
    const key = dayModel(prior);
    priorCounts.set(key, prior.steps);
    const observed = (groups.get(key) ?? []).reduce(addTotals, emptyTotals());
    const remaining = prior.steps - observed.steps;
    const betterEstimate = catalogInfo.digest === null && prior.unpricedSteps < observed.unpricedSteps;
    if (remaining > 0 || (remaining === 0 && (compareUsage(prior, observed) > 0 || betterEstimate))) {
      retained.push(prior);
      groups.delete(key);
      if (remaining > 0) slots.push({ day: prior.day, model: prior.model, capacity: remaining, matched: [] });
    }
  }

  // Old buckets lack step IDs, so partial observations may match historical slots
  const missingFields = ({ day, model }) => Number(day === UNKNOWN_DAY) + Number(unresolvedModel(model));
  const partial = [...groups].filter(([, [row]]) => missingFields(row));
  partial.sort(([, [a]], [, [b]]) => missingFields(a) - missingFields(b));
  for (const [key, rows] of partial) {
    const exact = priorCounts.get(key) ?? 0;
    groups.set(key, rows.filter((row, index) => index < exact || !coverPartialRequest(row, slots)));
  }
  return { buckets: mergeBuckets([], [...groups.values()].flat().concat(retained)), retained };
}

export function ledgerEntry(store, file, data, previous) {
  const requests = { ...previous?.requests };
  let lastActiveMs = previous?.lastActiveMs || 0;
  const resolve = createStepModelResolver(new Map());
  for (const row of data?.rows ?? []) {
    lastActiveMs = Math.max(lastActiveMs, count(row.createdAtMs));
    const old = requests[row.stepIndex];
    const difference = old ? compareUsage(row, old) : 1;
    if (difference >= 0) {
      const current = requestUsage(difference === 0 ? withKnownMetadata(row, old) : row);
      if (difference === 0 && !current.createdAtMs && old.day && old.day !== UNKNOWN_DAY) {
        delete current.createdAtMs;
        requests[row.stepIndex] = { ...old, ...current, day: old.day };
      } else {
        requests[row.stepIndex] = current;
      }
    }
  }
  for (const [step, request] of Object.entries(requests)) {
    requests[step] = reprice(request, resolve, previous?.requests?.[step]);
  }
  const observed = Object.values(requests);
  // Reconcile single-copy legacy buckets against all observed requests
  const sameDatabase = file && previous?.store === store.key && file.sig.startsWith(previous.sig + '|');
  const legacy = previous?.legacy ?? (previous && !previous.requests &&
    (!sameDatabase || catalogInfo.digest === null) ? previous.buckets : []);
  const { buckets, retained } = legacy.length
    ? reconcileLegacy(mergeBuckets([], legacy.map((bucket) => reprice(bucket, resolve))), observed)
    : { buckets: mergeBuckets([], observed), retained: [] };
  return {
    accountingVersion: ACCOUNTING_VERSION,
    store: store?.key ?? previous.store,
    client: store?.client ?? previous.client,
    sig: file?.sig ?? previous.sig,
    title: data?.title ?? previous.title,
    parentId: data ? data.trajectory.parentConversationId || data.trajectory.forkParentConversationId || '' : previous.parentId,
    lastActiveMs: lastActiveMs || Math.floor(file?.mtimeMs || 0),
    buckets,
    requests,
    ...(retained.length ? { legacy: retained } : {}),
  };
}

function textOf(value) {
  return typeof value === 'string' ? value : '';
}

// Group requests under the user message that started each turn
export function sessionDetail(id, store, data, recorded) {
  const resolve = createStepModelResolver(new Map());
  const rows = data.rows.map((row) => {
    const old = recorded?.[row.stepIndex];
    const difference = old ? compareUsage(old, row) : -1;
    // Text comes from disk; a finished request can have more usage than this surviving copy
    if (difference > 0) {
      row = { ...row, ...old, modelKey: old.modelKey ?? old.model,
        modelDisplayName: old.modelDisplayName ?? old.label, isComplete: false,
        totalDurationSec: null, totalTokPerSec: null, ttftSec: null, nativeTtftSec: null,
        speedSampleEligible: false, speedReason: 'incomplete_or_invalid_timing' };
    } else if (difference === 0) {
      row = withKnownMetadata(row, old);
    }
    const dayStart = !count(row.createdAtMs) && old?.day && old.day !== UNKNOWN_DAY
      ? new Date(old.day + 'T00:00:00') : null;
    const quote = price(dayStart ? { ...row, createdAtMs: +dayStart } : row, resolve, old);
    if (ratesChangeWithinDay(quote.rates, dayStart)) {
      quote.cost = old.unpricedSteps && !old.cost ? null : old.cost;
      quote.rates = null;
    }
    const { model, label, short, cost, rates, unpricedSteps } = quote;
    const input = count(row.inputTokens);
    const cached = count(row.cacheReadTokens);
    return { ...row, model: { id: model, label, short }, cost, unpricedSteps,
      rates: rates ? { input: rates.input, cache: rates.cache, output: rates.output } : null,
      cacheHitRate: input + cached > 0 ? cached / (input + cached) : null,
      cacheSavings: rates && cached > 0 ? cached / 1_000_000 * Math.max(0, rates.input - rates.cache) : null };
  });
  const steps = [...data.trajectory.steps].sort((a, b) => stepIndexOf(a) - stepIndexOf(b));
  const turns = [];
  let current = null;
  const startTurn = (step) => {
    current = {
      index: turns.length,
      prompt: textOf(step?.userInput?.userResponse),
      atMs: Date.parse(step?.metadata?.createdAt || '') || null,
      requests: [],
      errors: [],
      totals: emptyTotals(),
    };
    turns.push(current);
  };
  const rowsByIndex = new Map(rows.map((row) => [row.stepIndex, row]));
  for (const step of steps) {
    const stepIndex = stepIndexOf(step);
    if (step.userInput) startTurn(step);
    const error = step.errorMessage?.error;
    if (error) {
      if (!current) startTurn(null);
      current.errors.push({
        stepIndex,
        message: textOf(error.userErrorMessage) || textOf(error.shortError),
      });
    }
    const row = rowsByIndex.get(stepIndex);
    if (!row) continue;
    if (!current) startTurn(null);
    const response = step.plannerResponse || {};
    current.requests.push({
      ...row,
      content: {
        response: textOf(response.modifiedResponse) || textOf(response.response),
        thinking: textOf(response.thinking) || textOf(response.rawThinking),
        toolCalls: (response.toolCalls || []).map((call) => {
          const args = textOf(call.argumentsJson);
          return {
            name: textOf(call.name),
            arguments: args.slice(0, MAX_ARGUMENT_CHARS),
            truncated: args.length > MAX_ARGUMENT_CHARS,
          };
        }),
      },
    });
    addTotals(current.totals, {
      steps: 1,
      inputTokens: count(row.inputTokens),
      cacheReadTokens: count(row.cacheReadTokens),
      outputTokens: count(row.outputTokens),
      thinkingTokens: count(row.thinkingTokens),
      unpricedSteps: row.unpricedSteps ?? (row.cost === null ? 1 : 0),
      cost: row.cost ?? 0,
    });
  }
  const totals = turns.reduce((sum, turn) => addTotals(sum, turn.totals), emptyTotals());
  return {
    id,
    store: store.key,
    client: store.client,
    title: data.title,
    parentId: data.trajectory.parentConversationId || data.trajectory.forkParentConversationId || '',
    turns,
    totals,
    speed: summarizeSpeed(rows),
    ttft: summarizeTtft(rows),
  };
}
