// Anti-Power: trajectory and annotation readers only; store discovery lives in lib/stores.mjs

import { closeSync, existsSync, lstatSync, openSync, readFileSync, readSync } from 'node:fs';
import { basename, join } from 'node:path';
import {
  SQLITE_IN_CHUNK_SIZE,
  getNodeSqliteDatabaseSync,
  withReadOnlySqliteDb,
} from './sqlite-client.mjs';
import {
  decodeProtoDurationSlice,
  decodeProtoMapStringEntry,
  decodeProtoTimestampSlice,
  readProtoString,
  readProtoVarint,
  readSubEnd,
  skipProtoField,
  toUint8Array,
  unescapePbtxtString,
} from './proto-wire.mjs';

const UUID_FILE_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

// Wait until agy initializes the schema before acquiring a SQLite shared lock
function hasTrajectorySchema(dbPath) {
  const fd = openSync(dbPath, 'r');
  const header = Buffer.alloc(100);
  try {
    if (readSync(fd, header, 0, header.length, 0) !== header.length ||
        header.toString('latin1', 0, 16) !== 'SQLite format 3\0') return false;
  } finally {
    closeSync(fd);
  }
  if (header.readUInt32BE(40) !== 0) return true;
  // In WAL mode, the schema can live solely in the WAL until checkpoint
  if (header[18] !== 2) return false;
  const size = header.readUInt16BE(16);
  const pageSize = size === 1 ? 65536 : size;
  const wal = lstatSync(dbPath + '-wal', { throwIfNoEntry: false });
  return Boolean(wal?.isFile() && wal.size >= 32 + 24 + pageSize);
}

/**
 * 从 `annotations/<cascadeId>.pbtxt` 文本内容中提取顶层 `title` 字段。
 *
 * @param {string} rawText
 * @returns {string}
 */
export function parseAnnotationPbtxtTitle(rawText) {
  if (typeof rawText !== 'string' || !rawText) return '';
  const dq = rawText.match(/(?:^|\r?\n)\s*title\s*:\s*"((?:\\.|[^"\\])*)"/);
  if (dq) {
    return unescapePbtxtString(dq[1]).trim();
  }
  const sq = rawText.match(/(?:^|\r?\n)\s*title\s*:\s*'((?:\\.|[^'\\])*)'/);
  if (sq) {
    return unescapePbtxtString(sq[1]).trim();
  }
  return '';
}

/**
 * 按会话 ID 在各产品目录的 `annotations/<cascadeId>.pbtxt` 中查找标题。
 *
 * @param {string} cascadeId
 * @param {string[]} [annotationDirs=[]]
 * @returns {string}
 */
export function readAnnotationTitleForCascade(cascadeId, annotationDirs = []) {
  const normId = String(cascadeId || '').trim().toLowerCase();
  if (!normId) return '';
  const dirs = Array.isArray(annotationDirs) ? annotationDirs : [];
  for (const dir of dirs) {
    if (!dir) continue;
    const filePath = join(dir, `${normId}.pbtxt`);
    if (!existsSync(filePath)) continue;
    try {
      const title = parseAnnotationPbtxtTitle(readFileSync(filePath, 'utf8'));
      if (title) return title;
    } catch {
      // 忽略单文件读取失败
    }
  }
  return '';
}

// ---------------------------------------------------------------------------
// 零依赖轻量 Protobuf Wire 解码器（按需提取指标字段，O(1) 跳过大体积载荷）
// ---------------------------------------------------------------------------

const PROTO_STATIC_MODELS = Object.freeze({
  235: 'MODEL_CHAT_20706',
  246: 'MODEL_GOOGLE_GEMINI_2_5_PRO',
  269: 'MODEL_CHAT_23310',
  281: 'MODEL_CLAUDE_4_SONNET',
  282: 'MODEL_CLAUDE_4_SONNET_THINKING',
  290: 'MODEL_CLAUDE_4_OPUS',
  291: 'MODEL_CLAUDE_4_OPUS_THINKING',
  312: 'MODEL_GOOGLE_GEMINI_2_5_FLASH',
  313: 'MODEL_GOOGLE_GEMINI_2_5_FLASH_THINKING',
  327: 'MODEL_GOOGLE_GEMINI_FOR_GOOGLE_2_5_PRO',
  329: 'MODEL_GOOGLE_GEMINI_2_5_FLASH_THINKING_TOOLS',
  330: 'MODEL_GOOGLE_GEMINI_2_5_FLASH_LITE',
  332: 'MODEL_GOOGLE_GEMINI_2_5_FLASH_IMAGE_PREVIEW',
  333: 'MODEL_CLAUDE_4_5_SONNET',
  334: 'MODEL_CLAUDE_4_5_SONNET_THINKING',
  340: 'MODEL_CLAUDE_4_5_HAIKU',
  341: 'MODEL_CLAUDE_4_5_HAIKU_THINKING',
  342: 'MODEL_OPENAI_GPT_OSS_120B_MEDIUM',
});

const PROTO_API_PROVIDERS = Object.freeze({
  1: 'API_PROVIDER_INTERNAL',
  3: 'API_PROVIDER_GOOGLE_VERTEX',
  24: 'API_PROVIDER_GOOGLE_GEMINI',
  26: 'API_PROVIDER_ANTHROPIC_VERTEX',
  30: 'API_PROVIDER_GOOGLE_EVERGREEN',
  31: 'API_PROVIDER_OPENAI_VERTEX',
  32: 'API_PROVIDER_OPENAI_RESPONSES_VERTEX',
});

const PROTO_STEP_TYPES = Object.freeze({
  5: 'CORTEX_STEP_TYPE_CODE_ACTION',
  7: 'CORTEX_STEP_TYPE_GREP_SEARCH',
  8: 'CORTEX_STEP_TYPE_VIEW_FILE',
  9: 'CORTEX_STEP_TYPE_LIST_DIRECTORY',
  14: 'CORTEX_STEP_TYPE_USER_INPUT',
  15: 'CORTEX_STEP_TYPE_PLANNER_RESPONSE',
  17: 'CORTEX_STEP_TYPE_ERROR_MESSAGE',
  21: 'CORTEX_STEP_TYPE_RUN_COMMAND',
  23: 'CORTEX_STEP_TYPE_CHECKPOINT',
  25: 'CORTEX_STEP_TYPE_FIND',
  28: 'CORTEX_STEP_TYPE_COMMAND_STATUS',
  31: 'CORTEX_STEP_TYPE_READ_URL_CONTENT',
  33: 'CORTEX_STEP_TYPE_SEARCH_WEB',
  38: 'CORTEX_STEP_TYPE_MCP_TOOL',
  51: 'CORTEX_STEP_TYPE_LIST_RESOURCES',
  52: 'CORTEX_STEP_TYPE_READ_RESOURCE',
  81: 'CORTEX_STEP_TYPE_TASK_BOUNDARY',
  82: 'CORTEX_STEP_TYPE_NOTIFY_USER',
  85: 'CORTEX_STEP_TYPE_BROWSER_SUBAGENT',
  90: 'CORTEX_STEP_TYPE_EPHEMERAL_MESSAGE',
  91: 'CORTEX_STEP_TYPE_GENERATE_IMAGE',
  100: 'CORTEX_STEP_TYPE_SEND_COMMAND_INPUT',
  101: 'CORTEX_STEP_TYPE_SYSTEM_MESSAGE',
  127: 'CORTEX_STEP_TYPE_INVOKE_SUBAGENT',
  132: 'CORTEX_STEP_TYPE_GENERIC',
  138: 'CORTEX_STEP_TYPE_ASK_QUESTION',
});

