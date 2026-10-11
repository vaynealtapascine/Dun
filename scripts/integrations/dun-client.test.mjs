import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { sendReminder, sendTimer, listItems, getItem, actOnItem, deepLink } from './dun-client.mjs';

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

test('item client encodes IDs, uses authenticated routes, and never follows redirects', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'dun-client-'));
  const file = join(dir, 'connection.json');
  writeFileSync(file, JSON.stringify({ enabled: true, token: 'key', url: 'http://127.0.0.1:48475' }));
  const calls = [];
  const fake = async (url, options) => {
    calls.push([url.pathname + url.search, options.method, options.body]);
    assert.equal(options.headers.Authorization, 'Bearer key');
    assert.equal(options.redirect, 'error');
    return new Response(JSON.stringify({ id: 'api-one', created: true }));
  };
  try {
    await sendTimer({ title: 'Tea' }, file, fake);
    await listItems(true, file, fake);
    await getItem('a/b?#', file, fake);
    await actOnItem('api-one', { action: 'pause' }, file, fake);
    assert.deepEqual(calls, [
      ['/v1/timers', 'POST', '{"title":"Tea"}'],
      ['/v1/items?includeArchived=true', 'GET', undefined],
      ['/v1/items/a%2Fb%3F%23', 'GET', undefined],
      ['/v1/items/api-one/actions', 'POST', '{"action":"pause"}'],
    ]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('deep links encode Unicode, separators and timezone offsets', () => {
  const link = deepLink('reminder', { title: 'Café & tea', dueAt: '2026-10-12T09:00:00+08:00' });
  assert.equal(new URL(link).searchParams.get('title'), 'Café & tea');
  assert.match(link, /%2B08%3A00/);
  assert.equal(deepLink('open'), 'dun://open');
  assert.throws(() => deepLink('done'), /Unsupported/);
});
