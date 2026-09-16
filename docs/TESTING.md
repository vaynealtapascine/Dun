# Testing

## The script

```powershell
pwsh scripts/verify.ps1 -Quick      # everything except bundling (~90 s)
pwsh scripts/verify.ps1             # plus the NSIS installer
pwsh scripts/verify.ps1 -Android    # plus the APK, JNI export and 16 KB alignment
```

It stops at the first failure and runs, in order: `svelte-check
--fail-on-warnings`, `vitest run`, `cargo fmt --check`, `cargo clippy
--workspace --all-targets -D warnings`, `cargo test --workspace`, then the
bundles.

## What the automated tests cover

| Area | Where | What it proves |
|---|---|---|
| Recurrence, DST, month-end | `dun-core/src/recurrence.rs` | Gaps shift forward, folds take the earlier instant, day 31 clamps |
| Occurrences | `dun-core/src/occurrence.rs` | Which slot is ringing, collapsed misses, after-completion counting |
| Scheduler | `dun-core/src/scheduler/tests.rs` | Nag cadence, ring-once, snooze, missed labels, summary threshold, quiet hours, mute, handoff and fail-loud timings — all on a fake clock |
| Merge | `dun-core/tests/convergence_proptest.rs` | Two replicas, random ops in random orders, checked against an independent oracle |
| Storage | `dun-core/src/store/` | Paging by `seq`, `seq` moving only on real change, history and backup round-trips |
| A sync exchange | `dun-sync/tests/two_engines.rs` | Two real engines: changes both ways, echoes as no-ops, a phone covering a ring, Done on the phone stopping the PC |
| TLS and pairing | `dun-sync/tests/localhost.rs` | Real handshake on localhost: wrong pin, bad token, single-use and expired codes, a dead address falling through, and starting the server with no runtime in the caller |
| Quick-add parsing | `src/lib/*.test.ts` | Table-driven phrases → drafts, durations, tags, nag overrides |

The parser is checked against the engine at runtime too: the quick-add preview
asks Rust to validate the draft and hand back the next occurrences, so the TS
parser and the engine can't quietly disagree.

## Dev aids

Debug builds only; all of them do nothing in a release build.

| Flag / variable | What it does |
|---|---|
| `--dev-seed`, `DUN_DEV_SEED=1` | Adds a few items that ring within a minute |
| `--sync-on` | Turns sync on without the firewall prompt, and prints the pairing invite to stderr |
| `DUN_CLOCK_OFFSET_MS` | Shifts "now" — for skew warnings and time-jump handling |
| `--quick-add` | Opens the quick-add bar (what the shortcut passes) |
| `--hidden` | Starts in the tray, as autostart does |
| `--setup-firewall <port>` | The elevated helper run; adds the rule and exits |

`npm run dev` alone (no Tauri) still renders the whole UI: `src/lib/dev/mock.ts`
answers every command with sample data, which is the fastest way to work on
views.

## Manual: Windows

- **Ringing** — `--dev-seed`, wait a minute. Expect a toast with Done · +1m ·
  +5m · +15m, a chime, and the tray icon showing a badge.
- **Replace and remove** — the toast is replaced on each nag, not stacked, and
  disappears when the item is marked done in the app.
- **Toasts after Quit** — quit Dun, press Done in Action Center. Dun starts
  hidden and applies it.
- **Sleep/resume** — expect "missed at …" labels, and a single summary toast
  when more than three are newly missed.
- **Lock** — `rundll32 user32.dll,LockWorkStation` with a short idle threshold:
  silent toast first, loud two minutes later if no phone is covering it, and
  everything rings at once on unlock.
- **Firewall** — `Get-NetFirewallRule -DisplayName "Dun*"` shows one allow rule
  and no leftover block rules for the exe.
- **Network profile** — `Get-NetConnectionProfile`. On a Public network Windows
  blocks the port whatever the rule says; Settings should say so by name.
- **Autostart** — `reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Run"`,
  then reboot and check Dun is in the tray and not on screen.

## Manual: Android

```powershell
. .\scripts\dev-env.ps1
npx tauri android build --debug --apk --target aarch64
adb install -r <apk>
adb logcat -s DunNative DunReceiver DunPlugin PlanExecutor
```

- **Doze** — `adb shell dumpsys battery unplug` then
  `adb shell dumpsys deviceidle force-idle`, and wait for a nag.
- **Alarms and notifications** — `adb shell dumpsys alarm | findstr app.dun`,
  `adb shell dumpsys notification --noredact`.
- **Process death** — swipe out of recents, then `adb shell am kill app.dun`; a
  due alarm must still fire and its buttons must still work.
- **Reboot** — `adb reboot`, unlock, and check alarms are re-registered and
  missed items ring. (Nothing fires before the first unlock: storage is
  credential-encrypted.)
- **A push with nothing due** — the case the alarm path can't cover. Stop Dun
  on the PC, add an item on the phone dated days away, background the app and
  `adb shell am kill app.dun`. `dumpsys alarm` should show no alarm before that
  date, so only the pending-push job is left to deliver it. Start the PC again
  and leave the phone alone: the item must reach the PC anyway.
- **Standby** — `adb shell am set-standby-bucket app.dun rare`.
- **Handoff** — PC attended → phone silent; PC locked → phone rings; phone in
  airplane mode → PC loud after two minutes; Done on the phone → the PC stops
  within seconds; Done on the PC → the phone clears by its next nag.

### Testing sync without the LAN

Windows blocks inbound connections on a network it has marked Public, which
most home networks are until someone changes it. To test the phone against the
PC without touching the firewall at all, send the traffic over USB:

```powershell
adb reverse tcp:47823 tcp:47823
```

Start the PC with `--sync-on`, take the `dun://pair?…` line it prints, replace
the `a=` addresses with `127.0.0.1`, and paste that into the phone's pairing
screen. The phone then reaches the PC through loopback, which the firewall
never sees. `adb reverse` is dropped when the cable is unplugged, so re-run it
after reconnecting.

## Known environment traps

Things that cost an hour once and shouldn't cost another:

- **WebView2 exposes no accessibility tree** unless a client asks for it, so
  UI automation that looks for buttons by name finds only the window frame.
  Prefer driving the app through its own commands, or a dev flag, over poking
  the UI.
- **Native file dialogs** (backup export/import, sound import) block automation
  outright; those flows need a person.
- **`setAlarmClock` is the only alarm that nags every minute.**
  `set*AndAllowWhileIdle` is capped at roughly once every nine minutes, so it
  cannot be used for a nag cadence.
- **Force-stopping the app on Android cancels its alarms.** Nothing rings again
  until Dun is opened once. That is Android's rule, not a bug to chase.