const PROTO_STEP_STATUSES = Object.freeze({
  1: 'CORTEX_STEP_STATUS_PENDING',
  2: 'CORTEX_STEP_STATUS_RUNNING',
  3: 'CORTEX_STEP_STATUS_DONE',
  4: 'CORTEX_STEP_STATUS_INVALID',
  5: 'CORTEX_STEP_STATUS_CLEARED',
  6: 'CORTEX_STEP_STATUS_CANCELED',
  7: 'CORTEX_STEP_STATUS_ERROR',
  8: 'CORTEX_STEP_STATUS_GENERATING',
  9: 'CORTEX_STEP_STATUS_WAITING',
  11: 'CORTEX_STEP_STATUS_QUEUED',
  12: 'CORTEX_STEP_STATUS_INTERRUPTED',
});

const PROTO_TOKEN_TYPES = Object.freeze({
  1: 'TOKEN_TYPE_SYSTEM_PROMPT',
  2: 'TOKEN_TYPE_TOOLS',
  3: 'TOKEN_TYPE_RULES',
  4: 'TOKEN_TYPE_MCP_TOOLS',
  5: 'TOKEN_TYPE_SKILLS',
  6: 'TOKEN_TYPE_SUBAGENTS',
  7: 'TOKEN_TYPE_CHAT_MESSAGES',
  8: 'TOKEN_TYPE_WORKFLOWS',
  9: 'TOKEN_TYPE_PLUGINS',
  10: 'TOKEN_TYPE_FLOWS',
});

const PROTO_TOKEN_SOURCES = Object.freeze({
  1: 'TOKEN_SOURCE_SYSTEM',
  2: 'TOKEN_SOURCE_USER',
});

const PROTO_CHAT_MSG_SOURCES = Object.freeze({
  1: 'CHAT_MESSAGE_SOURCE_USER',
  2: 'CHAT_MESSAGE_SOURCE_SYSTEM',
  3: 'CHAT_MESSAGE_SOURCE_UNKNOWN',
  4: 'CHAT_MESSAGE_SOURCE_TOOL',
  5: 'CHAT_MESSAGE_SOURCE_SYSTEM_PROMPT',
  6: 'CHAT_MESSAGE_SOURCE_CONTEXT',
});

function decodeProtoModelEnumInt(val) {
  const n = Math.trunc(Number(val));
  if (!Number.isFinite(n) || n <= 0) return '';
  if (n >= 1000 && n < 100000) {
    return `MODEL_PLACEHOLDER_M${n - 1000}`;
  }
  return PROTO_STATIC_MODELS[n] || '';
}

function decodeProtoStepTypeEnum(val) {
  const n = Math.trunc(Number(val));
  return PROTO_STEP_TYPES[n] || 'CORTEX_STEP_TYPE_GENERIC';
}

function decodeProtoStepStatusEnum(val) {
  const n = Math.trunc(Number(val));
  return PROTO_STEP_STATUSES[n] || 'CORTEX_STEP_STATUS_DONE';
}

function decodeProtoSubagentSpecSlice(buf, state, end) {
  let typeName = '';
  let role = '';
  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 1 && wt === 2) {
      typeName = readProtoString(buf, state, end).trim();
    } else if (fn === 2 && wt === 2) {
      role = readProtoString(buf, state, end).trim();
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  state.off = end;
  return typeName || role ? { typeName, role } : null;
}

function decodeProtoTrajectoryMetadataSlice(buf, state, end) {
  let createdAt = '';
  let parentConversationId = '';
  let rootConversationId = '';
  let nestingDepth = 0;
  let subagentSpec = null;
  let agentScript = null;

  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 2 && wt === 2) {
      createdAt = decodeProtoTimestampSlice(buf, state, readSubEnd(buf, state, end));
    } else if (fn === 4 && wt === 2) {
      const asEnd = readSubEnd(buf, state, end);
      let name = '';
      while (state.off < asEnd) {
        const asTag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, asEnd)));
        if ((asTag >>> 3) === 1 && (asTag & 7) === 2) {
          name = readProtoString(buf, state, asEnd).trim();
        } else {
          skipProtoField(buf, state, asTag & 7, asEnd);
        }
      }
      state.off = asEnd;
      if (name) agentScript = { name };
    } else if (fn === 5 && wt === 2) {
      parentConversationId = readProtoString(buf, state, end).trim().toLowerCase();
    } else if (fn === 6 && wt === 2) {
      rootConversationId = readProtoString(buf, state, end).trim().toLowerCase();
    } else if (fn === 8 && wt === 2) {
      subagentSpec = decodeProtoSubagentSpecSlice(buf, state, readSubEnd(buf, state, end));
    } else if (fn === 17 && wt === 0) {
      nestingDepth = Number(BigInt.asIntN(32, readProtoVarint(buf, state, end)));
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  state.off = end;
  return {
    createdAt,
    parentConversationId,
    rootConversationId,
    nestingDepth,
    subagentSpec,
    agentScript,
  };
}

function decodeProtoModelUsageSlice(buf, state, end) {
  const usage = {};
  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 1 && wt === 0) {
      const m = decodeProtoModelEnumInt(readProtoVarint(buf, state, end));
      if (m) usage.model = m;
    } else if (fn === 1 && wt === 2) {
      const m = readProtoString(buf, state, end).trim();
      if (m) usage.model = m;
    } else if (fn === 2 && wt === 0) {
      const v = Number(BigInt.asUintN(64, readProtoVarint(buf, state, end)));
      if (v > 0) usage.inputTokens = v;
    } else if (fn === 3 && wt === 0) {
      const v = Number(BigInt.asUintN(64, readProtoVarint(buf, state, end)));
      if (v > 0) usage.outputTokens = v;
    } else if (fn === 5 && wt === 0) {
      const v = Number(BigInt.asUintN(64, readProtoVarint(buf, state, end)));
      if (v > 0) usage.cacheReadTokens = v;
    } else if (fn === 6 && wt === 0) {
      const provNum = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
      const prov = PROTO_API_PROVIDERS[provNum] || '';
      if (prov) usage.apiProvider = prov;
    } else if (fn === 9 && wt === 0) {
      const v = Number(BigInt.asUintN(64, readProtoVarint(buf, state, end)));
      if (v > 0) usage.thinkingOutputTokens = v;
    } else if (fn === 10 && wt === 0) {
      const v = Number(BigInt.asUintN(64, readProtoVarint(buf, state, end)));
      if (v > 0) usage.responseOutputTokens = v;
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  state.off = end;
  if (usage.inputTokens === undefined) usage.inputTokens = 0;
  if (usage.cacheReadTokens === undefined) usage.cacheReadTokens = 0;
  if (usage.outputTokens === undefined) usage.outputTokens = 0;
  return usage;
}

