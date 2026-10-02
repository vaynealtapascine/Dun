# Android device checks

The first device batch was run on 2026-10-01. Results below distinguish the checks
performed from the remaining matrix. Use this checklist for subsequent sessions.

1. Record model, Android version and font/display scaling; install the new debug APK
   without clearing app data. Keep existing pairing and reminders.
2. Check launcher, recent-app and notification branding: Dun ring/check artwork throughout,
   with no Tauri logo. Also check the system's next-alarm indicator.
3. Start two timers (one with a long title). Inspect the collapsed and expanded countdown
   notifications in light and dark mode, portrait and landscape, and larger system font.
   Countdown should be legible, title unclipped, and each timer separately identifiable.
4. Pause a timer from its notification; confirm the countdown is removed and Dun shows the
   remaining time. Resume in Dun, then Reset from its new notification. Neither control
   should change the other timer. Replay a stale button after restarting; it must do nothing.
5. Let a short timer expire with the screen locked. The countdown must be replaced by an
   audible alert with Done/+5m/+15m. Check silent/vibrate and DND behavior, notification
   permission denied, exact-alarm access, mute and quiet hours. Check the existing alarm
   foreground service stops after Done and doesn't keep a stale notification.
6. Close the app, lock the phone, test Doze, then reboot with a running timer. Confirm
   deadline restoration and alarms. Notification ticking must not require per-second wakes.
7. Send one Memos occurrence and one Arbor reminder to the desktop API; sync to the phone,
   snooze/finish there, and verify desktop state. Retrying delivery must not duplicate or
   revive either item. Confirm only the original Memos account routes to Dun.
8. Capture notification screenshots, relevant `dumpsys notification`/`alarm` output and
   focused Dun receiver/service logs in this one session. Record actual device findings
   here before claiming Android visual or delivery behavior verified.

## Session: 2026-10-01

Device: Samsung Galaxy A56 (SM-A566B), Android 16 / API 36, 1080 × 2340,
density 450, original font scale 1.0. Updated with `adb install -r`; the original
2026-09-16 install date, database, notification permission and PC pairing survived.
A private database copy and test evidence are in `F:\DunBuild\phone-checks-20261001`.

| Check | Observed result |
|---|---|
| Notification branding | Correct Dun ring/check icon in both collapsed and expanded Samsung notifications; no Tauri icon. Launcher resources were checked in the APK; launcher/recent-app artwork was not separately inspected on screen. |
| Countdown layout | Two API-created timers stayed individually identifiable. Large ticking countdowns, wrapped/ellipsized long titles and controls were legible in dark and light modes, font scale 1.3, portrait and landscape. |
| Notification controls | Actual Pause tap removed that countdown and preserved 16:36 in Dun. Resume continued it. Actual Reset tap returned it to idle on phone and PC; the other timer kept running. Stale-action rejection remains covered by Rust tests. |
| Locked expiry | A 45-second API timer expired with the screen locked in the phone's existing vibrate mode. Receiver and `DunTimerSound` logs confirm alarm-stream playback started. The short playback service exited; no stale foreground service remained after completion. |
| Snooze and completion | Native commands through the installed debug build verified +5-minute snooze, Done, notification cancellation and propagation back to the PC. Done/+5m/+15m action presence was verified in notification data; those three notification buttons were not all tapped on screen. |
| Restart | Boot receiver loaded the native library and rearmed the saved 4-minute timer after first unlock. It expired and started playback at its original deadline. Existing pairing and sync recovered. Subsequent foreground sync correctly suppressed the phone alert once the attended PC covered it. |
| Memos delivery | Created a private test memo in the original account. The live worker delivered it to Dun; its source link and completed state synced to the phone. Caught and fixed Memos flattening extra `/` tag separators into `-`. Slash and flattened forms, recurring/relative shapes, adapter upgrades and user isolation pass Node tests. |
| Arbor delivery | Authenticated live service created a temporary source item and sent its reminder. A retry returned the same ID with `created: false`. Removing the source item left the Dun reminder independent. Phone completion reached the PC. |
| Retry after cleanup | Memos and timer retries returned `created: false, archived: true`; they did not revive the test items. |
| Cleanup | Archived all six temporary Dun items, the private Memos test memo and the Arbor source item. Verified Dun cleanup on both devices. Restored font 1.0, portrait lock, dark mode and original USB stay-awake setting; removed debugger forwarding and idle override. |

Remaining device matrix: full DND combinations, notification permission denial,
mute/quiet hours, all expired-alert button taps, and launcher/recent-app visuals.
The attempt to force deep Doze stopped at `INACTIVE` with an imminent alarm-clock
wake; deep-Doze expiry was not verified. Countdown rendering uses Android's own
chronometer, and alarm inspection confirmed a single scheduled Dun wake rather
than a per-second countdown alarm. The phone clock was about 25 seconds ahead of
the PC during this session; tests compared saved deadlines with the phone clock.
