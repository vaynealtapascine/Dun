# Sync protocol

Two devices, no cloud, no server on the internet. The PC listens on the LAN;
the phone connects to it. Everything is HTTPS/1.1 on **port 47823** with a
self-signed certificate the phone pins by fingerprint.

Wire types live in `crates/dun-core/src/sync/protocol.rs`; what an exchange
*means* is `sync/session.rs`; TLS, pairing and addresses are `crates/dun-sync`.

## Pairing

`POST /v1/pair` is open **only while the Pairing dialog is on screen**: five
minutes, five tries, one success.

```
→ {"code":"904018","deviceId":"…","name":"Galaxy A56"}
← {"token":"<64 hex>","pcDeviceId":"…","pcName":"DESKTOP-ABC1234"}
```

The PC stores `sha256(token)` and compares in constant time, so a stolen
database can't be replayed. The phone stores the token and the PC's certificate
fingerprint.

The dialog also shows the invite as a QR code and as text:

```
dun://pair?v=1&id=<pc device id>&n=<pc name>&p=47823&a=<ip,ip>&fp=<sha256 of cert DER>&c=<code>
```

`PairingInvite::parse` refuses anything without a 64-character fingerprint or
with no addresses, so a truncated paste fails immediately instead of half
working.

## Syncing

`POST /v1/sync`, `Authorization: Bearer <token>`. One request carries
everything: what the phone changed, where it got to, what it is about to ring.

```jsonc
// request
{
  "deviceId": "…",
  "now": 1789500000000,          // the phone's clock
  "pulledUpto": 42,              // highest PC seq the phone has stored
  "push": { "registers": [...], "history": [...], "maxSeq": 7 },
  "dueItems": [{"item":"…","occurrence":1789500060000}],
  "appVer": "0.1.0",
  "schemaVer": 1
}

// response
{
  "pcNow": 1789500000100,
  "ackedUpto": 7,                // the phone's maxSeq, now stored
  "pull": { "registers": [...], "history": [...], "newPulledUpto": 91, "more": false },
  "attended": true,              // someone is at the PC
  "ringingSoon": [{"item":"…","occurrence":…}],
  "addrs": ["192.168.1.10","100.64.0.2"],
  "skewWarning": null,           // ms, when the clocks disagree by over 30 s
  "update": null                 // {version, size, sha256} when the PC holds a newer build
}
```

The PC does all of it in one transaction: drift guard, merge, record the
handoff acknowledgements, evaluate at `pcNow`, answer.

**Cursors.** Each side keeps `pulled_upto` (how far it has read the peer) and
`pushed_upto` (how far the peer has acknowledged its own rows). A page is
**1000 rows**; while `pull.more` is set the foreground app syncs again straight
away, and a receiver takes one page and leaves the rest to the next check-in.

**Echo suppression.** A page leaves out rows whose `device` is the peer's own
id. They would be no-ops after merge, but they double the traffic of every
sync, and a row that comes back is written again locally only if it changes
something (`seq` moves only on real change) — so an echo cannot start a
ping-pong either way.

**Drift guard.** A push stamped more than **24 hours** from the PC's clock is
refused with `clockDrift` rather than merged into the future. Between 30 s and
24 h the response carries `skewWarning` and the UI says so.

**Updates.** `GET /v1/apk`, same bearer token, serves the Android package
the PC is holding in its `updates` folder. The offer only appears in a
response when the requesting phone's `appVer` is older, compared number by
number so 0.10.0 beats 0.9.0. The phone checks the SHA-256 before handing the
file to Android's installer, and nothing downloads or installs without the
user asking.

## Merge rules

Default: the higher `(hlc, device)` wins, per field. The exceptions exist
because "newest wins" is the wrong answer for them:

| Field | Rule | Why |
|---|---|---|
| `item.completion {through, doneAt, gen}` | max `(gen, through)` | Concurrent Done is idempotent; Undo writes `gen+1` so a stale Done loses. Re-ringing is the safe direction. |
| `item.snooze {occurrence, until, count}` | applied only if it names the current occurrence | Done always beats a stale snooze, with no timestamp race. |
| `item.schedule.effectiveFrom` | set when the rule is edited | An edit never produces a burst of phantom missed rings. |
| `item.created` | first writer wins | Two devices creating the same id would otherwise flip-flop. |
| `deleted` | last writer wins, tombstones kept forever | A resurrected item is worse than a kept tombstone. |

History rows are append-only and unioned by id.

## Handoff

Before every alert it is about to raise **or suppress**, the phone lists that
occurrence in `dueItems`. The PC records an acknowledgement per item.

An ack **covers** a ring when all of these hold:

- it names the current occurrence,
- it arrived no earlier than 30 s before the item was due (`ACK_SKEW_MS`),
- and, for a nagging item, `now <= ack.at + nag interval + 120 s`
  (`phone_grace_s`).

With that, an unattended PC shows one silent toast and waits. Without it, the
PC stays quiet until **120 s** after the item was due (`fail_loud_after_s`) and
then rings properly — Dun would rather be annoying than silent. Acks resuming
put it back to deferred, and the moment someone touches the PC every deferred
item rings at once.

The phone's side of the same rule: stay quiet only while the PC says
`attended` **and** lists that occurrence in `ringingSoon` (which looks 5 s
ahead). Anything else — including not reaching the PC at all — means the phone
rings.

## Refusals

`error` is stable and machine-readable; `message` is one sentence for a person.

| `error` | Status | Meaning |
|---|---|---|
| `badCode` | 403 | Wrong, expired, or already-used pairing code |
| `notPairing` | 403 | The Pairing dialog isn't open |
| `tooManyTries` | 403 | Five wrong codes; reopen the dialog |
| `unauthorized` | 401 | Missing or unknown bearer token |
| `clockDrift` | 400 | Pushed rows more than a day from the PC's clock |
| `badRequest` | 400 | Malformed request |

## Reaching the PC

The phone tries its `last_ok_addr` first, then the rest of the PC's addresses
staggered 300 ms apart, with a 1.5 s connect timeout and 3.5 s for the whole
attempt (4 s hard budget in the receiver, well inside Android's 10 s). Every
response refreshes the address list, so a PC that moves between Wi-Fi, Ethernet
and Tailscale is followed automatically.

When a check-in fails with unpushed changes, a WorkManager job retries until it
gets through.

## Security notes

- The certificate is pinned by SHA-256 of its DER. A different certificate
  fails the handshake, so the PC never even sees the request.
- Tokens are random 256-bit values, stored hashed on the PC.
- The firewall rule allows the port on **private and domain** networks only,
  from `localsubnet` and the Tailscale range `100.64.0.0/10`. On a network
  Windows has marked Public, Dun says so in Settings rather than opening the
  port there.