function decodeProtoChildTokenGroupSlice(buf, state, end, depth = 0) {
  let name = '';
  let type = '';
  let numTokens = 0;
  const children = [];

  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 1 && wt === 2) {
      name = readProtoString(buf, state, end);
    } else if (fn === 2 && wt === 2 && depth < 2) {
      // 子级 TokenGroup 的 children 位于 field 2
      children.push(decodeProtoChildTokenGroupSlice(buf, state, readSubEnd(buf, state, end), depth + 1));
    } else if (fn === 2 && wt === 0) {
      const tNum = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
      type = PROTO_TOKEN_TYPES[tNum] || '';
    } else if (fn === 3 && wt === 0) {
      // 子级 TokenGroup 的 numTokens 位于 field 3
      numTokens = Number(BigInt.asIntN(32, readProtoVarint(buf, state, end)));
    } else if (fn === 4 && wt === 0) {
      // 兼容直接按顶层结构编码的子组（numTokens 位于 field 4）
      numTokens = Number(BigInt.asIntN(32, readProtoVarint(buf, state, end)));
    } else if (fn === 5 && wt === 2 && depth < 2) {
      children.push(decodeProtoChildTokenGroupSlice(buf, state, readSubEnd(buf, state, end), depth + 1));
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  state.off = end;
  return {
    name,
    ...(type ? { type } : {}),
    numTokens,
    ...(children.length > 0 ? { children } : {}),
  };
}

function decodeProtoTopTokenGroupSlice(buf, state, end) {
  let name = '';
  let type = '';
  let source = '';
  let numTokens = 0;
  const children = [];

  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 1 && wt === 2) {
      name = readProtoString(buf, state, end);
    } else if (fn === 2 && wt === 0) {
      const tNum = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
      type = PROTO_TOKEN_TYPES[tNum] || '';
    } else if (fn === 3 && wt === 0) {
      const sNum = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
      source = PROTO_TOKEN_SOURCES[sNum] || '';
    } else if (fn === 4 && wt === 0) {
      numTokens = Number(BigInt.asIntN(32, readProtoVarint(buf, state, end)));
    } else if (fn === 5 && wt === 2) {
      children.push(decodeProtoChildTokenGroupSlice(buf, state, readSubEnd(buf, state, end), 0));
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  state.off = end;
  return {
    name,
    ...(type ? { type } : {}),
    ...(source ? { source } : {}),
    numTokens,
    ...(children.length > 0 ? { children } : {}),
  };
}

function decodeProtoContextWindowMetaSlice(buf, state, end, includeBreakdown) {
  let estimatedTokensUsed = 0;
  let maxContextTokens = 0;
  const groups = [];
  let totalTokens = 0;

  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 1 && wt === 0) {
      estimatedTokensUsed = Number(BigInt.asIntN(32, readProtoVarint(buf, state, end)));
    } else if (fn === 4 && wt === 0) {
      maxContextTokens = Number(BigInt.asIntN(32, readProtoVarint(buf, state, end)));
    } else if (fn === 3 && wt === 2) {
      if (includeBreakdown) {
        const tbEnd = readSubEnd(buf, state, end);
        while (state.off < tbEnd) {
          const tbTag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, tbEnd)));
          const tbFn = tbTag >>> 3;
          const tbWt = tbTag & 7;
          if (tbFn === 1 && tbWt === 2) {
            groups.push(decodeProtoTopTokenGroupSlice(buf, state, readSubEnd(buf, state, tbEnd)));
          } else if (tbFn === 2 && tbWt === 0) {
            totalTokens = Number(BigInt.asIntN(32, readProtoVarint(buf, state, tbEnd)));
          } else {
            skipProtoField(buf, state, tbWt, tbEnd);
          }
        }
        state.off = tbEnd;
      } else {
        skipProtoField(buf, state, wt, end);
      }
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  state.off = end;

  return {
    ...(estimatedTokensUsed > 0 ? { estimatedTokensUsed } : {}),
    ...(maxContextTokens > 0 ? { maxContextTokens } : {}),
    ...(includeBreakdown && groups.length > 0
      ? { tokenBreakdown: { groups, ...(totalTokens > 0 ? { totalTokens } : {}) } }
      : {}),
  };
}

function decodeProtoRetryInfoSlice(buf, state, end) {
  const ri = {};
  while (state.off < end) {
    const rTag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const rFn = rTag >>> 3;
    const rWt = rTag & 7;
    if (rFn === 2 && rWt === 2) {
      ri.usage = decodeProtoModelUsageSlice(buf, state, readSubEnd(buf, state, end));
    } else if (rFn === 3 && rWt === 2) {
      const errStr = readProtoString(buf, state, end).trim();
      if (errStr) {
        ri.error = errStr.length > 320 ? `${errStr.slice(0, 319)}…` : errStr;
      }
    } else if (rFn === 5 && rWt === 0) {
      const code = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
      if (code > 0) ri.reasonCode = code;
    } else if (rFn === 7 && rWt === 2) {
      const dur = decodeProtoDurationSlice(buf, state, readSubEnd(buf, state, end));
      if (dur) ri.retryDelay = dur;
    } else {
      skipProtoField(buf, state, rWt, end);
    }
  }
  state.off = end;
  return ri;
}

function decodeProtoErrorMessageSlice(buf, state, end) {
  let userErrorMessage = '';
  let shortError = '';
  let errorCode = 0;
  let shouldShowModel = false;
  let shouldShowUser = false;

  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 3 && wt === 2) {
      const eEnd = readSubEnd(buf, state, end);
      while (state.off < eEnd) {
        const eTag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, eEnd)));
        const eFn = eTag >>> 3;
        const eWt = eTag & 7;
        if (eFn === 1 && eWt === 2) {
          const s = readProtoString(buf, state, eEnd).trim();
          if (s) userErrorMessage = s.length > 240 ? `${s.slice(0, 239)}…` : s;
        } else if (eFn === 2 && eWt === 2) {
          const s = readProtoString(buf, state, eEnd).trim();
          if (s) shortError = s.length > 320 ? `${s.slice(0, 319)}…` : s;
        } else if (eFn === 7 && eWt === 0) {
          errorCode = Number(BigInt.asUintN(32, readProtoVarint(buf, state, eEnd)));
        } else {
          skipProtoField(buf, state, eWt, eEnd);
        }
      }
      state.off = eEnd;
    } else if (fn === 4 && wt === 0) {
      shouldShowModel = readProtoVarint(buf, state, end) !== 0n;
    } else if (fn === 5 && wt === 0) {
      shouldShowUser = readProtoVarint(buf, state, end) !== 0n;
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  state.off = end;
  if (!userErrorMessage && !shortError && !errorCode) return null;
  return {
    error: {
      ...(userErrorMessage ? { userErrorMessage } : {}),
      ...(shortError ? { shortError } : {}),
      ...(errorCode > 0 ? { errorCode } : {}),
    },
    ...(shouldShowModel ? { shouldShowModel: true } : {}),
    ...(shouldShowUser ? { shouldShowUser: true } : {}),
  };
}

