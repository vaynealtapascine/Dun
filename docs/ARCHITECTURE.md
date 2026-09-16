# Architecture

Dun is one Svelte UI and one Rust core, wrapped twice: a Tauri desktop app on
Windows and a Tauri Android app that also runs from broadcast receivers with no
Activity in sight. The PC is the hub; the phone is a peer that syncs with it
directly over the LAN.

```
 Svelte 5 (src/)                      one UI, two shells
   main window · quick-add bar          window label picks the root view
        │ invoke / events
 ┌──────┴───────────────────────────────────────────────┐
 │ src-tauri/                                           │
 │   desktop/   scheduler loop, toasts + COM activator, │
 │              attended detection, audio, tray, hotkey,│
 │              firewall helper, sync server            │
 │   mobile/    JNI bridge, plan builder, check-in      │
 └──────┬───────────────────────────────────────────────┘
        │
 ┌──────┴──────────┐   ┌──────────────────┐
 │ crates/dun-core │   │ crates/dun-sync  │
 │ model, recurrence│   │ cert, pinning,  │
 │ scheduler, HLC,  │   │ pairing, HTTPS  │
 │ merge, SQLite    │   │ server + client │
 └──────────────────┘   └──────────────────┘
```

## The core is pure, the shells are not

`dun-core` knows nothing about Tauri, Windows or Android. It holds the model,
recurrence maths, the SQLite store, CRDT-style merge, and a scheduler that is a
single function:

```rust
scheduler.evaluate(state, now, tz, role) -> Vec<Effect>
```

`Effect` is `ShowAlert`, `ShowSummary`, `ClearAlert`, `PlayChime`,
`ScheduleWake`, `StateChanged` or `TrayStatus`. Nothing rings inside the core —
the shells carry effects out. That is what makes ring cadence, missed-ring
collapsing, quiet hours, mute and handoff testable with a fake clock and no
platform at all (`crates/dun-core/src/scheduler/tests.rs`).

`Role` decides who is expected to ring: `Solo`, `Hub` (a PC with a paired
phone) or `Phone`.

## Storage: registers, not rows

Everything synced lives in one table of per-field registers:

```
reg(entity, id, field, value JSON, hlc, device, seq)
```

`entity` is `item`, `tag`, `preset` or `setting`. The projection in
`state.rs` folds registers into the structs the UI sees; `history` is a
separate append-only log that is unioned on sync.

- **HLC** (`hlc.rs`): `(physical_ms << 16) | counter`, ties broken by device id,
  so every write is ordered without a coordinator.
- **`seq`** is the local change cursor, and it is bumped **only when a merge
  actually changes a row**. Without that rule two devices echo each other's rows
  forever.
- **Merge** is last-writer-wins per field, with three deliberate exceptions:
  completion merges by max `(gen, through)` so a stale Done can never beat an
  Undo; a snooze applies only if it names the current occurrence; `created`
  keeps the first writer. See [SYNC-PROTOCOL.md](SYNC-PROTOCOL.md).

Device-local state (ring states, peers, chime, hotkey, window geometry) lives in
`local_setting` and never syncs.

A property test (`tests/convergence_proptest.rs`) runs random operations on two
replicas in random orders and checks both against an independent oracle, so a
merge that "converges" on something wrong still fails.

## Desktop

`desktop/scheduler_loop.rs` is a thread with no fixed tick: it blocks on a
channel with a timeout of "however long until the next deadline, at most a
second", and mutations, merges and toast clicks wake it immediately. A
wall-clock jump over a few seconds means the machine slept, which feeds the
missed-ring path.

Each wake also polls whether someone is there (`attended.rs`: session lock state
from WTS plus `GetLastInputInfo`), which is what the Hub role turns into a
decision to ring or to let the phone do it.

Windows notifications (`toast/`, `toasts.rs`) are raw WinRT: an HKCU
`AppUserModelId`, a `CLSID\LocalServer32` entry and a registered COM activator,
so a button press works from Action Center even after Dun has exited — Windows
relaunches `dun.exe -ToastActivated` and the click still lands.

## Android

One shared library, `dun_lib`, exports a single JNI entry point:

```kotlin
DunNative.handleEvent(dataDir, tzId, nowMs, eventJson): String
```

Kotlin makes no scheduling decisions. It sends an event (`alarm`, `action`,
`dismissed`, `reschedule`, `appStarted`, `syncOnly`) and gets back a **plan**:
notifications to post, ids to cancel, the single `next_wake_at` to set with
`setAlarmClock`, and whether a push is still pending. `mobile/plan.rs` is that
contract; `PlanExecutor.kt` carries it out.

Two rules hold the design together:

- **Only one copy of the library and one copy of SQLite in the process.** The
  JNI exports live in the `src-tauri` lib crate itself, because a linker may
  drop exports that come from a dependency rlib.
- **Never hold the core mutex across network I/O.** `mobile/bridge.rs` applies
  the event, releases the lock, checks in with the PC under a hard deadline,
  re-locks, merges, then evaluates.

Alerts always come from AlarmManager, never from a Rust thread, so the app in
the foreground and a receiver in the background cannot both ring.

Two smaller paths keep the phone honest between alarms: the app syncs when it
comes to the foreground and every half minute while it stays there, and a
WorkManager job carries a change the PC hasn't acknowledged — the case where
nothing is due, so no alarm is coming to retry it.

## Sync

`dun-sync` is transport only: a self-signed certificate (`cert.rs`),
SHA-256 certificate pinning (`pin.rs`), pairing codes and tokens
(`pairing.rs`), an axum HTTPS server (`server.rs`, desktop only) and a client
that races the PC's known addresses (`client.rs`).

What a sync *means* lives in `dun-core/src/sync/session.rs`, so both sides are
tested against two real engines with no sockets at all
(`crates/dun-sync/tests/two_engines.rs`).

The server owns its own Tokio runtime; it is started from Tauri's setup thread,
where no runtime is running.

## Time

One-offs and timers are absolute instants. Recurring rules are civil time with
an optional IANA zone (`null` means "wherever the device is"), evaluated with
jiff: a DST gap shifts forward, a fold takes the earlier instant, and month-day
values clamp to the end of short months.
