# Dun

<!-- impeccable:product-schema 1 -->

## Platform

web

Dun shares a Svelte interface between Windows and Android Tauri shells. This platform value describes the interface's design language; the shipped product is an installed app on both operating systems. Browser preview is a development tool backed by mock data.

## Users

Dun serves both its creator's daily personal use and wider distribution to individual users. The current supported context is a Windows PC, with an optional paired Android phone.

The primary job is to capture something that needs doing, set when it matters, and keep being reminded until it is handled, including when moving between the PC and phone. No narrower demographic or professional audience has been established.

## Product Purpose

Dun provides reminders and countdown timers with persistent, durable, intentionally annoying alerts. This is its most important product commitment, confirmed by the user during init.

Success means the reminder reaches the user wherever they are and keeps prompting an explicit response. It should avoid ringing on devices where the user is absent. Persistence includes retaining schedules and timer state across restarts and recovering missed occurrences, subject to operating-system restrictions.

## Positioning

Dun combines repeated alerts until **Done** with direct PC–phone sync and device-presence handoff. The PC is the hub; an optional Android peer shares the list. The PC rings while attended, and the phone takes over when the PC is locked or idle. A delayed PC fallback favors an alert over silence when phone coverage is unconfirmed.

The repository cites Due for iOS as an inspiration. Dun currently implements Windows and Android support.

## Operating Context

- Capture a reminder or timer through natural-language quick add, review the interpreted time or duration, and submit it. **More** opens the full form. A configurable Windows hotkey opens a separate quick-add window.
- Review **Reminders**, **Timers**, and **History**; search or filter by tag. Reminders support one-off and recurring schedules, including repeats counted from the schedule or from completion. Timers support presets, pause, resume, and restart.
- Use **Done** or **Snooze** from the list or notification. Dismissing a notification does not complete an item; repeat nagging can bring it back.
- Recover through completion **Undo**, **Archive**, and **Restore**. Archive stops alerts; restoring an overdue item may make it ring again.
- On Windows, closing the window hides it to the tray. The tray provides background operation and an explicit **Quit** action.
- Pair PC and phone over LAN or Tailscale. Local storage and direct device sync avoid a public cloud service; pairing and network reachability still require setup.

## Capabilities and Constraints

- Preserve the distinction between due, ringing, snoozed, and held alerts. Quiet hours, temporary mute, and handoff affect delivery without completing a reminder. Repeat nagging is configurable, and items can also ring once.
- Timers are exempt from quiet hours by default. Item-level exceptions and sound choices are supported. Alert persistence must respect explicit user controls.
- Android delivery depends on notification and exact-alarm permissions, battery policy, alarm volume, and Do Not Disturb. Force-stop cancels alarms until the app is reopened; after reboot, restoration requires the first unlock. Do not promise guaranteed delivery.
- Some preferences are shared, while device settings such as sounds, hotkeys, and window state are local. Do not assume identical delivery controls on both platforms.
- Windows app connections allow authenticated same-PC apps to create reminders and start timers. Memos and Arbor integrations retain source links. Completion in Dun and completion in the source app are independent.
- The existing stack is Svelte 5, TypeScript, Vite, Tauri 2, a shared Rust core with SQLite storage, and Kotlin Android delivery code. Maintain the shared interface and platform-specific shell behavior unless the product scope is deliberately changed.
- Time handling distinguishes absolute one-off/timer instants from recurring civil-time rules. Future copy must describe the relevant schedule type accurately.

## Brand Commitments

The product name is **Dun**. Existing identity assets include `assets/app-icon.svg` and the platform icons under `src-tauri/icons/`.

Persistence and annoyance are intentional parts of the promise. They serve the user's chosen reminder, with delivery directed toward the user's actual location and explicit controls for postponing or silencing alerts.

## Evidence on Hand

- `README.md`: product summary, supported platforms, development status, and setup.
- `docs/HELP.md`: user workflows, alert behavior, handoff, recovery, and delivery limitations.
- `docs/ARCHITECTURE.md` and `docs/SYNC-PROTOCOL.md`: device roles, storage, scheduling, time semantics, and direct sync.
- `docs/APP-API.md`: app connections and integration boundaries.
- `src/`: the incumbent shared interface; `src/lib/dev/mock.ts` provides development demonstration data, not real usage evidence.

These are repository evidence, not a fresh validation of Windows or Android delivery. No customer testimonials, adoption figures, delivery guarantees, or accessibility certification were established during init.

## Product Principles

1. **Keep prompting until handled.** Durable, repeated alerts are the core value; incidental interface changes must preserve their meaning and behavior.
2. **Reach the person.** Aim alerts at the attended device and use fallback when coverage is uncertain; avoid unnecessary alerts where the user is absent.
3. **Capture quickly and visibly.** Minimize interruption while making the interpreted schedule reviewable before submission.
4. **Honor explicit intent.** Keep completion distinct from snooze, dismissal, mute, and archive; retain recovery paths for mistakes.
5. **Preserve local ownership.** Keep local storage and direct device sync as the current product model, and describe network and platform limitations honestly.

## Open Product Decisions

Specific distribution channels, a narrower audience, additional supported platforms, and any required accessibility standard remain undecided. Init records product truth; it does not select a new visual direction.