function decodeProtoChatModelMetaSlice(buf, state, end, includeBreakdown) {
  let model = '';
  let usage = null;
  let contextWindowMetadata = null;
  let timeToFirstToken = '';
  let streamingDuration = '';
  let responseModel = '';
  let modelDisplayName = '';
  let retryCount = 0;
  let retryInfos = null;
  const extraMeta = {};
  let messagePrompts = null;
  let tools = null;
  let promptSections = null;

  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 2 && wt === 2) {
      if (includeBreakdown) {
        const mpEnd = readSubEnd(buf, state, end);
        const msg = {};
        while (state.off < mpEnd) {
          const mTag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, mpEnd)));
          const mFn = mTag >>> 3;
          const mWt = mTag & 7;
          if (mFn === 2 && mWt === 0) {
            const src = PROTO_CHAT_MSG_SOURCES[Number(BigInt.asUintN(32, readProtoVarint(buf, state, mpEnd)))];
            if (src) msg.source = src;
          } else if (mFn === 3 && mWt === 2) {
            msg.prompt = readProtoString(buf, state, mpEnd);
          } else if (mFn === 4 && mWt === 0) {
            const nt = Number(BigInt.asUintN(32, readProtoVarint(buf, state, mpEnd)));
            if (nt > 0) msg.numTokens = nt;
          } else {
            skipProtoField(buf, state, mWt, mpEnd);
          }
        }
        state.off = mpEnd;
        if (!messagePrompts) messagePrompts = [];
        messagePrompts.push(msg);
      } else {
        skipProtoField(buf, state, wt, end);
      }
    } else if (fn === 3 && wt === 0) {
      const enumStr = decodeProtoModelEnumInt(readProtoVarint(buf, state, end));
      if (enumStr && !model) model = enumStr;
    } else if (fn === 4 && wt === 2) {
      usage = decodeProtoModelUsageSlice(buf, state, readSubEnd(buf, state, end));
    } else if (fn === 8 && wt === 2) {
      if (includeBreakdown) {
        const tdEnd = readSubEnd(buf, state, end);
        const tool = {};
        while (state.off < tdEnd) {
          const tTag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, tdEnd)));
          const tFn = tTag >>> 3;
          const tWt = tTag & 7;
          if (tFn === 1 && tWt === 2) tool.name = readProtoString(buf, state, tdEnd);
          else if (tFn === 2 && tWt === 2) tool.description = readProtoString(buf, state, tdEnd);
          else if (tFn === 3 && tWt === 2) tool.jsonSchemaString = readProtoString(buf, state, tdEnd);
          else if (tFn === 6 && tWt === 2) tool.serverName = readProtoString(buf, state, tdEnd);
          else skipProtoField(buf, state, tWt, tdEnd);
        }
        state.off = tdEnd;
        if (!tools) tools = [];
        tools.push(tool);
      } else {
        skipProtoField(buf, state, wt, end);
      }
    } else if (fn === 9 && wt === 2) {
      const csEnd = readSubEnd(buf, state, end);
      while (state.off < csEnd) {
        const csTag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, csEnd)));
        const csFn = csTag >>> 3;
        const csWt = csTag & 7;
        if (csFn === 10 && csWt === 2) {
          contextWindowMetadata = decodeProtoContextWindowMetaSlice(
            buf,
            state,
            readSubEnd(buf, state, csEnd),
            includeBreakdown
          );
        } else {
          skipProtoField(buf, state, csWt, csEnd);
        }
      }
      state.off = csEnd;
    } else if (fn === 11 && wt === 2) {
      timeToFirstToken = decodeProtoDurationSlice(buf, state, readSubEnd(buf, state, end));
    } else if (fn === 12 && wt === 2) {
      streamingDuration = decodeProtoDurationSlice(buf, state, readSubEnd(buf, state, end));
    } else if (fn === 14 && wt === 0) {
      retryCount = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    } else if (fn === 16 && wt === 2) {
      if (includeBreakdown) {
        const psEnd = readSubEnd(buf, state, end);
        const sec = {};
        while (state.off < psEnd) {
          const pTag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, psEnd)));
          const pFn = pTag >>> 3;
          const pWt = pTag & 7;
          if (pFn === 1 && pWt === 2) sec.title = readProtoString(buf, state, psEnd);
          else if (pFn === 2 && pWt === 2) sec.content = readProtoString(buf, state, psEnd);
          else if (pFn === 6 && pWt === 0) {
            const tt = PROTO_TOKEN_TYPES[Number(BigInt.asUintN(32, readProtoVarint(buf, state, psEnd)))];
            if (tt) sec.tokenType = tt;
          } else if (pFn === 7 && pWt === 0) {
            const ts = PROTO_TOKEN_SOURCES[Number(BigInt.asUintN(32, readProtoVarint(buf, state, psEnd)))];
            if (ts) sec.tokenSource = ts;
          } else {
            skipProtoField(buf, state, pWt, psEnd);
          }
        }
        state.off = psEnd;
        if (!promptSections) promptSections = [];
        promptSections.push(sec);
      } else {
        skipProtoField(buf, state, wt, end);
      }
    } else if (fn === 17 && wt === 2) {
      const ri = decodeProtoRetryInfoSlice(buf, state, readSubEnd(buf, state, end));
      if (!retryInfos) retryInfos = [];
      retryInfos.push(ri);
    } else if (fn === 19 && wt === 2) {
      responseModel = readProtoString(buf, state, end).trim();
    } else if (fn === 20 && wt === 2) {
      const [k, v] = decodeProtoMapStringEntry(buf, state, readSubEnd(buf, state, end));
      if (k && v) extraMeta[k] = v;
    } else if (fn === 21 && wt === 2) {
      modelDisplayName = readProtoString(buf, state, end).trim();
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  state.off = end;

  if (extraMeta.model_enum) {
    const cleanEnum = extraMeta.model_enum.trim();
    if (cleanEnum && !/^MODEL_(?:UNSPECIFIED|UNKNOWN)$/i.test(cleanEnum) && cleanEnum !== '0') {
      model = cleanEnum;
      if (usage && !usage.model) {
        usage.model = model;
      }
    }
  }
  if (extraMeta.used_claude === 'true' && usage && !usage.apiProvider) {
    usage.apiProvider = 'API_PROVIDER_ANTHROPIC';
  }

  const hasRetryEvidence =
    retryCount > 0 || Boolean(retryInfos && retryInfos.some((r) => r?.error || r?.reasonCode));
  return {
    chatModel: {
      ...(model ? { model } : {}),
      ...(usage ? { usage } : {}),
      ...(contextWindowMetadata && Object.keys(contextWindowMetadata).length > 0
        ? { chatStartMetadata: { contextWindowMetadata } }
        : {}),
      ...(timeToFirstToken ? { timeToFirstToken } : {}),
      ...(streamingDuration ? { streamingDuration } : {}),
      ...(retryCount > 0 ? { retryCount } : {}),
      ...(hasRetryEvidence && retryInfos ? { retryInfos } : {}),
      ...(responseModel ? { responseModel } : {}),
      ...(modelDisplayName ? { modelDisplayName } : {}),
      ...(messagePrompts ? { messagePrompts } : {}),
      ...(tools ? { tools } : {}),
      ...(promptSections ? { promptSections } : {}),
    },
    extraMeta,
  };
}

function decodeProtoPlannerConfigSlice(buf, state, end) {
  const pc = {};
  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 1 && wt === 0) {
      const pm = decodeProtoModelEnumInt(readProtoVarint(buf, state, end));
      if (pm) pc.planModel = pm;
    } else if (fn === 28 && wt === 2) {
      const mn = readProtoString(buf, state, end).trim();
      if (mn) pc.modelName = mn;
    } else if (fn === 60 && wt === 2) {
      const pecEnd = readSubEnd(buf, state, end);
      let execModel = '';
      while (state.off < pecEnd) {
        const pTag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, pecEnd)));
        if ((pTag >>> 3) === 1 && (pTag & 7) === 0) {
          execModel = decodeProtoModelEnumInt(readProtoVarint(buf, state, pecEnd));
        } else {
          skipProtoField(buf, state, pTag & 7, pecEnd);
        }
      }
      state.off = pecEnd;
      if (execModel) pc.planExecuteConfig = { executionModel: execModel };
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  state.off = end;
  return Object.keys(pc).length > 0 ? pc : null;
}

