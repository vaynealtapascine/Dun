# Sync protocol

No cloud, no server on the internet. Every PC listens on the LAN; a phone
connects to one of them, and PCs paired with each other connect both ways.
Everything is HTTPS/1.1 on **port 47823** with a self-signed certificate the
client pins by fingerprint.

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

A PC joining another PC (**Join another PC…**, pasting the invite) adds a way
back to the same request:

```
→ {"code":"904018","deviceId":"…","name":"LAPTOP-7Q2",
   "back":{"token":"<64 hex>","fingerprint":"<sha256>","addrs":["192.168.1.11"],"port":47823}}
```

Each PC then keeps both halves: the hash of the token it issued, and the
token, fingerprint and addresses it needs to sync with the other. A `back`
without a 64-character fingerprint is dropped, so the pairing degrades to one
way rather than trusting an unpinned certificate.

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

**Cursors.** Each side keeps `pulled_upto` (how far it has stored the peer's
rows) and `pushed_upto` (how far the peer has acknowledged its own rows). The
server moves them on too (`Peer::note_served`) with the same meaning, so two
PCs that sync each other in both directions share one pair of cursors. A page is
**1000 rows**; while `pull.more` is set the foreground app syncs again straight
away, and a receiver takes one page and leaves the rest to the next check-in.

**Echo suppression.** A page leaves out rows whose `device` is the peer's own
id. They would be no-ops after merge, but they double the traffic of every
sync, and a row that comes back is written again locally only if it changes
something (`seq` moves only on real change) — so an echo cannot start a
ping-pong either way.

**Echoes in a mesh.** Rows written by a third device are not left out, which
is how they reach PCs that aren't paired with their author. When one comes
back round a cycle of PCs it changes nothing, `seq` stays put, and it goes no
further.

**Drift guard.** A push stamped more than **24 hours** from the PC's clock is
refused with `clockDrift` rather than merged into the future. Between 30 s and
24 h the response carries `skewWarning` and the UI says so.

**Updates.** `GET /v1/apk`, same bearer token, serves the Android package
the PC is holding in its `updates` folder. The offer only appears in a
response when the requesting phone's `appVer` is older, compared number by
number so 0.10.0 beats 0.9.0. The phone checks the SHA-256 before handing the
file to Android's installer, and nothing downloads or installs without the
user asking.

## Between PCs

A PC's request also says whether someone is at it, which is what marks it as a
PC, and carries what it knows of the mesh; the response answers in kind. A
phone's requests and responses never carry these fields.

```jsonc
// request, besides the fields above
"attended": false,
"presence": [{"deviceId":"…","attendedAt":1789500000000}],
"dueItems": [{"item":"…","occurrence":…,"at":1789499990000}]

// response, besides the fields above
"presence": [...],
"acks": [{"item":"…","occurrence":…,"at":…}]
```

- **Presence** is the last moment each PC itself saw someone at it. A PC lists
  itself while attended, plus every other PC it heard about in the last
  **35 s**, and never the PC it is talking to. Relayed entries keep their
  original time, so a PC that was left drops out everywhere once the 35 s pass,
  rather than being kept alive by PCs echoing it to each other.
- **Acks** are the phone's handoff acknowledgements, passed on with the time
  the first PC heard them (`at`). For each item the latest occurrence, then
  the latest ack of it, wins.
- **`attended`** in any response, to a phone too, means someone is at this PC
  or at another PC heard from in the last 35 s.

PCs sync with each other straight away after a local change or when someone
arrives or leaves, and every **10 s** otherwise; one that doesn't answer is
left for 30 s.

Phones stay clients. A phone paired with several PCs checks in with the one
that answered most recently, and after a failure tries a different one next
time.

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

Countdowns are registers on the `setting` entity with the id
`countdown:<id>` (fields `title`, `notes`, `date`, `time`, `repeat`,
`direction`, `display`, `pinned`, `deleted`), all last writer wins. They don't
get an entity of their own because builds before 1.4 reject a sync carrying an
entity they don't know, but store and pass on setting ids other than `global`.
So a mixed-version mesh keeps syncing, and the older device relays countdowns
it can't show. `display` is one register holding the whole look, so a restyle
on two devices settles on one of them rather than a mix.

## Handoff

Only a PC someone is at rings. An unattended PC stays quiet while another PC
has reported someone at it within the last 35 s, however long that goes on.

Before every alert it is about to raise **or suppress**, the phone lists that
occurrence in `dueItems`. The PC records an acknowledgement per item and
passes it on to the other PCs.

An ack **covers** a ring when all of these hold:

- it names the current occurrence,
- it arrived no earlier than 30 s before the item was due (`ACK_SKEW_MS`),
- and, for a nagging item, `now <= ack.at + nag interval + 120 s`
  (`phone_grace_s`).

With that, an unattended PC shows one silent toast and waits. Without it, the
PC stays quiet until **120 s** after the item was due, or after the last
attended PC was left if that is later (`fail_loud_after_s`), and then rings
properly — Dun would rather be annoying than silent. Acks resuming
put it back to deferred, and the moment someone touches the PC every deferred
item rings at once.

The phone's side of the same rule: stay quiet only while the PC says
`attended` (someone is at it or at another PC) **and** lists that occurrence
in `ringingSoon` (which looks 5 s
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

## Reaching a PC

The phone tries its `last_ok_addr` first, then the rest of the PC's addresses
staggered 300 ms apart, with a 1.5 s connect timeout and 3.5 s for the whole
attempt (4 s hard budget in the receiver, well inside Android's 10 s). Every
response refreshes the address list, so a PC that moves between Wi-Fi, Ethernet
and Tailscale is followed automatically. A PC syncing with another PC reaches
it the same way.

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
