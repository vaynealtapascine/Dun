# Android countdown widget

Mode: Operate. A glanceable home-screen extension of Dun's established design.
User scope: 3×1 by default, resizable to 3×2 with Done and Snooze; next due or a
specific timer/reminder, with snooze taking precedence.

## Direction contract

THESIS: Read one item's remaining time from the home screen, with explicit state.
OWN-WORLD: Inherit Dun's cool surfaces, ink text and teal controls; native system
type and tabular numerals suit a launcher RemoteViews surface. Follow system night
mode because the widget sits among the phone's other widgets in varied light.
STORY: Add, choose Next due or a pinned item, glance, then expand to handle a due
item. A small settings control changes the selection independently per widget.
FIRST VIEWPORT: Item title above a large countdown, concise mode/state below;
48dp settings target at the right. Extra height reveals two labeled buttons.
FORM: Narrow user-specified extension; no concept seed. Native configuration list
and explicit Save, with cancel preserving the current selection.
FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

Uses existing DESIGN.md tokens without changing global design decisions. No
shipping rasters. Verification covers native RemoteViews inflation and state
selection, Rust projection, and light/dark renders at both requested sizes.

Finish verdict (2026-10-15): native light/dark renders at 260×80dp and 260×160dp
reviewed. Compact status clipping found in the first pass was fixed with reduced
vertical padding; all four confirmation renders fit. Android instrumentation
passes, as do all 14 Rust mobile tests. Existing DESIGN.md remains the authority.
Full APK launcher lifecycle and physical-device action delivery remain in the
Android device checklist; this review does not claim those checks were performed.

Countdown extension: a per-widget Ignore repeating reminders checkbox affects
Next due, while explicit pins remain available. Countdown choices use the existing
banner's solid, soft or outline colors, including its ink-color rule. Hide alarm
actions for countdowns, preserve the native live H:MM:SS reading, and label
Until/Since. Past countdowns are available as pins but do not block Next due.
Review: all six banner fill/theme combinations rendered and inspected, with
filter, pin, recurrence, DST and preference-restoration instrumentation passing.