function decodeProtoGeneratorMeta(rawBlob, includeBreakdown = true) {
  const buf = toUint8Array(rawBlob);
  if (!buf || buf.length === 0) return null;
  const state = { off: 0 };
  const end = buf.length;
  const stepIndices = [];
  let chatDecoded = null;
  let plannerConfig = null;

  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 2 && wt === 0) {
      stepIndices.push(Number(BigInt.asUintN(32, readProtoVarint(buf, state, end))));
    } else if (fn === 2 && wt === 2) {
      const pEnd = readSubEnd(buf, state, end);
      while (state.off < pEnd) {
        stepIndices.push(Number(BigInt.asUintN(32, readProtoVarint(buf, state, pEnd))));
      }
      state.off = pEnd;
    } else if (fn === 1 && wt === 2) {
      chatDecoded = decodeProtoChatModelMetaSlice(
        buf,
        state,
        readSubEnd(buf, state, end),
        includeBreakdown
      );
    } else if (fn === 3 && wt === 2) {
      plannerConfig = decodeProtoPlannerConfigSlice(buf, state, readSubEnd(buf, state, end));
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }

  if (!chatDecoded && !plannerConfig) return null;
  return {
    stepIndices,
    ...(chatDecoded ? { chatModel: chatDecoded.chatModel } : {}),
    ...(plannerConfig ? { plannerConfig } : {}),
    extraMeta: chatDecoded?.extraMeta || {},
  };
}

function decodeProtoToolCallSlice(buf, state, end, includeArgs = true) {
  const tc = {};
  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 1 && wt === 2) {
      tc.id = readProtoString(buf, state, end);
    } else if (fn === 2 && wt === 2) {
      tc.name = readProtoString(buf, state, end);
    } else if (fn === 3 && wt === 2) {
      if (includeArgs) {
        tc.argumentsJson = readProtoString(buf, state, end);
      } else {
        skipProtoField(buf, state, wt, end);
      }
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  state.off = end;
  return tc;
}

function decodeProtoStepMeta(rawBlob, fallbackIdx) {
  const buf = toUint8Array(rawBlob);
  if (!buf || buf.length === 0) {
    return { sourceTrajectoryStepInfo: { stepIndex: fallbackIdx, cascadeId: '' } };
  }
  const state = { off: 0 };
  const end = buf.length;

  let createdAt = '';
  let viewableAt = '';
  let finishedGeneratingAt = '';
  let completedAt = '';
  let startedAt = '';
  let modelUsage = null;
  let generatorModel = '';
  let toolCall = null;
  let retryInfos = null;
  let toolSummary = '';
  let stepIndex = fallbackIdx;
  let sawStepIndex = false;
  let originCascadeId = '';

  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 1 && wt === 2) {
      createdAt = decodeProtoTimestampSlice(buf, state, readSubEnd(buf, state, end));
    } else if (fn === 4 && wt === 2) {
      toolCall = decodeProtoToolCallSlice(buf, state, readSubEnd(buf, state, end), true);
    } else if (fn === 6 && wt === 2) {
      viewableAt = decodeProtoTimestampSlice(buf, state, readSubEnd(buf, state, end));
    } else if (fn === 7 && wt === 2) {
      finishedGeneratingAt = decodeProtoTimestampSlice(buf, state, readSubEnd(buf, state, end));
    } else if (fn === 8 && wt === 2) {
      completedAt = decodeProtoTimestampSlice(buf, state, readSubEnd(buf, state, end));
    } else if (fn === 9 && wt === 2) {
      modelUsage = decodeProtoModelUsageSlice(buf, state, readSubEnd(buf, state, end));
    } else if (fn === 11 && wt === 0) {
      generatorModel = decodeProtoModelEnumInt(readProtoVarint(buf, state, end));
    } else if (fn === 20 && wt === 2) {
      const siEnd = readSubEnd(buf, state, end);
      while (state.off < siEnd) {
        const siTag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, siEnd)));
        const siFn = siTag >>> 3;
        const siWt = siTag & 7;
        if (siFn === 2 && siWt === 0) {
          stepIndex = Number(BigInt.asUintN(32, readProtoVarint(buf, state, siEnd)));
          sawStepIndex = true;
        } else if (siFn === 4 && siWt === 2) {
          originCascadeId = readProtoString(buf, state, siEnd).trim().toLowerCase();
        } else {
          skipProtoField(buf, state, siWt, siEnd);
        }
      }
      state.off = siEnd;
    } else if (fn === 28 && wt === 2) {
      const ri = decodeProtoRetryInfoSlice(buf, state, readSubEnd(buf, state, end));
      if (!retryInfos) retryInfos = [];
      retryInfos.push(ri);
    } else if (fn === 30 && wt === 2) {
      toolSummary = readProtoString(buf, state, end);
    } else if (fn === 32 && wt === 2) {
      startedAt = decodeProtoTimestampSlice(buf, state, readSubEnd(buf, state, end));
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }

  return {
    ...(createdAt ? { createdAt } : {}),
    ...(viewableAt ? { viewableAt } : {}),
    ...(finishedGeneratingAt ? { finishedGeneratingAt } : {}),
    ...(completedAt ? { completedAt } : startedAt && !finishedGeneratingAt ? { completedAt: startedAt } : {}),
    ...(startedAt ? { startedAt } : {}),
    ...(generatorModel ? { generatorModel } : {}),
    ...(toolCall ? { toolCall } : {}),
    ...(retryInfos ? { retryInfos } : {}),
    ...(toolSummary ? { toolSummary } : {}),
    sourceTrajectoryStepInfo: {
      stepIndex: sawStepIndex ? stepIndex : fallbackIdx,
      ...(originCascadeId ? { cascadeId: originCascadeId } : {}),
    },
    ...(modelUsage ? { modelUsage } : {}),
  };
}

function hasNonWhitespaceUtf8(buf, start, end) {
  for (let i = start; i < end; i++) {
    const b = buf[i];
    if (b !== 0x20 && b !== 0x09 && b !== 0x0a && b !== 0x0d) return true;
  }
  return false;
}

function decodeProtoPlannerResponseSlice(buf, state, end, keepFullText) {
  const pr = {};
  let hasText = false;
  let toolCalls = null;

  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if ((fn === 1 || fn === 8) && wt === 2) {
      if (keepFullText) {
        const str = readProtoString(buf, state, end);
        if (str) {
          if (fn === 1) pr.response = str;
          else pr.modifiedResponse = str;
        }
      } else {
        const sEnd = readSubEnd(buf, state, end);
        if (!hasText && hasNonWhitespaceUtf8(buf, state.off, sEnd)) {
          hasText = true;
        }
        state.off = sEnd;
      }
    } else if ((fn === 3 || fn === 16) && wt === 2) {
      if (keepFullText) {
        const str = readProtoString(buf, state, end);
        if (str) {
          if (fn === 3) pr.thinking = str;
          else pr.rawThinking = str;
        }
      } else {
        skipProtoField(buf, state, wt, end);
      }
    } else if (fn === 7 && wt === 2) {
      if (!toolCalls) toolCalls = [];
      toolCalls.push(decodeProtoToolCallSlice(buf, state, readSubEnd(buf, state, end), keepFullText));
    } else if (fn === 11 && wt === 2) {
      const dur = decodeProtoDurationSlice(buf, state, readSubEnd(buf, state, end));
      if (dur) pr.thinkingDuration = dur;
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  state.off = end;
  if (!keepFullText && hasText) {
    pr.response = '1';
  }
  if (toolCalls) {
    pr.toolCalls = toolCalls;
  }
  return pr;
}

function decodeProtoUserInputSlice(buf, state, end) {
  let query = '';
  let userResponse = '';
  const items = [];
  let hasMedia = false;

  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 1 && wt === 2) {
      query = readProtoString(buf, state, end);
    } else if (fn === 2 && wt === 2) {
      userResponse = readProtoString(buf, state, end);
    } else if (fn === 3 && wt === 2) {
      const itEnd = readSubEnd(buf, state, end);
      let itemText = '';
      while (state.off < itEnd) {
        const iTag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, itEnd)));
        if ((iTag >>> 3) === 1 && (iTag & 7) === 2) {
          itemText = readProtoString(buf, state, itEnd);
        } else {
          skipProtoField(buf, state, iTag & 7, itEnd);
        }
      }
      state.off = itEnd;
      if (itemText) items.push({ text: itemText });
    } else if ((fn === 4 || fn === 5 || fn === 6) && wt === 2) {
      const mEnd = readSubEnd(buf, state, end);
      if (mEnd > state.off) hasMedia = true;
      state.off = mEnd;
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  state.off = end;

  const primaryText = userResponse || query || items.map((x) => x.text).join('\n') || '';
  if (!primaryText && !hasMedia) return null;
  return {
    ...(primaryText ? { userResponse: primaryText } : {}),
    ...(items.length > 0 ? { items } : {}),
    ...(hasMedia ? { media: [{}] } : {}),
  };
}

