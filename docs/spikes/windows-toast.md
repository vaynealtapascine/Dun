# Spike M2: Windows toast buttons via a COM activator

**Question:** can an unpackaged Tauri EXE show reminder toasts whose Done / +1m / +5m / +15m buttons reach Dun while it runs, after it quits, and from Action Center?

**Answer:** yes. One behaviour changes the scheduler design, and a dev-environment trap makes naive testing give false failures.

Code: `src-tauri/src/desktop/toast/` (`registration.rs`, `activator.rs`, `xml.rs`, `mod.rs`), driven by the temporary `desktop/toast_spike.rs` and `src/views/ToastSpike.svelte`.

## Design as built
- **Registration** is rewritten on every start:
  - `HKCU\Software\Classes\AppUserModelId\<aumid>`: DisplayName, IconUri, IconBackgroundColor, CustomActivator.
  - `HKCU\Software\Classes\CLSID\{clsid}\LocalServer32` = `"<exe>" -ToastActivated`.
  - Dev and release builds use different AUMIDs (`app.dun.dev` / `app.dun`) and CLSIDs.
  - Spotify, Flow Launcher and Todoist on this machine register the same way.
- **Activator:** a `#[implement(IClassFactory)]` factory creates `#[implement(INotificationActivationCallback)]` objects. It is registered with `CoRegisterClassObject(CLSCTX_LOCAL_SERVER, REGCLS_MULTIPLEUSE)` on a parked MTA thread, before the Tauri builder runs.
- **Toast XML:**
  - `scenario="reminder"`, silent audio (Dun plays its own chime), up to 3 text lines.
  - Background-activation buttons with arguments `a=done;i=<item>;o=<occurrence>` / `a=snz;...;m=<minutes>`.
  - Arguments are percent-escaped, and a malformed string is an error, never a guess.

## Evidence (2026-09-16, Windows 11 Pro 26200, debug build)

| Check | Result |
|---|---|
| Toast delivered | `ToastNotificationManager.History.GetHistory('app.dun.dev')` held the toast with tag `spike`, group `ring`. `notifier.Setting()` = Enabled |
| Banner rendered | Seen on screen: "Dun (dev)" header and icon, title, two lines, buttons **Done · +1m · +5m · +15m**. It stays up (reminder scenario) |
| Warm activation (app running) | `CoCreateInstance` + `Activate("a=snz;i=spike;o=1000;m=5")` reached the running process (same pid) in **13 ms**. Parsed `Snooze { item: "spike", occ: 1000, minutes: 5 }` |
| Cold activation (app not running) | COM launched `dun.exe -ToastActivated -Embedding`. The log shows `start ... launched_by_toast=true`, then `activated ... Done { item: "spike", occ: 1000 }` **15 ms** after process start (729 ms including launch). A second activation went to the same process in 2 ms |
| Same tag, banner still on screen | Content updated in place ("replace · show #2"); history still holds 1 entry |
| Same tag **after the banner was dismissed** | History updated to "replace · show #3", but **no banner appeared** |
| Remove then show | A fresh banner popped ("ring · show #4") |

The activation harness is `scripts/dev/toast-activate.ps1`. It calls the activator exactly as the shell does: `CoCreateInstance` on the CLSID, then `INotificationActivationCallback::Activate`.

## Consequences for M6
1. **A Full nag must remove, then show.** Replacing a toast the user already dismissed updates Action Center silently, so every nag after the first would be invisible. Use `History.RemoveGroupedTagWithId` then `Show`. Keep in-place replace (optionally `SuppressPopup`) for Silent updates, such as a deferred or muted item's label changing.
2. `-ToastActivated` arrives together with `-Embedding` (added by COM). Match it anywhere in argv, as `lib.rs` does.
3. Activations can arrive long after the toast was shown, so the occurrence in the arguments is load-bearing (see plan §4 "stale Action Center click").

## Dev-environment trap
Processes started from the Claude desktop app, the terminal where this was developed, inherit that packaged app's **AppData and HKCU virtualization**. A dev run's registration lands in the package's private hive and `LocalCache\Roaming`. The real shell and RPCSS don't see it, and cold activation fails with `REGDB_E_CLASSNOTREG`.
- **Fix for testing:** launch through Explorer (`explorer.exe path\to\dun.exe`) so Dun registers in the real hive.
- **Production:** unaffected. The NSIS install is started by Explorer or the Run key.

## Not verified yet
- **A physical mouse click** on a banner button. The COM path it uses is verified above; the remaining step is the shell's own click handling. Check it by hand when M6 lands: show a toast, press +5m, and confirm the item snoozes.
- **Behaviour while the session is locked, and under Windows 11 Do Not Disturb** (reminder-scenario toasts should break through when "Show reminders" is on). Belongs to the M11 matrix.
- **Release-build AUMID `app.dun`** once installed with NSIS.

## Side observation
Launching the Explorer-started process triggered a **MacType** "incompatible with this application, please update to 2023.5.31 or later" dialog: MacType injects into Explorer children and WebView2 rejects old versions. This is not Dun's bug, but users with an old MacType will see it with any WebView2 app.
