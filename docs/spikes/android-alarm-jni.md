# Spike M1: Android alarm → receiver → JNI → SQLite → notification

**Question:** can a BroadcastReceiver, with the app's UI process dead, load `dun_lib`, run Rust against SQLite and post a nagging notification with working Done / +5m / +15m buttons, every minute, through Doze and reboots?

**Status: code complete, device matrix NOT run yet.** No phone or emulator was connected when this was written. Nothing below the "Build evidence" section has been observed on a device.

Code:
- Kotlin: `plugins/tauri-plugin-dun-android/android/...` (`DunNative`, `DunReceiver`, `SystemEventReceiver`, `PlanExecutor`, `Channels`, `AlarmLog`, `DunPlugin`)
- Rust: `src-tauri/src/mobile/{plan,bridge}.rs`, `src-tauri/src/android_jni.rs`
- Spike UI: `src/views/AlarmSpike.svelte`

## Build evidence (2026-09-16)
- `tauri android build --debug --apk --target aarch64` succeeds with NDK r28c; the Kotlin plugin compiles.
- `llvm-nm -D libdun_lib.so` lists `T Java_app_dun_android_DunNative_handleEvent` next to wry's `Java_app_dun_Rust_*`.
- `llvm-objdump -p`: LOAD segments `align 2**14` (16 KB). `zipalign -c -P 16 4` passes on the APK.
- The merged manifest (`aapt2 dump xmltree`) contains:
  - Permissions: POST_NOTIFICATIONS, USE_EXACT_ALARM, SCHEDULE_EXACT_ALARM (maxSdk 32), RECEIVE_BOOT_COMPLETED, REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, ACCESS_NETWORK_STATE, VIBRATE.
  - Both receivers.
- Rust unit tests (desktop) for the contract and spike logic cover:
  - nag every minute until Done
  - swipe keeps nagging
  - snooze pushes the alarm out
  - a stale button for an old occurrence is ignored
  - an early alarm re-arms
  - a reboot after the due time rings as "Missed at"
  - bad JSON returns `ok=false` instead of panicking

## Device matrix to run (Samsung, Wi-Fi adb so charging doesn't block Doze)
Install with `adb install -r app-universal-debug.apk`, open Dun, allow notifications, then **Ring in 30 s**. Watch with `adb logcat -s DunReceiver DunNative DunPlan DunSystemEvent`.

| # | Scenario | How | Pass when |
|---|---|---|---|
| 1 | Warm ring | App open | Notification at +30 s with 3 buttons; logcat shows handler time |
| 2 | Process dead | Swipe Dun from recents, or `adb shell am kill app.dun` | Ring still arrives; logcat shows `lib load N ms` (cold) and total under 1.5 s |
| 3 | Nag cadence | Ignore it | Re-rings every ~60 s ("ring #2", "#3"...) |
| 4 | Swipe away | Swipe the notification | Next nag still arrives |
| 5 | Buttons, process dead | `am kill`, then press +5m | Notification clears; alarm moves ~5 min out (`dumpsys alarm \| findstr app.dun`) |
| 6 | Done | Press Done | Notification clears; no alarm pending |
| 7 | Doze | `dumpsys battery unplug`; `dumpsys deviceidle force-idle`; wait | Nags keep arriving about every minute (setAlarmClock) |
| 8 | Standby bucket | `am set-standby-bucket app.dun rare` | Nags keep arriving |
| 9 | Reboot | Arm a 2-min ring, `adb reboot`, unlock after it is due | "Missed at ..." ring right after unlock; nagging continues |
| 10 | Manual reschedule | `adb shell am broadcast -a app.dun.RESCHEDULE -n app.dun/app.dun.android.SystemEventReceiver` | Log line `rescheduled after manual` |
| 11 | Failure path | Temporarily break the event JSON in `DunReceiver` | "Dun: check your reminders" notification, retry alarm at +60 s |

**Go / no-go:**
- **Go** (keep the JNI-from-receiver design): 1–10 pass, and the cold handler time is under 1.5 s.
- **No-go:** switch to the fallback in the plan (Rust precomputes a 24 h plan; Kotlin executes and journals actions).

## Deferred to M9/M10
The pinned-HTTPS check-in from inside `goAsync` needs `dun-sync`, which doesn't exist yet. The Android 15 "network outside a valid process lifecycle" rule should allow it (a receiver's process counts as foreground), but that is unverified until then.