function decodeProtoInvokeSubagentSlice(buf, state, end) {
  const subagents = [];
  const results = [];
  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 1 && wt === 2) {
      const spec = decodeProtoSubagentSpecSlice(buf, state, readSubEnd(buf, state, end));
      subagents.push(spec || { typeName: '', role: '' });
    } else if (fn === 2 && wt === 2) {
      const resEnd = readSubEnd(buf, state, end);
      let conversationId = '';
      while (state.off < resEnd) {
        const rTag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, resEnd)));
        if ((rTag >>> 3) === 1 && (rTag & 7) === 2) {
          conversationId = readProtoString(buf, state, resEnd).trim().toLowerCase();
        } else {
          skipProtoField(buf, state, rTag & 7, resEnd);
        }
      }
      state.off = resEnd;
      results.push({ conversationId });
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  state.off = end;
  return { subagents, results };
}

function decodeProtoGenericStepInvokeSubagentSlice(buf, state, end) {
  let invokeSubagent = null;
  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 2 && wt === 2) {
      const resEnd = readSubEnd(buf, state, end);
      while (state.off < resEnd) {
        const resTag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, resEnd)));
        if ((resTag >>> 3) === 6 && (resTag & 7) === 2) {
          const anyEnd = readSubEnd(buf, state, resEnd);
          let typeUrl = '';
          let valStart = 0;
          let valEnd = 0;
          while (state.off < anyEnd) {
            const aTag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, anyEnd)));
            if ((aTag >>> 3) === 1 && (aTag & 7) === 2) {
              typeUrl = readProtoString(buf, state, anyEnd);
            } else if ((aTag >>> 3) === 2 && (aTag & 7) === 2) {
              valEnd = readSubEnd(buf, state, anyEnd);
              valStart = state.off;
              state.off = valEnd;
            } else {
              skipProtoField(buf, state, aTag & 7, anyEnd);
            }
          }
          state.off = anyEnd;
          if (
            valEnd > valStart &&
            (typeUrl.endsWith('/exa.cortex.core.tools.InvokeSubagent') ||
              typeUrl === 'exa.cortex.core.tools.InvokeSubagent')
          ) {
            invokeSubagent = decodeProtoInvokeSubagentSlice(buf, { off: valStart }, valEnd);
          }
        } else {
          skipProtoField(buf, state, resTag & 7, resEnd);
        }
      }
      state.off = resEnd;
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  state.off = end;
  return invokeSubagent;
}

function decodeProtoSystemMessageSlice(buf, state, end) {
  let message = '';
  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 1 && wt === 2) {
      message = readProtoString(buf, state, end);
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  state.off = end;
  return message;
}

function decodeProtoSelectiveStepPayload(
  rawBlob,
  { stepTypeNum, isInvokeSubagent, includeSystemMessage = false, keepFullText }
) {
  const buf = toUint8Array(rawBlob);
  if (!buf || buf.length === 0) return {};
  const state = { off: 0 };
  const end = buf.length;
  const out = {};

  while (state.off < end) {
    const tag = Number(BigInt.asUintN(32, readProtoVarint(buf, state, end)));
    const fn = tag >>> 3;
    const wt = tag & 7;
    if (fn === 19 && wt === 2 && stepTypeNum === 14) {
      const ui = decodeProtoUserInputSlice(buf, state, readSubEnd(buf, state, end));
      if (ui) out.userInput = ui;
    } else if (fn === 20 && wt === 2 && stepTypeNum === 15) {
      out.plannerResponse = decodeProtoPlannerResponseSlice(
        buf,
        state,
        readSubEnd(buf, state, end),
        keepFullText
      );
    } else if (fn === 24 && wt === 2 && stepTypeNum === 17) {
      const em = decodeProtoErrorMessageSlice(buf, state, readSubEnd(buf, state, end));
      if (em) out.errorMessage = em;
    } else if (fn === 114 && wt === 2 && stepTypeNum === 101 && includeSystemMessage) {
      const sysMsg = decodeProtoSystemMessageSlice(buf, state, readSubEnd(buf, state, end));
      if (sysMsg) out.systemMessage = { message: sysMsg };
    } else if (fn === 140 && wt === 2 && isInvokeSubagent) {
      const inv = decodeProtoGenericStepInvokeSubagentSlice(buf, state, readSubEnd(buf, state, end));
      if (inv) out.invokeSubagent = inv;
    } else if (fn === 143 && wt === 2 && isInvokeSubagent) {
      out.invokeSubagent = decodeProtoInvokeSubagentSlice(buf, state, readSubEnd(buf, state, end));
    } else {
      skipProtoField(buf, state, wt, end);
    }
  }
  return out;
}

function isInvokeSubagentToolName(rawName) {
  return /^(?:default_api:)?invoke_subagent$/i.test(String(rawName || '').trim());
}

function mapRunningGenericToolStepType(rawToolName) {
  const name = String(rawToolName || '')
    .trim()
    .toLowerCase()
    .replace(/^default_api:/, '');
  if (!name) return 'CORTEX_STEP_TYPE_GENERIC';
  if (name === 'invoke_subagent') return 'CORTEX_STEP_TYPE_INVOKE_SUBAGENT';
  if (name === 'run_command') return 'CORTEX_STEP_TYPE_RUN_COMMAND';
  if (name === 'command_status') return 'CORTEX_STEP_TYPE_COMMAND_STATUS';
  if (name === 'send_command_input') return 'CORTEX_STEP_TYPE_SEND_COMMAND_INPUT';
  if (
    name === 'read_file' ||
    name === 'view_file' ||
    name === 'view_file_outline' ||
    name === 'view_code_item'
  ) {
    return 'CORTEX_STEP_TYPE_VIEW_FILE';
  }
  if (
    name === 'edit_file' ||
    name === 'write_file' ||
    name === 'write_to_file' ||
    name === 'replace_file_content' ||
    name === 'multi_replace_file_content'
  ) {
    return 'CORTEX_STEP_TYPE_CODE_ACTION';
  }
  if (name === 'grep_search' || name === 'ripgrep') return 'CORTEX_STEP_TYPE_GREP_SEARCH';
  if (name === 'find_by_name') return 'CORTEX_STEP_TYPE_FIND';
  if (name === 'list_dir') return 'CORTEX_STEP_TYPE_LIST_DIRECTORY';
  if (name === 'search_web') return 'CORTEX_STEP_TYPE_SEARCH_WEB';
  if (name === 'read_url_content' || name === 'fetch_url') return 'CORTEX_STEP_TYPE_READ_URL_CONTENT';
  if (name === 'ask_question') return 'CORTEX_STEP_TYPE_ASK_QUESTION';
  if (name === 'call_mcp_tool' || name.startsWith('mcp_') || name.startsWith('mcp-')) {
    return 'CORTEX_STEP_TYPE_MCP_TOOL';
  }
  if (name === 'list_resources') return 'CORTEX_STEP_TYPE_LIST_RESOURCES';
  if (name === 'read_resource') return 'CORTEX_STEP_TYPE_READ_RESOURCE';
  return 'CORTEX_STEP_TYPE_GENERIC';
}

