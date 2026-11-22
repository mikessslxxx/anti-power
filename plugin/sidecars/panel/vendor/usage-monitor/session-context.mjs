// Adapted from usage-monitor v1.5.2 telemetry-builder and telemetry-display-rules
function scaleBucketsExact(buckets, keys, targetTotal) {
  const safeTarget = Math.max(0, Math.round(Number(targetTotal || 0)));
  const out = {};
  let currentSum = 0;
  for (const k of keys) {
    const val = Math.max(0, Number(buckets[k] || 0));
    out[k] = val;
    currentSum += val;
  }
  if (safeTarget === 0 || currentSum <= 0) {
    for (const k of keys) out[k] = 0;
    return out;
  }
  if (currentSum === safeTarget) {
    for (const k of keys) out[k] = Math.round(out[k]);
    return out;
  }
  const remainders = [];
  let assigned = 0;
  for (const k of keys) {
    const rawVal = out[k];
    if (rawVal <= 0) {
      out[k] = 0;
      continue;
    }
    const exact = (rawVal * safeTarget) / currentSum;
    const floored = Math.floor(exact);
    out[k] = floored;
    assigned += floored;
    remainders.push({ key: k, rem: exact - floored, orig: rawVal });
  }
  const diff = safeTarget - assigned;
  if (diff > 0 && remainders.length > 0) {
    remainders.sort((a, b) => b.rem - a.rem || b.orig - a.orig);
    for (let i = 0; i < diff; i++) {
      out[remainders[i % remainders.length].key] += 1;
    }
  }
  return out;
}

const CONTEXT_CHANNEL_KEYS = [
  'userInput',
  'assistant',
  'toolResults',
  'subagent',
  'checkpoint',
  'systemTools',
  'mcpTools',
  'skills',
  'systemPrompt',
  'userRules',
];

function resolveStepOutputTokens(st) {
  const explicitOut = Number(st?.outputTokens || 0);
  if (explicitOut > 0) return explicitOut;
  return Number(st?.thinkingTokens || 0) + Number(st?.responseTokens || 0);
}

function extractRawBreakdownChannels(breakdown) {
  const rawMessages = Number(breakdown?.messagesTokens || 0);
  const hasGranularMsgBreakdown =
    Boolean(breakdown) &&
    Object.prototype.hasOwnProperty.call(breakdown, 'toolResultsTokens');
  const channels = {
    userInput: hasGranularMsgBreakdown
      ? Number(breakdown?.userInputTokens || 0)
      : rawMessages,
    assistant: Number(breakdown?.assistantTokens || 0),
    toolResults: Number(breakdown?.toolResultsTokens || 0),
    subagent: Number(breakdown?.subagentTokens || 0),
    checkpoint: Number(breakdown?.checkpointTokens || 0),
    systemTools: Number(breakdown?.systemToolsTokens || 0),
    mcpTools: Number(breakdown?.mcpToolsTokens || 0),
    skills: Number(breakdown?.skillsTokens || 0),
    systemPrompt: Number(breakdown?.systemPromptTokens || 0),
    userRules: Number(breakdown?.userRulesTokens || 0),
  };
  const msgSubtotal =
    channels.userInput +
    channels.assistant +
    channels.toolResults +
    channels.subagent +
    channels.checkpoint;
  if (rawMessages > msgSubtotal) {
    channels.userInput += rawMessages - msgSubtotal;
  }
  return channels;
}

function resolveCompactionSummaryTokens(compactionStep) {
  const respTok = Number(compactionStep?.responseTokens || 0);
  if (respTok > 0) return respTok;
  return resolveStepOutputTokens(compactionStep);
}

