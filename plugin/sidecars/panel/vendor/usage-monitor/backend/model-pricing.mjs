/**
 * @fileoverview 模型身份识别、展示名称规范化与单步 API 等效成本计算。
 *
 * 核心原则：
 * 1. 优先融合服务端下发的动态模型目录 (`dynamicModelMap`)、静态占位符表 (`PLACEHOLDER_MAP`)
 *    与真实响应模型 (`responseModel`)，并优先匹配含档位后缀的精确价格条目；
 * 2. 价格未知或模型身份无法确认时，严格返回 `pricing: null` 与 `costUsd: null`，
 *    绝不借用其他模型单价伪造费用。
 */

import { parseIsoMs } from '../trajectory-metrics.mjs';
import { MODEL_PRICING, PLACEHOLDER_MAP } from './pricing-catalog.mjs';

/**
 * 根据 API 提供商枚举或模型标识符推断所属模型家族（`'Gemini'` | `'Claude'` | `'GPT'` | `'Other'`）。
 * 注意：先匹配更具体的 `ANTHROPIC`/`CLAUDE` 与 `OPENAI`/`GPT`（含 `o1`/`o3`/`o4` 系列），
 * 防止 `API_PROVIDER_GOOGLE_VERTEX_ANTHROPIC` 被宽泛的 `GOOGLE` 子串抢先误判为 `'Gemini'`。
 *
 * @param {string} [rawModelKey=''] 原始模型键名或占位符
 * @param {string} [modelId=''] 规范化模型 ID
 * @param {string} [apiProvider=''] 上游 API 提供商枚举字符串
 * @returns {'Gemini'|'Claude'|'GPT'|'Other'}
 */
function inferFamilyFromProviderOrKey(rawModelKey = '', modelId = '', apiProvider = '') {
  const prov = String(apiProvider || '').toUpperCase();
  if (prov.includes('ANTHROPIC') || prov.includes('CLAUDE')) return 'Claude';
  if (prov.includes('OPENAI') || prov.includes('GPT')) return 'GPT';
  if (prov.includes('GEMINI') || prov.includes('GOOGLE')) return 'Gemini';

  const combined = `${rawModelKey} ${modelId}`.toUpperCase();
  if (
    combined.includes('ANTHROPIC') ||
    combined.includes('CLAUDE') ||
    combined.includes('FABLE') ||
    combined.includes('SONNET') ||
    combined.includes('OPUS') ||
    combined.includes('HAIKU')
  ) {
    return 'Claude';
  }
  if (
    combined.includes('GPT') ||
    combined.includes('OPENAI') ||
    /\bO[134](?:[-_\s]|$)/.test(combined)
  ) {
    return 'GPT';
  }
  if (
    combined.includes('GEMINI') ||
    combined.includes('CARBON') ||
    combined.includes('ARGON') ||
    combined.includes('HYDROGEN') ||
    combined.includes('ORCHESTRATOR') ||
    combined.includes('HELIX') ||
    combined.includes('FLASH') ||
    combined.includes('GOOGLE')
  ) {
    return 'Gemini';
  }
  return 'Other';
}

/**
 * 判断一组模型标识字段是否指向 Gemini 4 Argon 系列模型（含 M417/M418/M419 及 M681 Orchestrator (Argon)）。
 *
 * @param {...string} parts 待检测的标识片段
 * @returns {boolean}
 */
export function isArgonModelIdentifier(...parts) {
  const text = parts.filter(Boolean).join(' ');
  if (
    /\bhydrogen\b|\bgemini-flash-worker-agy\b|MODEL_PLACEHOLDER_M(?:669|52[5678])\b/i.test(text) &&
    !/\bargon\b|MODEL_PLACEHOLDER_M(?:41[789]|681)\b/i.test(text)
  ) {
    return false;
  }
  return /argon|MODEL_PLACEHOLDER_M(?:41[789]|681)\b/i.test(text);
}

/**
 * 判断一组模型标识字段是否指向 Gemini 4 Carbon 系列模型（含 M427/M429/M437/M438 及 M667 Orchestrator (Carbon)）。
 *
 * @param {...string} parts 待检测的标识片段
 * @returns {boolean}
 */
export function isCarbonModelIdentifier(...parts) {
  const text = parts.filter(Boolean).join(' ');
  if (/\bcarbon\b|gemini-4-carbon|MODEL_PLACEHOLDER_M(?:42[79]|43[78]|667)\b/i.test(text)) {
    return true;
  }
  if (/\bhydrogen\b|\bgemini-flash-worker-agy\b|MODEL_PLACEHOLDER_M(?:669|52[5678])\b/i.test(text)) {
    return false;
  }
  return /\bgemini-pro-orchestrator\b(?!-argon)/i.test(text);
}

