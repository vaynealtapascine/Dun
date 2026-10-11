// Server-side client. Credentials stay on disk; rotation is picked up on every send.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

export function connectionFile() {
  return process.env.DUN_CONNECTION_FILE || (process.env.APPDATA && join(process.env.APPDATA, 'app.dun', 'integration-connection.json'));
}

async function request(path, method, payload, file = connectionFile(), fetcher = fetch) {
  if (!file) throw new Error('Set DUN_CONNECTION_FILE to Dun’s integration-connection.json on its PC.');
  let config;
  try { config = JSON.parse(readFileSync(file, 'utf8')); }
  catch { throw new Error('Open Dun → Settings → App connections and enable connections on this PC.'); }
  if (!config.enabled || !config.token) throw new Error('App connections are disabled in Dun.');
  const target = new URL(config.url);
  if (target.protocol !== 'http:' || target.hostname !== '127.0.0.1' || target.username || target.password) {
    throw new Error('Dun’s connection must use http://127.0.0.1 on the same PC.');
  }
  let response;
  try {
    response = await fetcher(new URL(path, target), {
      method, signal: AbortSignal.timeout(5000), redirect: 'error',
      headers: { Authorization: `Bearer ${config.token}`, 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
  } catch { throw new Error('Dun is unavailable. Keep it running in the tray and try again.'); }
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error || `Dun refused the request (${response.status}).`);
  return body;
}

async function create(path, payload, file, fetcher) {
  const body = await request(path, 'POST', payload, file, fetcher);
  if (typeof body.id !== 'string' || typeof body.created !== 'boolean') throw new Error('Dun returned an unexpected response. The reminder has not been confirmed.');
  return body;
}

export function sendReminder(payload, file, fetcher) {
  return create('/v1/reminders', payload, file, fetcher);
}

export function sendTimer(payload, file, fetcher) {
  return create('/v1/timers', payload, file, fetcher);
}

export function listItems(includeArchived = false, file, fetcher) {
  return request(`/v1/items?includeArchived=${includeArchived ? 'true' : 'false'}`, 'GET', undefined, file, fetcher);
}

export function getItem(id, file, fetcher) {
  return request(`/v1/items/${encodeURIComponent(id)}`, 'GET', undefined, file, fetcher);
}

// No automatic retries: snooze/start/reset are time-relative actions.
export function actOnItem(id, action, file, fetcher) {
  return request(`/v1/items/${encodeURIComponent(id)}/actions`, 'POST', action, file, fetcher);
}

// No credentials in links. Opening a creation link prepares a form to review.
export function deepLink(kind, params = {}) {
  if (!['open', 'item', 'reminder', 'timer'].includes(kind)) throw new Error('Unsupported Dun link kind.');
  const query = new URLSearchParams(params).toString();
  return `dun://${kind}${query ? `?${query}` : ''}`;
}
