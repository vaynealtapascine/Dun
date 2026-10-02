// Add Dun delivery to the existing multi-user Memos reminder service.
// The original script/config are backed up locally; credentials aren't copied into this repo.
import { copyFileSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { connectionFile } from './dun-client.mjs';

export function patchMemos(source) {
  const replace = (before, after) => {
    if (!source.includes(before)) throw new Error(`Memos script has changed; missing integration point: ${before.slice(0, 60)}`);
    source = source.replace(before, after);
  };
  if (!source.includes('// Dun integration v1')) {
    replace('import { readFileSync,', '// Dun integration v1\nimport { sendReminder } from "./dun-client.mjs";\nimport { readFileSync,');
    replace('topic: cfg.topic };', 'topic: cfg.topic, dun: cfg.dun === true };');
    replace('function notifyDue(user, memo) {', 'async function notifyDue(user, memo, occurrence, key) {');
    replace('  return publish(user.topic, { title, message: body || "Memo reminder",', `  if (user.dun) {
    const payload = { source: "Memos", externalId: user.username + "|" + key,
      title, notes: (memo.content || "").slice(0, 4000), dueAt: occurrence.toISOString(), url: memoUrl(memo.name) };
    if (DRY) return log("[dry] would send to Dun", memo.name, occurrence.toISOString());
    return sendReminder(payload, CONFIG.dunConnectionFile);
  }
  return publish(user.topic, { title, message: body || "Memo reminder",`);
    replace('try { await notifyDue(user, memo); } catch (e) { log("notify failed:", e.message); continue; }',
      'try { await notifyDue(user, memo, last, key); } catch (e) { log("notify failed:", e.message); nextWake = new Date(Math.min(nextWake, now.getTime() + 60e3)); continue; }');
  }
  if (!source.includes('// Dun Memos tag compatibility v1')) {
    replace('import { sendReminder } from "./dun-client.mjs";', 'import { sendReminder } from "./dun-client.mjs";\n// Dun Memos tag compatibility v1\nimport { reminderParts } from "./memos-tags.mjs";');
    replace('const p = tag.toLowerCase().split("/").slice(1);', 'const p = reminderParts(tag);');
  }
  return source;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const directory = resolve(process.argv[2] || join(process.env.USERPROFILE || '', 'selfhost', 'memos-remind'));
  const file = join(directory, 'memos-remind.mjs');
  const configFile = join(directory, 'config.json');
  const original = readFileSync(file, 'utf8');
  const patched = patchMemos(original);
  const config = JSON.parse(readFileSync(configFile, 'utf8'));
  const originalAccount = config.users?.find(u => u.tokenFile.replaceAll('\\', '/') === 'memos-token.txt');
  if (!originalAccount) throw new Error('Could not find the original Memos account; no files changed.');
  const connection = process.argv[3] || connectionFile();
  if (!connection) throw new Error('Provide the path to Dun’s integration-connection.json; no files changed.');
  originalAccount.dun = true;
  config.dunConnectionFile = connection;
  for (const target of [file, configFile]) {
    if (!existsSync(target + '.before-dun')) copyFileSync(target, target + '.before-dun');
  }
  // Adapter and script first. The existing service watches config and restarts itself last.
  copyFileSync(join(dirname(fileURLToPath(import.meta.url)), 'dun-client.mjs'), join(directory, 'dun-client.mjs'));
  copyFileSync(join(dirname(fileURLToPath(import.meta.url)), 'memos-tags.mjs'), join(directory, 'memos-tags.mjs'));
  writeFileSync(file, patched);
  writeFileSync(configFile, JSON.stringify(config, null, 2) + '\n');
  console.log('Memos is configured to send the original account’s due reminders to Dun. Other accounts keep their existing delivery.');
}
