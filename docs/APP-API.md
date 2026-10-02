# App connections

Dun's desktop app accepts reminder and timer creation from other apps on the same PC.
Enable **Settings → App connections → Allow apps to send to Dun**. Dun must stay running
in its tray. Items use Dun's normal storage, nagging, quiet hours and phone sync.

The server binds only `127.0.0.1:48475`, separately from the paired-phone HTTPS server.
Every request requires `Authorization: Bearer <connection key>` and JSON requests require
`Content-Type: application/json`. There is no browser CORS access; browser apps should send
through their own server. Disable connections or generate a new key in Settings to revoke access.

| Method and path | Purpose |
|---|---|
| `GET /v1/health` | Authenticated readiness check: `{ "ok": true, "apiVersion": 1 }` |
| `POST /v1/reminders` | Create a reminder using `dueAt` |
| `POST /v1/timers` | Create and immediately start a timer using `durationSeconds` |

## Request and response

```json
{
  "source": "My app",
  "externalId": "task-42/2026-10-01T09:00:00+08:00",
  "title": "Review the draft",
  "dueAt": "2026-10-01T09:00:00+08:00",
  "notes": "Optional context",
  "url": "https://example.com/tasks/42"
}
```

For a timer, replace `dueAt` with `"durationSeconds": 300`. Durations must be whole
seconds between 1 and 31,536,000. Reminder dates must include a timezone offset or `Z`;
past dates are accepted for delayed deliveries and immediately become due.

`source`, `externalId` and `title` are required (maximum 80, 512 and 300 UTF-8 bytes).
`notes` is optional (16,000 bytes). `url` is optional (HTTP/HTTPS, 2,048 bytes). Unknown
fields, missing dates, invalid durations, and combining a date with a duration are refused.
The request body limit is 24 KiB. Invalid requests never create an item.

Success returns HTTP 200 and:

```json
{ "id": "api-...", "created": true, "archived": false }
```

The pair `source` + `externalId` identifies one delivery. The first accepted payload wins.
Retries return the same ID with `created: false`, even after a restart, completion or
archive. They never restart timers or revive completed reminders. Include an occurrence
time in `externalId` when sending recurring reminders or scheduling another alert for a
task. A completed item remains independently editable in Dun. There is no automatic
completion or cancellation back to the sending app.

Authentication failures return 401, browser Origin requests 403, validation failures 422,
and storage errors 500, with `{ "error": "..." }`. Malformed JSON/oversized bodies use
the HTTP framework's rejection response. Sending apps should retry network/5xx errors
with the **same** external ID, and correct 4xx errors before retrying.

## Local services and key rotation

Dun writes a private connection descriptor next to `dun.sqlite3`:
`%APPDATA%\app.dun\integration-connection.json`. It contains `enabled`, `url`, and `token`.
Keep this file private; it is not part of synced state or exported backups.
`scripts/integrations/dun-client.mjs` reads it on every send, so rotation is automatic.
Services running as another Windows account need its explicit absolute path through
`DUN_CONNECTION_FILE`. That service account must be able to read it.
For scripted setup, launching Dun with `--apps-on` enables connections persistently.

## Memos

The existing `selfhost\memos-remind` service continues parsing its usual `#remind/...`
tags. The adapter sends each **due occurrence** to Dun, where it nags until done;
upcoming tags remain scheduled by the Memos service. Sharing and reminder confirmations
still use its existing notifier. Other Memos users keep their existing delivery.

Memos versions that flatten extra tag separators are supported too: for example,
`#remind/2026-10-01/0900` may be saved as `#remind/2026-10-01-0900`. The adapter
recognizes both forms, including recurring and relative tags, while keeping the
stored occurrence key unchanged for duplicate protection.

Install the adapter for the original `memos-token.txt` account:

```powershell
node scripts/integrations/install-memos.mjs C:\Users\pcuser\selfhost\memos-remind
```

This backs up the script and config as `*.before-dun`, copies the client, sets only the
original account's `dun: true`, and stores the descriptor path. The service already watches
config and restarts itself. If Dun is unavailable, the occurrence stays undelivered and
retries in a minute; it is marked fired only after Dun accepts it. Existing fired occurrences
are not replayed. Dry runs never send to Dun. To undo routing, set that user's `dun` to false.

The local adapter connects to a colocated Dun desktop. A copy of the service on a VPS
cannot reach the PC's loopback address; remote delivery requires a separately designed
authenticated relay, not opening this HTTP listener to the internet.

## Arbor

The sibling Arbor project has **item menu → Remind me in Dun…**, with a date/time picker
and 15-minute, 1-hour and tomorrow shortcuts. Its authenticated server reads the item title
and notes from storage, then forwards to Dun. The browser never sees the connection key.
Duplicate clicks for the same item/time are safe. Open the source link in Dun to return to
that item in Arbor. Completing or archiving the Arbor item does not cancel the Dun reminder.

The server uses `DUN_CONNECTION_FILE` (or the current account's default descriptor).
An Arbor service can also read the descriptor path from `data/dun-connection-path.txt`.
An offline request shows an error and leaves the form available to retry; it does not claim
success or silently create an offline queue.
