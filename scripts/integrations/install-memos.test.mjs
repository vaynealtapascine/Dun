import { test } from 'node:test';
import assert from 'node:assert/strict';
import { runInNewContext } from 'node:vm';
import { patchMemos } from './install-memos.mjs';
import { reminderParts } from './memos-tags.mjs';

const fixture = `import { readFileSync, writeFileSync } from "node:fs";
const u = { token: "local", topic: cfg.topic };
function notifyDue(user, memo) {
  const { title, body } = preview(memo.content);
  return publish(user.topic, { title, message: body || "Memo reminder", click: memoUrl(memo.name) });
}
function notifySet() {}
function parseTag(tag) { const p = tag.toLowerCase().split("/").slice(1); return p; }
try { await notifyDue(user, memo); } catch (e) { log("notify failed:", e.message); continue; }
`;

test('Memos routes only opted-in users, uses stable occurrence IDs, keeps dry runs and propagates failures', async () => {
  const patched = patchMemos(fixture);
  assert.equal(patchMemos(patched), patched);
  assert.match(patched, /notifyDue\(user, memo, last, key\)/);
  assert.match(patched, /now.getTime\(\) \+ 60e3/);
  assert.throws(() => patchMemos('changed script'), /missing integration point/);
  const functionSource = patched.slice(patched.indexOf('async function notifyDue'), patched.indexOf('function notifySet'));
  const sent = [], published = [], dryLogs = [];
  let offline = false;
  const context = {
    DRY: false, CONFIG: { dunConnectionFile: 'private-file' },
    preview: () => ({ title: 'Read me', body: 'Details' }), memoUrl: name => `https://memos.example/${name}`,
    log: (...args) => dryLogs.push(args), publish: (...args) => published.push(args),
    sendReminder: async (...args) => { if (offline) throw new Error('offline'); sent.push(args); return { id: 'api-one', created: true }; },
  };
  runInNewContext(functionSource, context);
  const memo = { name: 'memos/one', content: 'Private reminder' };
  const when = new Date('2026-10-01T09:00:00+08:00');
  const user = { username: 'original', topic: 'own-topic', dun: true };
  await context.notifyDue(user, memo, when, 'memos/one|tag|once');
  await context.notifyDue(user, memo, when, 'memos/one|tag|once');
  assert.equal(sent[0][0].externalId, sent[1][0].externalId);
  assert.equal(sent[0][0].dueAt, when.toISOString());
  assert.equal(sent[0][1], 'private-file');
  await context.notifyDue({ username: 'kai', topic: 'kai-topic', dun: false }, memo, when, 'key');
  assert.equal(sent.length, 2); assert.equal(published.length, 1);
  offline = true; await assert.rejects(context.notifyDue(user, memo, when, 'key'), /offline/);
  context.DRY = true; await context.notifyDue(user, memo, when, 'key');
  assert.equal(sent.length, 2); assert.equal(dryLogs.length, 1);
});

test('Memos flattened tags retain all supported reminder shapes and original slash forms', () => {
  for (const [tag, parts] of [
    ['2026-10-01-0416', ['2026-10-01', '0416']],
    ['in-3d', ['in', '3d']], ['daily-0800', ['daily', '0800']],
    ['weekly-mon-0800', ['weekly', 'mon', '0800']], ['weekly-mon', ['weekly', 'mon']],
    ['monthly-15-0900', ['monthly', '15', '0900']], ['monthly-15', ['monthly', '15']],
    ['yearly-09-20-0800', ['yearly', '09-20', '0800']], ['yearly-09-20', ['yearly', '09-20']],
  ]) {
    assert.deepEqual(reminderParts(`remind/${tag}`), parts);
    assert.deepEqual(reminderParts(`remind/${parts.join('/')}`), parts);
  }
  assert.deepEqual(reminderParts('remind/2026-10-01'), ['2026-10-01']);
  assert.deepEqual(reminderParts('remind/daily-nope'), ['daily-nope']);
  const patched = patchMemos(fixture);
  const old = patched.replace('// Dun Memos tag compatibility v1\nimport { reminderParts } from "./memos-tags.mjs";\n', '')
    .replace('const p = reminderParts(tag);', 'const p = tag.toLowerCase().split("/").slice(1);');
  assert.equal(patchMemos(old), patched);
});