/**
 * 剥离模型展示名称末尾的思考档位后缀（如 `(XHigh)`、`(High)`、`(Medium)`、`(Thinking)`），提取基础模型族名。
 * 保留 `(Carbon)` / `(Argon)` 后缀，以便 `Orchestrator (Carbon)` 与 `Orchestrator (Argon)` 在汇总表中保持分列。
 *
 * @param {string} [label='']
 * @returns {string}
 */
export function getBaseModelLabel(label = '') {
  return (
    String(label || '')
      .replace(/\bFlash-Lite\b/gi, 'Flash Lite')
      .replace(/\s*\((?:Max|XHigh|High|Medium|Low|Thinking|Extra Low|Tiered|Preview)\)\s*$/i, '')
      .trim() || String(label || 'Unknown Model')
  );
}

const SLUG_TIER_LABELS = {
  xhigh: 'XHigh',
  high: 'High',
  medium: 'Medium',
  low: 'Low',
  'extra-low': 'Extra Low',
  tiered: 'Tiered',
};

/**
 * 将严格匹配的 `gemini-3.1-pro`、`gemini-3.5-flash-lite`、`gemini-4-carbon` 或已知内部路由别名转为人类可读标签。
 * 使用行尾锚点 `$` 防止未收录新后缀（如 `gemini-3.1-pro-ultra-v2`）被静默截断伪装成旧模型获取错误定价。
 *
 * @param {string} [slug='']
 * @returns {string}
 */
export function formatGenericModelSlug(slug = '') {
  const s = String(slug || '')
    .trim()
    .replace(/^session-display:/i, '');
  if (!s || /^MODEL_PLACEHOLDER_/i.test(s)) return '';
  if (/^gemini-4-argon-(?:b|f|trial|disabled)$/i.test(s)) return 'Gemini 4 Argon';
  if (/^gemini-3\.8-flash-n$/i.test(s)) return 'Gemini 3.8 Flash';
  if (/^gemini-3\.7-flash-safety-le$/i.test(s)) return 'Gemini 3.7 Flash';
  if (/^gemini-3-flash-(?:[abd]|agent)$/i.test(s)) return 'Gemini 3.5 Flash';
  if (/^gemini-3-flash-c$/i.test(s)) return 'Gemini 3 Flash';
  if (/^gemini-pro-(?:default|agent|b)$/i.test(s)) return 'Gemini 3.1 Pro';
  const m = s.match(
    /^gemini-(\d+(?:\.\d+)?)-(pro|flash|argon|carbon|hydrogen|helix)(?:-(lite))?(?:-(xhigh|high|medium|low|extra-low|tiered|preview))?$/i
  );
  if (m) {
    const ver = m[1];
    const tier = m[2].charAt(0).toUpperCase() + m[2].slice(1).toLowerCase();
    const lite = m[3] ? ' Lite' : '';
    const suffixKey = m[4] ? m[4].toLowerCase() : '';
    const tierSuffix = SLUG_TIER_LABELS[suffixKey] ? ` (${SLUG_TIER_LABELS[suffixKey]})` : '';
    return `Gemini ${ver} ${tier}${lite}${tierSuffix}`;
  }
  const codenameMatch = s.match(/^gemini-(hydrogen|helix)$/i);
  if (codenameMatch) {
    const codename =
      codenameMatch[1].charAt(0).toUpperCase() + codenameMatch[1].slice(1).toLowerCase();
    return `Gemini ${codename}`;
  }
  if (/^gemini-flash-worker-agy$/i.test(s)) {
    return 'Orchestrator Worker (Hydrogen)';
  }
  return '';
}

function extractStepTimestampMs(step) {
  if (!step || typeof step !== 'object') return 0;
  const direct = Number(step.createdAtMs);
  if (Number.isFinite(direct) && direct > 0) return direct;
  if (typeof step.createdAt === 'string' && step.createdAt.trim()) {
    return parseIsoMs(step.createdAt.trim()) || 0;
  }
  return 0;
}

/**
 * 根据目标时间戳匹配模型价格条目的时间阶段（`phases`，左闭右开 `[fromMs, untilMs)`）。
 * 未传入有效时间戳时按当前时刻匹配；未命中任何阶段或未配置阶段时回退至顶层基础单价。
 *
 * @param {object|null} pricingEntry
 * @param {number|string|null} [atMs]
 * @returns {object|null}
 */