function cleanShortPromptTitle(rawText, maxChars = 80) {
  const withoutTags = String(rawText || '')
    .replace(/<CONTEXT_SUMMARY>[\s\S]*?<\/CONTEXT_SUMMARY>/gi, '')
    .replace(/<SYSTEM_MESSAGE>[\s\S]*$/i, '')
    .replace(/<ADDITIONAL_METADATA>[\s\S]*$/i, '');
  const reqMatches = [...withoutTags.matchAll(/<USER_REQUEST>([\s\S]*?)<\/USER_REQUEST>/gi)];
  const target = reqMatches.length > 0 ? reqMatches[reqMatches.length - 1][1] : withoutTags;
  const singleLine = target.replace(/\s+/g, ' ').trim();
  if (!singleLine) return '';
  return singleLine.length <= maxChars ? singleLine : `${singleLine.slice(0, maxChars - 1).trimEnd()}…`;
}

function hasContextBreakdownPayload(chat) {
  if (!chat || typeof chat !== 'object') return false;
  return Boolean(
    chat.chatStartMetadata?.contextWindowMetadata?.tokenBreakdown?.groups?.length ||
      chat.promptSections?.length ||
      chat.tools?.length ||
      chat.messagePrompts?.length
  );
}

/**
 * 从单个会话的本地 SQLite 数据库（`<product>/conversations/<cascadeId>.db`）提取步骤元数据、按需载荷与生成器元数据。
 * 自动过滤分叉/旁路提问克隆自父会话的步骤（`originCid !== cascadeId`），并按需解码末尾上下文拆解。
 *
 * @param {string} dbFilePath
 * @param {{ cascadeId?: string, isRunning?: boolean }} [options={}]
 */