// Scale the request-start breakdown to metered input, then include the latest output
export function sessionContext(enrichedActiveSteps) {
  const latestEnriched = enrichedActiveSteps.at(-1);
  if (!latestEnriched) return { used: null, limit: null, breakdown: null };
  let lastCompactionIdx = -1;
  for (let i = enrichedActiveSteps.length - 1; i >= 0; i--) {
    if (enrichedActiveSteps[i]?.isCompactionStep) {
      lastCompactionIdx = i;
      break;
    }
  }

  let latestBreakdown = null;
  let breakdownIdx = -1;
  for (let i = enrichedActiveSteps.length - 1; i > lastCompactionIdx; i--) {
    if (enrichedActiveSteps[i]?.contextBreakdown) {
      latestBreakdown = enrichedActiveSteps[i].contextBreakdown;
      breakdownIdx = i;
      break;
    }
  }

  let channels = extractRawBreakdownChannels(latestBreakdown);
  let fallbackPreCompactionLimit = 0;

  if (latestBreakdown && breakdownIdx > lastCompactionIdx) {
    const rawBreakdownTotal = CONTEXT_CHANNEL_KEYS.reduce((sum, k) => sum + channels[k], 0);
    if (rawBreakdownTotal > 0) {
      const bdStep = enrichedActiveSteps[breakdownIdx];
      const bdPromptTokens =
        Number(bdStep?.inputTokens || 0) + Number(bdStep?.cacheReadTokens || 0);
      const bdOutTokens = resolveStepOutputTokens(bdStep);
      // Only align when metered input looks complete (e.g. cacheReadTokens is not omitted)
      if (bdPromptTokens >= rawBreakdownTotal * 0.5) {
        channels = scaleBucketsExact(channels, CONTEXT_CHANNEL_KEYS, bdPromptTokens);
        channels.assistant += bdOutTokens;
        let runningTotal = bdPromptTokens + bdOutTokens;

        for (let m = breakdownIdx + 1; m < enrichedActiveSteps.length; m++) {
          const stepM = enrichedActiveSteps[m];
          if (!stepM || stepM.isCompactionStep) break;
          const mPrompt =
            Number(stepM.inputTokens || 0) + Number(stepM.cacheReadTokens || 0);
          const mOut = resolveStepOutputTokens(stepM);
          if (mPrompt >= runningTotal * 0.5) {
            if (mPrompt > runningTotal) {
              const delta = mPrompt - runningTotal;
              if (enrichedActiveSteps[m - 1]?.hasToolCalls) {
                channels.toolResults += delta;
              } else {
                channels.userInput += delta;
              }
            } else if (mPrompt < runningTotal) {
              channels = scaleBucketsExact(channels, CONTEXT_CHANNEL_KEYS, mPrompt);
            }
            channels.assistant += mOut;
            runningTotal = mPrompt + mOut;
          }
        }
      }
    }
  } else if (lastCompactionIdx >= 0) {
    const compactionStep = enrichedActiveSteps[lastCompactionIdx];
    let preStep = null;
    for (let i = lastCompactionIdx - 1; i >= 0; i--) {
      if (enrichedActiveSteps[i]?.contextBreakdown) {
        preStep = enrichedActiveSteps[i];
        break;
      }
    }
    if (preStep) {
      fallbackPreCompactionLimit = Number(preStep.reportedContextLimitTokens || 0);
      let preChannels = extractRawBreakdownChannels(preStep.contextBreakdown);
      const preRawTotal = CONTEXT_CHANNEL_KEYS.reduce((sum, k) => sum + preChannels[k], 0);
      const prePromptTokens =
        Number(preStep.inputTokens || 0) + Number(preStep.cacheReadTokens || 0);
      if (preRawTotal > 0 && prePromptTokens >= preRawTotal * 0.5) {
        preChannels = scaleBucketsExact(preChannels, CONTEXT_CHANNEL_KEYS, prePromptTokens);
      }
      channels.systemTools = preChannels.systemTools;
      channels.mcpTools = preChannels.mcpTools;
      channels.skills = preChannels.skills;
      channels.systemPrompt = preChannels.systemPrompt;
      channels.userRules = preChannels.userRules;
    }
    channels.userInput = 0;
    channels.assistant = 0;
    channels.toolResults = 0;
    channels.subagent = 0;
    channels.checkpoint = resolveCompactionSummaryTokens(compactionStep);

    let runningTotal = CONTEXT_CHANNEL_KEYS.reduce((sum, k) => sum + channels[k], 0);
    for (let m = lastCompactionIdx + 1; m < enrichedActiveSteps.length; m++) {
      const stepM = enrichedActiveSteps[m];
      if (!stepM || stepM.isCompactionStep) continue;
      const mPrompt =
        Number(stepM.inputTokens || 0) + Number(stepM.cacheReadTokens || 0);
      const mOut = resolveStepOutputTokens(stepM);
      if (mPrompt > 0 && (runningTotal === 0 || mPrompt >= runningTotal * 0.35)) {
        if (mPrompt > runningTotal) {
          const delta = mPrompt - runningTotal;
          if (m > lastCompactionIdx + 1 && enrichedActiveSteps[m - 1]?.hasToolCalls) {
            channels.toolResults += delta;
          } else {
            channels.userInput += delta;
          }
        } else if (mPrompt < runningTotal) {
          channels = scaleBucketsExact(channels, CONTEXT_CHANNEL_KEYS, mPrompt);
        }
        channels.assistant += mOut;
        runningTotal = mPrompt + mOut;
      }
    }
  }

  const channelTotal = CONTEXT_CHANNEL_KEYS.reduce((sum, key) => sum + channels[key], 0);
  const prompt = Number(latestEnriched.inputTokens || 0) + Number(latestEnriched.cacheReadTokens || 0);
  const output = resolveStepOutputTokens(latestEnriched);
  const used = channelTotal || (latestEnriched.isCompactionStep ? null :
    (prompt || Number(latestEnriched.estimatedContextTokens || 0)) + output || null);
  const limit = latestEnriched.reportedContextLimitTokens ||
    (latestEnriched.isCompactionStep ? fallbackPreCompactionLimit : null) || null;
  const breakdown = channelTotal ? Object.fromEntries(
    CONTEXT_CHANNEL_KEYS.map((key) => [key + 'Tokens', channels[key]])
  ) : null;
  return { used, limit, breakdown };
}