export function resolvePricingAt(pricingEntry, atMs) {
  if (!pricingEntry || typeof pricingEntry !== 'object') return null;
  const baseEntry = pricingEntry._basePricing || pricingEntry;
  const phases = baseEntry.phases;
  if (!Array.isArray(phases) || phases.length === 0) return baseEntry;

  let tMs = 0;
  if (typeof atMs === 'number' && Number.isFinite(atMs) && atMs > 0) {
    tMs = atMs;
  } else if (typeof atMs === 'string' && atMs.trim()) {
    tMs = parseIsoMs(atMs.trim()) || 0;
  }
  if (tMs <= 0) {
    tMs = Date.now();
  }

  for (let i = 0; i < phases.length; i++) {
    const ph = phases[i];
    if (!ph || typeof ph !== 'object') continue;
    const fromMs = Number.isFinite(ph.fromMs)
      ? ph.fromMs
      : typeof ph.from === 'string' && ph.from.trim()
        ? parseIsoMs(ph.from.trim()) || -Infinity
        : -Infinity;
    const untilMs = Number.isFinite(ph.untilMs)
      ? ph.untilMs
      : typeof ph.until === 'string' && ph.until.trim()
        ? parseIsoMs(ph.until.trim()) || Infinity
        : Infinity;
    if (tMs >= fromMs && tMs < untilMs) {
      if (
        ph.input === baseEntry.input &&
        ph.cache === baseEntry.cache &&
        ph.output === baseEntry.output
      ) {
        return baseEntry;
      }
      const resolved = {
        ...baseEntry,
        input: ph.input,
        cache: ph.cache,
        output: ph.output,
      };
      Object.defineProperty(resolved, '_basePricing', {
        value: baseEntry,
        enumerable: false,
      });
      return resolved;
    }
  }
  return baseEntry;
}

function resolveModelCatalogEntry(
  rawModelKey,
  dynamicModelMap = new Map(),
  apiProvider = '',
  modelDisplayName = '',
  responseModel = ''
) {
  const dynEntry =
    dynamicModelMap && typeof dynamicModelMap.get === 'function'
      ? dynamicModelMap.get(rawModelKey)
      : null;
  const staticPh = PLACEHOLDER_MAP[rawModelKey] || null;

  // 若 GetUserStatus 仅下发了 label 而未下发真实 modelId（或被填成占位符自身），继续合并静态 PLACEHOLDER_MAP 的精确 modelId
  const dynValidModelId =
    dynEntry?.modelId && dynEntry.modelId !== rawModelKey ? dynEntry.modelId : '';
  const mergedModelId = dynValidModelId || staticPh?.modelId || dynEntry?.modelId || '';
  const mergedLabel = dynEntry?.label || staticPh?.label || '';

  const cleanResponseModel =
    responseModel && responseModel !== 'unknown' ? String(responseModel).trim() : '';
  const cleanRawFallback =
    rawModelKey && rawModelKey !== 'unknown' ? rawModelKey : cleanResponseModel;
  const isWorkerSlot =
    rawModelKey === 'MODEL_PLACEHOLDER_M669' ||
    /^gemini-flash-worker-agy$/i.test(cleanResponseModel);
  const isVagueGenericDisplay = Boolean(
    modelDisplayName &&
      (/^gemini\s+(?:flash|pro)(?:\s+worker)?$/i.test(modelDisplayName.trim()) ||
        (isWorkerSlot && /^gemini\s+4\s+hydrogen$/i.test(modelDisplayName.trim())))
  );
  const effectiveDisplayName =
    modelDisplayName &&
    ((mergedLabel &&
      (/\borchestrator\b/i.test(modelDisplayName) || isVagueGenericDisplay) &&
      !/\borchestrator\b/i.test(mergedLabel)) ||
      (isVagueGenericDisplay && /^gemini-flash-worker-agy$/i.test(cleanResponseModel)))
      ? ''
      : modelDisplayName;
  const derivedDisplay =
    effectiveDisplayName ||
    mergedLabel ||
    formatGenericModelSlug(rawModelKey) ||
    formatGenericModelSlug(cleanResponseModel) ||
    cleanRawFallback ||
    '模型未知';

  // 先生成保留档位后缀的精确 Slug（如 "Gemini 3.8 Flash (Low)" -> "gemini-3.8-flash-low"），防止被基础族名定价覆盖
  const tieredSlug = derivedDisplay
    .toLowerCase()
    .replace(/[()]/g, ' ')
    .trim()
    .replace(/[^a-z0-9.]+/g, '-')
    .replace(/^-+|-+$/g, '');
  const tieredDashedSlug = tieredSlug.replace(/\./g, '-');

  const baseLabel = getBaseModelLabel(derivedDisplay);
  const canonicalSlug = baseLabel.toLowerCase().replace(/\s+/g, '-');
  const dashedSlug = canonicalSlug.replace(/\./g, '-');
  const modelId =
    mergedModelId ||
    (rawModelKey && rawModelKey !== 'unknown' ? rawModelKey : cleanResponseModel) ||
    'unknown';

  const isOpaqueOrchestratorResp = Boolean(
    cleanResponseModel && /\borchestrator\b/i.test(cleanResponseModel)
  );
  const explicitPricing =
    MODEL_PRICING[modelId] ||
    MODEL_PRICING[rawModelKey] ||
    (cleanResponseModel && !isOpaqueOrchestratorResp
      ? MODEL_PRICING[cleanResponseModel]
      : null) ||
    MODEL_PRICING[tieredSlug] ||
    MODEL_PRICING[tieredDashedSlug] ||
    MODEL_PRICING[canonicalSlug] ||
    MODEL_PRICING[dashedSlug] ||
    (cleanResponseModel && isOpaqueOrchestratorResp
      ? MODEL_PRICING[cleanResponseModel]
      : null);

  if (explicitPricing) {
    return {
      catalogPricing: explicitPricing,
      info: {
        rawModel: rawModelKey,
        modelId,
        label: derivedDisplay,
        baseLabel,
        shortName: explicitPricing.short,
        family: explicitPricing.family,
        pricing: explicitPricing,
        isEstimatedPrice: false,
      },
    };
  }

  const inferredFamily = inferFamilyFromProviderOrKey(rawModelKey, modelId, apiProvider);
  return {
    catalogPricing: null,
    info: {
      rawModel: rawModelKey,
      modelId,
      label: derivedDisplay,
      baseLabel,
      shortName: derivedDisplay,
      family: inferredFamily,
      pricing: null,
      isEstimatedPrice: true,
    },
  };
}

