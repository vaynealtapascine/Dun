import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { sendReminder } from './dun-client.mjs';

test('credentials rotate per send; disabled/remote connections and errors reach the caller', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'dun-client-'));
  const file = join(dir, 'connection.json');
  const save = (token, enabled = true, url = 'http://127.0.0.1:48475') => writeFileSync(file, JSON.stringify({ token, enabled, url }));
  const payload = { source: 'Memos', externalId: 'memo/1/occurrence', title: 'Read', dueAt: new Date().toISOString() };
  const tokens = [];
  const fake = async (url, request) => {
    assert.equal(url.pathname, '/v1/reminders');
    assert.deepEqual(JSON.parse(request.body), payload);
    tokens.push(request.headers.Authorization);
    return new Response(JSON.stringify({ id: 'api-one', created: tokens.length === 1 }));
  };
  try {
    save('one'); await sendReminder(payload, file, fake);
    save('two'); await sendReminder(payload, file, fake);
    assert.deepEqual(tokens, ['Bearer one', 'Bearer two']);
    save('two', false); await assert.rejects(sendReminder(payload, file, fake), /disabled/);
    save('two', true, 'https://example.com'); await assert.rejects(sendReminder(payload, file, fake), /same PC/);
    save('two'); await assert.rejects(sendReminder(payload, file, async () => { throw new Error('offline'); }), /unavailable/);
    await assert.rejects(sendReminder(payload, file, async () => new Response('{"error":"Invalid key"}', { status: 401 })), /Invalid key/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