export function readLocalSqliteTrajectory(dbFilePath, options = {}) {
  if (!dbFilePath || !existsSync(dbFilePath)) return null;
  const DatabaseSync = getNodeSqliteDatabaseSync();
  if (!DatabaseSync) return null;

  let normCid = String(options?.cascadeId || '')
    .trim()
    .toLowerCase();
  if (!normCid) {
    const stem = basename(dbFilePath).replace(/\.db$/i, '').trim().toLowerCase();
    if (UUID_FILE_RE.test(stem)) {
      normCid = stem;
    }
  }
  const isRunning = Boolean(options?.isRunning);
  // Anti-Power: keep full response and thinking text for every step, not just the running tail
  const fullText = Boolean(options?.fullText);

  try {
    if (!hasTrajectorySchema(dbFilePath)) return null;
    return withReadOnlySqliteDb(dbFilePath, (db) => {
      let trajectoryType = 0;
      try {
        const metaRow = db
          .prepare(
            'SELECT trajectory_id, cascade_id, trajectory_type, source FROM trajectory_meta LIMIT 1'
          )
          .get();
        if (!normCid && metaRow?.cascade_id) {
          normCid = String(metaRow.cascade_id).trim().toLowerCase();
        }
        trajectoryType = Number(metaRow?.trajectory_type || 0);
      } catch {
        // 兼容未包含 trajectory_meta 表的精简库
      }
      const isSideQuestion = trajectoryType === 22;

      let trajectoryMetadata = {
        createdAt: '',
        parentConversationId: '',
        rootConversationId: '',
        nestingDepth: 0,
        subagentSpec: null,
        agentScript: null,
      };
      try {
        const blobRow = db
          .prepare("SELECT data FROM trajectory_metadata_blob WHERE id = 'main' LIMIT 1")
          .get();
        const blobBuf = toUint8Array(blobRow?.data);
        if (blobBuf && blobBuf.length > 0) {
          trajectoryMetadata = decodeProtoTrajectoryMetadataSlice(
            blobBuf,
            { off: 0 },
            blobBuf.length
          );
        }
      } catch {
        // 兼容未包含 trajectory_metadata_blob 表的库
      }

      let forkParentConversationId = '';
      try {
        const refRows = db.prepare('SELECT idx, data FROM parent_references ORDER BY idx ASC').all();
        for (const refRow of refRows || []) {
          const refBuf = toUint8Array(refRow?.data);
          if (!refBuf || refBuf.length === 0) continue;
          const st = { off: 0 };
          let refCid = '';
          while (st.off < refBuf.length) {
            const tag = Number(BigInt.asUintN(32, readProtoVarint(refBuf, st, refBuf.length)));
            if ((tag >>> 3) === 6 && (tag & 7) === 2) {
              refCid = readProtoString(refBuf, st, refBuf.length).trim().toLowerCase();
            } else {
              skipProtoField(refBuf, st, tag & 7, refBuf.length);
            }
          }
          if (refCid && (!normCid || refCid !== normCid)) {
            forkParentConversationId = refCid;
          }
        }
      } catch {
        // 兼容未包含 parent_references 表的库
      }

      const genRows = db.prepare('SELECT idx, data FROM gen_metadata ORDER BY idx ASC').all();
      const ownGenRecords = [];
      let rootConversationId = trajectoryMetadata.rootConversationId || '';
      let parentConversationId = trajectoryMetadata.parentConversationId || '';

      for (const row of genRows || []) {
        if (!row?.data) continue;
        try {
          const decoded = decodeProtoGeneratorMeta(row.data, false);
          if (!decoded) continue;
          const gmCid = String(decoded.extraMeta?.cascade_id || '')
            .trim()
            .toLowerCase();
          if (gmCid && normCid && gmCid !== normCid) {
            // 过滤旁路提问或会话分叉从父会话克隆来的历史生成器记录
            continue;
          }
          if (!rootConversationId && decoded.extraMeta?.root_cascade_id) {
            rootConversationId = String(decoded.extraMeta.root_cascade_id).trim().toLowerCase();
          }
          if (!parentConversationId && decoded.extraMeta?.parent_cascade_id) {
            parentConversationId = String(decoded.extraMeta.parent_cascade_id).trim().toLowerCase();
          }
          ownGenRecords.push({ rawData: row.data, gm: decoded });
        } catch {
          // 忽略单条损坏的 generatorMetadata
        }
      }

      if (ownGenRecords.length > 0) {
        // 从末尾向前定位最近一条包含完整上下文拆解（tokenBreakdown / promptSections / tools / messagePrompts）的记录，
        // 避免末尾紧跟 CHECKPOINT 压缩步或多条纯 plannerConfig 记录时漏解压缩前的静态上下文构成。
        for (let i = ownGenRecords.length - 1; i >= 0; i--) {
          const rec = ownGenRecords[i];
          try {
            const fullDecoded = decodeProtoGeneratorMeta(rec.rawData, true);
            if (fullDecoded) {
              rec.gm = fullDecoded;
              if (hasContextBreakdownPayload(fullDecoded.chatModel)) {
                break;
              }
            }
          } catch {
            // 保留轻量版本
          }
        }
      }

      const generatorMetadata = [];
      const genByStep = new Map();
      for (const { gm } of ownGenRecords) {
        const gmItem = {
          stepIndices: gm.stepIndices,
          ...(gm.chatModel ? { chatModel: gm.chatModel } : {}),
          ...(gm.plannerConfig ? { plannerConfig: gm.plannerConfig } : {}),
        };
        generatorMetadata.push(gmItem);
        for (const sIdx of gm.stepIndices) {
          genByStep.set(Number(sIdx), gmItem);
        }
      }

      const stepRows = db
        .prepare('SELECT idx, step_type, status, metadata FROM steps ORDER BY idx ASC')
        .all();
      const ownStepRecords = [];
      for (const row of stepRows || []) {
        const idx = Number.isFinite(Number(row?.idx)) ? Number(row.idx) : ownStepRecords.length;
        const stepTypeNum = Number(row?.step_type || 0);
        const statusNum = Number(row?.status || 0);
        let metadata = { sourceTrajectoryStepInfo: { stepIndex: idx } };
        if (row?.metadata && row.metadata.byteLength > 0) {
          try {
            metadata = decodeProtoStepMeta(row.metadata, idx);
          } catch {
            // 忽略单步元数据解码异常
          }
        }
        const originCid = String(metadata.sourceTrajectoryStepInfo?.cascadeId || '')
          .trim()
          .toLowerCase();
        if (originCid && normCid && originCid !== normCid) {
          // 过滤克隆自父会话的步骤，防止双倍重复计费
          continue;
        }
        ownStepRecords.push({ idx, stepTypeNum, statusNum, metadata });
      }

      const isChildTrajectory =
        !isSideQuestion &&
        Boolean(
          trajectoryMetadata.parentConversationId ||
            parentConversationId ||
            (trajectoryMetadata.rootConversationId &&
              trajectoryMetadata.rootConversationId !== normCid) ||
            (rootConversationId && rootConversationId !== normCid) ||
            trajectoryMetadata.subagentSpec
        );

      const runningTailStart = isRunning
        ? Math.max(0, ownStepRecords.length - 16)
        : ownStepRecords.length;

      const payloadIndices = [];
      try {
        for (let i = 0; i < ownStepRecords.length; i++) {
          const rec = ownStepRecords[i];
          const inRunningTail = isRunning && i >= runningTailStart;
          const isInvSub =
            rec.stepTypeNum === 127 ||
            (rec.stepTypeNum === 132 && isInvokeSubagentToolName(rec.metadata?.toolCall?.name));
          if (
            rec.stepTypeNum === 14 ||
            rec.stepTypeNum === 15 ||
            rec.stepTypeNum === 17 ||
            (isChildTrajectory && rec.stepTypeNum === 101) ||
            isInvSub ||
            (inRunningTail && (rec.stepTypeNum !== 132 || !rec.metadata?.toolCall?.argumentsJson))
          ) {
            payloadIndices.push(rec.idx);
          }
        }
      } catch {
        // 忽略索引构建异常
      }

      const payloadByIdx = new Map();
      if (payloadIndices.length > 0) {
        try {
          for (let off = 0; off < payloadIndices.length; off += SQLITE_IN_CHUNK_SIZE) {
            const batch = payloadIndices.slice(off, off + SQLITE_IN_CHUNK_SIZE);
            const placeholders = batch.map(() => '?').join(',');
            const pRows = db
              .prepare(`SELECT idx, step_payload FROM steps WHERE idx IN (${placeholders})`)
              .all(...batch);
            for (const pRow of pRows || []) {
              if (pRow?.step_payload && pRow.step_payload.byteLength > 0) {
                payloadByIdx.set(Number(pRow.idx), pRow.step_payload);
              }
            }
          }
        } catch {
          // 兼容不含 step_payload 列的旧表
        }
      }

      let firstOwnUserPrompt = '';
      const steps = [];
      for (let i = 0; i < ownStepRecords.length; i++) {
        const rec = ownStepRecords[i];
        const inRunningTail = isRunning && i >= runningTailStart;
        const isInvSub =
          rec.stepTypeNum === 127 ||
          (rec.stepTypeNum === 132 && isInvokeSubagentToolName(rec.metadata?.toolCall?.name));

        let type = '';
        if (isInvSub) {
          type = 'CORTEX_STEP_TYPE_INVOKE_SUBAGENT';
        } else if (rec.stepTypeNum === 132 && inRunningTail) {
          type = mapRunningGenericToolStepType(rec.metadata?.toolCall?.name);
        } else {
          type = decodeProtoStepTypeEnum(rec.stepTypeNum);
        }
        const status = decodeProtoStepStatusEnum(rec.statusNum);
        const metadata = rec.metadata;
        const stepIdx = metadata.sourceTrajectoryStepInfo?.stepIndex ?? rec.idx;
        const gm = genByStep.get(Number(stepIdx));
        if (metadata.modelUsage && gm?.chatModel?.model && !metadata.modelUsage.model) {
          metadata.modelUsage.model = gm.chatModel.model;
        }

        const stepObj = { idx: rec.idx, type, status, metadata };
        const rawPayload = payloadByIdx.get(rec.idx);
        if (rawPayload) {
          try {
            const decodedPayload = decodeProtoSelectiveStepPayload(rawPayload, {
              stepTypeNum: rec.stepTypeNum,
              isInvokeSubagent: isInvSub,
              includeSystemMessage: isChildTrajectory && rec.stepTypeNum === 101,
              keepFullText: inRunningTail || fullText,
            });
            if (decodedPayload.userInput) {
              stepObj.userInput = decodedPayload.userInput;
              if (!firstOwnUserPrompt && decodedPayload.userInput.userResponse) {
                firstOwnUserPrompt = cleanShortPromptTitle(
                  decodedPayload.userInput.userResponse,
                  80
                );
              }
            }
            if (decodedPayload.systemMessage) {
              stepObj.systemMessage = decodedPayload.systemMessage;
              stepObj.content = decodedPayload.systemMessage.message;
            }
            if (decodedPayload.plannerResponse) {
              stepObj.plannerResponse = decodedPayload.plannerResponse;
            }
            if (decodedPayload.errorMessage) {
              stepObj.errorMessage = decodedPayload.errorMessage;
            }
            if (decodedPayload.invokeSubagent) {
              stepObj.invokeSubagent = decodedPayload.invokeSubagent;
            }
          } catch {
            // 忽略单步载荷解析异常
          }
        }
        steps.push(stepObj);
      }

      // Anti-Power: distinguish a readable empty trajectory from a failed database read
      if (!options.includeEmpty && steps.length === 0 && generatorMetadata.length === 0 && !isSideQuestion) {
        return null;
      }

      const mergedMetadata = {
        ...trajectoryMetadata,
        parentConversationId: trajectoryMetadata.parentConversationId || parentConversationId,
        rootConversationId: trajectoryMetadata.rootConversationId || rootConversationId,
      };

      return {
        cascadeId: normCid,
        numTotalSteps: steps.length,
        rawTableStepCount: stepRows.length,
        trajectoryType,
        isSideQuestion,
        forkParentConversationId,
        firstOwnUserPrompt,
        steps,
        generatorMetadata,
        rootConversationId: mergedMetadata.rootConversationId,
        parentConversationId: mergedMetadata.parentConversationId,
        trajectory: {
          metadata: mergedMetadata,
          steps,
          generatorMetadata,
        },
      };
    });
  } catch {
    return null;
  }
}