/**
 * 综合动态模型目录、占位符字典与价格表，解析步骤对应的模型元数据与指定时刻的单价配置。
 *
 * @param {string} rawModelKey 步骤记录的原始模型键（如 `MODEL_PLACEHOLDER_M320`）
 * @param {Map<string, {modelId: string, label: string, quotaInfo?: object}>} [dynamicModelMap=new Map()] 从 `GetUserStatus` 提取的动态模型表
 * @param {string} [apiProvider=''] API 提供商枚举
 * @param {string} [modelDisplayName=''] 已知的模型展示名称
 * @param {string} [responseModel=''] 实际响应的上游模型 ID（如 `gemini-3.8-flash-n`）
 * @param {number|string|null} [atMs=0] 步骤发生时间戳（缺省按当前时刻匹配阶段价格）
 */
export function resolveModelInfo(
  rawModelKey,
  dynamicModelMap = new Map(),
  apiProvider = '',
  modelDisplayName = '',
  responseModel = '',
  atMs = 0
) {
  const { catalogPricing, info } = resolveModelCatalogEntry(
    rawModelKey,
    dynamicModelMap,
    apiProvider,
    modelDisplayName,
    responseModel
  );
  if (!catalogPricing) return info;
  const effectivePricing = resolvePricingAt(catalogPricing, atMs);
  return effectivePricing === catalogPricing ? info : { ...info, pricing: effectivePricing };
}

