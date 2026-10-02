// Server-side client. Credentials stay on disk; rotation is picked up on every send.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

export function connectionFile() {
  return process.env.DUN_CONNECTION_FILE || (process.env.APPDATA && join(process.env.APPDATA, 'app.dun', 'integration-connection.json'));
}

export async function sendReminder(payload, file = connectionFile(), fetcher = fetch) {
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
    response = await fetcher(new URL('/v1/reminders', target), {
      method: 'POST', signal: AbortSignal.timeout(5000),
      headers: { Authorization: `Bearer ${config.token}`, 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
  } catch { throw new Error('Dun is unavailable. Keep it running in the tray and try again.'); }
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error || `Dun refused the reminder (${response.status}).`);
  if (typeof body.id !== 'string' || typeof body.created !== 'boolean') throw new Error('Dun returned an unexpected response. The reminder has not been confirmed.');
  return body;
}
