// Build minimal Antigravity conversation databases for tests
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';

const varint = (value) => {
  let v = BigInt(value);
  const out = [];
  do {
    let byte = Number(v & 0x7fn);
    v >>= 7n;
    if (v) byte |= 0x80;
    out.push(byte);
  } while (v);
  return out;
};
const key = (field, wire) => varint((field << 3) | wire);
const int = (field, value) => [...key(field, 0), ...varint(value)];
const bytes = (field, data) => [...key(field, 2), ...varint(data.length), ...data];
const str = (field, text) => bytes(field, [...Buffer.from(text, 'utf8')]);
const msg = (field, ...parts) => bytes(field, parts.flat());
const ts = (field, ms) => msg(field, int(1, Math.floor(ms / 1000)), int(2, (ms % 1000) * 1e6));

const USER_INPUT = 14;
const PLANNER_RESPONSE = 15;
const DONE = 3;
// MODEL_PLACEHOLDER_M437, which the bundled catalog maps to Gemini 4 Carbon (Medium)
const MODEL = 1437;

// Write `<root>/conversations/<id>.db` with one prompt and one model response per turn
export function writeConversation(root, id, turns) {
  const dir = join(root, 'conversations');
  mkdirSync(dir, { recursive: true });
  const db = new DatabaseSync(join(dir, `${id}.db`));
  db.exec(`
    CREATE TABLE trajectory_meta (trajectory_id text, cascade_id text, trajectory_type integer, source integer, PRIMARY KEY (trajectory_id));
    CREATE TABLE steps (idx integer, step_type integer NOT NULL DEFAULT 0, status integer NOT NULL DEFAULT 0,
      has_subtrajectory numeric NOT NULL DEFAULT false, metadata blob, error_details blob, permissions blob,
      task_details blob, render_info blob, step_payload blob, step_format integer NOT NULL DEFAULT 0, PRIMARY KEY (idx));
    CREATE TABLE gen_metadata (idx integer, data blob, size integer NOT NULL DEFAULT 0, PRIMARY KEY (idx));
    CREATE TABLE parent_references (idx integer, data blob, PRIMARY KEY (idx));
    CREATE TABLE trajectory_metadata_blob (id text DEFAULT "main", data blob, PRIMARY KEY (id));
  `);
  db.prepare('INSERT INTO trajectory_meta VALUES (?, ?, 0, 0)').run(`t-${id}`, id);
  const insert = db.prepare(
    'INSERT INTO steps (idx, step_type, status, metadata, step_payload) VALUES (?, ?, ?, ?, ?)'
  );
  const generator = db.prepare('INSERT INTO gen_metadata (idx, data) VALUES (?, ?)');
  let generations = 0;
  let idx = 0;
  for (const turn of turns) {
    const origin = (index) => msg(20, int(2, index), str(4, id));
    insert.run(
      idx,
      USER_INPUT,
      DONE,
      Uint8Array.from([...ts(1, turn.atMs), ...origin(idx)]),
      Uint8Array.from(msg(19, str(2, turn.prompt)))
    );
    idx++;
    const { atMs, ttftMs, durationMs, usage, model = MODEL, omitRequestTime = false } = turn;
    const metadata = [
      ...(omitRequestTime ? [] : ts(1, atMs)),
      ...ts(6, atMs + ttftMs),
      ...ts(7, atMs + durationMs),
      ...ts(8, atMs + durationMs),
      ...msg(
        9,
        model == null ? [] : int(1, model),
        int(2, usage.input),
        int(3, usage.thinking + usage.response),
        int(5, usage.cacheRead),
        int(6, 24),
        int(9, usage.thinking),
        int(10, usage.response)
      ),
      ...(model == null ? [] : int(11, model)),
      ...origin(idx),
    ];
    insert.run(
      idx,
      PLANNER_RESPONSE,
      DONE,
      Uint8Array.from(metadata),
      Uint8Array.from(msg(20, str(1, turn.response), str(3, turn.thinking || ''),
        ...(turn.toolCalls ?? []).map((call) => msg(7, str(2, call.name), str(3, call.arguments)))))
    );
    // The client records time to first token in the generator metadata
    const ttft = msg(11, int(1, Math.floor(ttftMs / 1000)), int(2, (ttftMs % 1000) * 1e6));
    generator.run(generations++, Uint8Array.from([...int(2, idx), ...msg(1, ttft)]));
    idx++;
  }
  db.close();
}

export const TURN = {
  atMs: Date.parse('2026-11-18T10:00:00Z'),
  ttftMs: 1500,
  durationMs: 4500,
  prompt: 'Explain <b>caching</b>',
  thinking: 'Consider the cache',
  response: 'Caching stores results.',
  usage: { input: 10_000, cacheRead: 50_000, thinking: 200, response: 400 },
};

// 10k new input at $2, 50k cache reads at $0.10, 600 output at $10, per million tokens
export const TURN_COST = 0.02 + 0.005 + 0.006;