/** 在单次遥测请求内按模型身份与价格阶段复用解析结果。 */
export function createStepModelResolver(dynamicModelMap) {
  const models = new Map();
  return (step) => {
    const stepMs = extractStepTimestampMs(step);
    const { modelKey, apiProvider = '', modelDisplayName = '', responseModel = '' } = step || {};
    if (
      typeof modelKey !== 'string' ||
      typeof apiProvider !== 'string' ||
      typeof modelDisplayName !== 'string' ||
      typeof responseModel !== 'string'
    ) {
      return resolveModelInfo(
        modelKey,
        dynamicModelMap,
        apiProvider,
        modelDisplayName,
        responseModel,
        stepMs
      );
    }
    let providers = models.get(modelKey);
    if (!providers) models.set(modelKey, (providers = new Map()));
    let names = providers.get(apiProvider);
    if (!names) providers.set(apiProvider, (names = new Map()));
    let responses = names.get(modelDisplayName);
    if (!responses) names.set(modelDisplayName, (responses = new Map()));
    let cached = responses.get(responseModel);
    if (!cached) {
      const { catalogPricing, info } = resolveModelCatalogEntry(
        modelKey,
        dynamicModelMap,
        apiProvider,
        modelDisplayName,
        responseModel
      );
      cached = {
        catalogPricing,
        baseInfo: info,
        byPhaseIdx: catalogPricing?.phases?.length ? new Map() : null,
      };
      responses.set(responseModel, cached);
    }
    if (!cached.byPhaseIdx) {
      return cached.baseInfo;
    }
    const tMs = stepMs > 0 ? stepMs : Date.now();
    const phases = cached.catalogPricing.phases;
    let phaseIdx = -1;
    for (let i = 0; i < phases.length; i++) {
      const ph = phases[i];
      const fromMs = Number.isFinite(ph.fromMs) ? ph.fromMs : -Infinity;
      const untilMs = Number.isFinite(ph.untilMs) ? ph.untilMs : Infinity;
      if (tMs >= fromMs && tMs < untilMs) {
        phaseIdx = i;
        break;
      }
    }
    let phasedInfo = cached.byPhaseIdx.get(phaseIdx);
    if (!phasedInfo) {
      const effectivePricing =
        phaseIdx >= 0
          ? resolvePricingAt(cached.catalogPricing, tMs)
          : cached.catalogPricing;
      phasedInfo =
        effectivePricing === cached.catalogPricing
          ? cached.baseInfo
          : { ...cached.baseInfo, pricing: effectivePricing };
      cached.byPhaseIdx.set(phaseIdx, phasedInfo);
    }
    return phasedInfo;
  };
}

/**
 * 为不透明的 Orchestrator 路由模型 ID 补充底层实际模型代号标注（如 `gemini-pro-orchestrator (carbon-4)`），
 * 普通透明上游模型 ID（如 `gemini-3.8-flash-n`、`gemini-4-hydrogen`）原样返回。
 *
 * @param {string} [rawUpstream='']
 * @param {ReturnType<typeof resolveModelInfo>|null} [mInfo=null]
 * @returns {string}
 */
export function formatUpstreamModelId(rawUpstream = '', mInfo = null) {
  const s = String(rawUpstream || '').trim();
  if (!s || !/(?:orchestrator|-worker-agy\b)/i.test(s) || /\([^)]+\)\s*$/.test(s)) {
    return s;
  }
  const metaText = `${mInfo?.rawModel || ''} ${mInfo?.modelId || ''} ${mInfo?.shortName || ''} ${mInfo?.label || ''} ${s}`;
  if (
    /\bhydrogen\b|\bgemini-flash-worker-agy\b|MODEL_PLACEHOLDER_M(?:669|52[5678])\b/i.test(
      metaText
    )
  ) {
    return `${s} (hydrogen-4)`;
  }
  if (isArgonModelIdentifier(mInfo?.rawModel, mInfo?.modelId, mInfo?.label, s)) {
    return `${s} (argon-4)`;
  }
  if (isCarbonModelIdentifier(mInfo?.rawModel, mInfo?.modelId, mInfo?.label, s)) {
    return `${s} (carbon-4)`;
  }
  return s;
}

/**
 * 计算单个步骤的 API 等效美元成本；若模型未定价（`pricing === null`）则返回 `null`。
 * 对负数、NaN 或非有限数值统一安全钳制为 0。
 *
 * @param {{ inputTokens: number, cacheReadTokens: number, outputTokens: number }} step
 * @param {{ input: number, cache: number, output: number } | null} pricing
 * @returns {number|null}
 */
export function computeStepCost(step, pricing) {
  if (!pricing) return null;
  const stepMs = extractStepTimestampMs(step);
  const effectivePricing =
    stepMs > 0 && Array.isArray(pricing.phases) && pricing.phases.length > 0
      ? resolvePricingAt(pricing, stepMs)
      : pricing;
  const safeTok = (v) => {
    const n = Number(v);
    return Number.isFinite(n) && n > 0 ? n : 0;
  };
  const inCost = (safeTok(step?.inputTokens) / 1_000_000) * effectivePricing.input;
  const cacheCost = (safeTok(step?.cacheReadTokens) / 1_000_000) * effectivePricing.cache;
  const outCost = (safeTok(step?.outputTokens) / 1_000_000) * effectivePricing.output;
  return inCost + cacheCost + outCost;
}
