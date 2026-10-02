---
name: Dun
description: "Interval instrument: time, state, and an explicit next action."
colors:
  bg: "#e9eef1"
  bg-raised: "#f7f9fa"
  bg-sunken: "#dfe7ec"
  fg: "#162b37"
  fg-muted: "#546873"
  fg-faint: "#788b96"
  border: "#cbd7df"
  accent: "#287d87"
  accent-fg: "#ffffff"
  ringing: "#b83424"
  ringing-bg: "#fae5df"
  alarm: "#d4432a"
  alarm-fg: "#ffffff"
  overdue: "#855512"
  overdue-bg: "#f6e8cc"
  ok: "#276b4b"
  focus: "#287d87"
  timer-track: "#d7e0e6"
  bg-dark: "#101e26"
  bg-raised-dark: "#182c37"
  bg-sunken-dark: "#243c49"
  fg-dark: "#eef4f5"
  fg-muted-dark: "#b0c1ca"
  fg-faint-dark: "#7f949f"
  border-dark: "#37505d"
  accent-dark: "#7fc5cc"
  accent-fg-dark: "#112831"
  ringing-dark: "#ff9a86"
  ringing-bg-dark: "#412b29"
  overdue-dark: "#edc87f"
  overdue-bg-dark: "#3c3424"
  ok-dark: "#8cdbb1"
  focus-dark: "#7fc5cc"
  timer-track-dark: "#354e5b"
typography:
  display:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "clamp(5.5rem, 25vw, 7.375rem)"
    fontWeight: 700
    lineHeight: 0.9
    letterSpacing: "-0.025em"
  display-long:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "clamp(3rem, 13vw, 4.4rem)"
    fontWeight: 700
    lineHeight: 0.9
    letterSpacing: "-0.025em"
  countdown:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "2.5rem"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "-0.025em"
  countdown-long:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "1.55rem"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "-0.025em"
  setter:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "2.1rem"
    fontWeight: 700
    lineHeight: 1
  reminder-countdown:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "1.5rem"
    fontWeight: 600
    lineHeight: 1.2
  reminder-countdown-compact:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "1.25rem"
    fontWeight: 600
    lineHeight: 1.2
  headline:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "1.75rem"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "-0.025em"
  title:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "1.5rem"
    fontWeight: 700
    lineHeight: 1.15
  section-title:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "1.3rem"
    fontWeight: 700
    lineHeight: 1.1
  alarm-state:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "2rem"
    fontWeight: 700
    lineHeight: 1
  state:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "1.15rem"
    fontWeight: 700
    lineHeight: 1.25
  body:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "1rem"
    fontWeight: 400
    lineHeight: 1.4
  label:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "1rem"
    fontWeight: 600
    lineHeight: 1.4
  label-strong:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "1rem"
    fontWeight: 700
    lineHeight: 1.4
  timer-action:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "1.2rem"
    fontWeight: 700
    lineHeight: 1.2
  unit-label:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "0.7rem"
    fontWeight: 400
    lineHeight: 1.4
    letterSpacing: "-0.03em"
  preset:
    fontFamily: "Source Sans 3, Segoe UI, sans-serif"
    fontSize: "0.75rem"
    fontWeight: 700
    lineHeight: 1.4
rounded:
  radius: "8px"
  radius-sm: "6px"
  compact: "4px"
spacing:
  tight: "0.35rem"
  control-gap: "0.4rem"
  workspace-gap: "0.5rem"
  row-gap: "0.5625rem"
  action-gap: "0.6rem"
  page-inline: "0.625rem"
  compact-padding: "0.75rem"
  card-inline: "0.85rem"
  control-inline: "0.9rem"
  row-inline: "1rem"
  dial-gap: "1.5rem"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.accent-fg}"
    typography: "{typography.label-strong}"
    rounded: "{rounded.radius-sm}"
    padding: "0.5rem 0.9rem"
    height: "2.75rem"
  button-primary-hover:
    backgroundColor: "color-mix(in srgb, var(--accent) 88%, var(--fg))"
  button-secondary:
    backgroundColor: "{colors.bg-sunken}"
    textColor: "{colors.fg}"
    typography: "{typography.label}"
    rounded: "{rounded.radius-sm}"
    padding: "0.5rem 0.9rem"
    height: "2.75rem"
  button-icon:
    backgroundColor: "transparent"
    textColor: "{colors.fg-muted}"
    rounded: "{rounded.radius-sm}"
    size: "2.75rem"
  input:
    backgroundColor: "{colors.bg-raised}"
    textColor: "{colors.fg}"
    typography: "{typography.body}"
    rounded: "{rounded.radius-sm}"
    padding: "0.5rem 0.7rem"
    height: "44px"
    width: "100%"
  chip:
    backgroundColor: "{colors.bg-raised}"
    textColor: "{colors.fg}"
    rounded: "{rounded.radius-sm}"
    padding: "0.3rem 0.7rem"
    height: "36px"
  chip-selected:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.accent-fg}"
    typography: "{typography.label-strong}"
  navigation-tab:
    backgroundColor: "transparent"
    textColor: "{colors.fg-muted}"
    typography: "{typography.label}"
    padding: "0.45rem 0.25rem"
    height: "44px"
  navigation-tab-selected:
    textColor: "{colors.accent}"
    typography: "{typography.label-strong}"
  timer-row:
    backgroundColor: "{colors.bg-raised}"
    textColor: "{colors.fg}"
    rounded: "{rounded.radius}"
    padding: "0.55rem 1rem"
  timer-ringing:
    backgroundColor: "{colors.alarm}"
    textColor: "{colors.alarm-fg}"
    rounded: "{rounded.radius}"
    padding: "0.4rem 0.85rem 0.53rem"
  timer-held:
    backgroundColor: "{colors.ringing-bg}"
    textColor: "{colors.ringing}"
    rounded: "{rounded.radius}"
    padding: "0.4rem 0.85rem 0.53rem"
  timer-action:
    typography: "{typography.timer-action}"
    rounded: "{rounded.radius-sm}"
    padding: "0.45rem 0.6rem"
    height: "2.75rem"
  duration-input:
    backgroundColor: "{colors.bg-raised}"
    textColor: "{colors.fg}"
    typography: "{typography.setter}"
    rounded: "{rounded.radius-sm}"
    padding: "0.1rem"
    height: "2.85rem"
    width: "100%"
  timer-start:
    backgroundColor: "{colors.bg-raised}"
    textColor: "{colors.fg}"
    rounded: "{rounded.radius}"
    padding: "0.4rem 0.85rem"
  preset:
    backgroundColor: "{colors.bg-sunken}"
    textColor: "{colors.fg}"
    typography: "{typography.preset}"
    rounded: "{rounded.radius-sm}"
    padding: "0.45rem 0.55rem"
    height: "max(44px, 2.1rem)"
---

# Design System: Dun

## Overview

**Creative North Star: "Interval instrument"**

The Interval instrument makes time easy to read and the next action easy to name. Cool gray surfaces and ink text provide a quiet ground; teal identifies activity, while a locally ringing timer occupies a solid red face. Broad numerals carry the measurement. Rectangular controls carry the decision.

Dun uses the same Svelte interface in Windows and Android shells. The visual system must preserve the product’s durable alert semantics: ringing, snoozed, held, running, paused, and ready are different states. A quieter appearance records an explicit delivery state; it never implies completion. Layout, color, and type provide hierarchy without ornamental imagery or a decorative pulse.

**Key Characteristics:**

- Humanist Source Sans 3 throughout, with bold tabular countdown numerals.
- Flat cool surfaces, restrained borders, and compact rectangular controls.
- Dominant red only for a locally ringing alert or an error requiring attention.
- Data-driven remaining-time arcs, teal for running and muted ink for paused.
- One readable column with visible quick entry, stable active timer order, and direct labeled actions across phone and desktop.

The normative values above come from the finished implementation. The sidecar extends them with motion, breakpoints, shadow vocabulary, component previews, and the same narrative.

## Colors

The palette combines cool gray ground, blue ink, instrument teal, and direct expiry red. Color names retain the source custom-property names; a `-dark` suffix denotes the corresponding dark-theme override. Bind interface styles to the unsuffixed CSS variables, rather than choosing a light value directly.

### Primary

- **Instrument teal** (`accent`, `accent-fg`): primary actions, the selected navigation underline, running state text, and running arcs. The dark theme uses the `accent-dark` pair for legibility.
- **Local expiry red** (`alarm`, `alarm-fg`): the large locally ringing timer face, unread ringing-count badges, and visible error notice. These tokens remain constant in both themes.
- **Quiet expiry ink and surface** (`ringing`, `ringing-bg`): held or snoozed due timers and related warning text. Their dark overrides provide a soft coral foreground on a dark red-brown surface.

### Secondary

- **Amber caution** (`overdue`, `overdue-bg`): delivery-related caution, including the temporary mute status. It is not a second activity accent.
- **Confirmation green** (`ok`): successful or healthy status in the existing interface. Preserve its semantic role when extending other views.

### Neutral

- **Cool ground** (`bg`): the app and surrounding desktop canvas.
- **Raised paper** (`bg-raised`): timer rows, duration entry, fields, and filter chips.
- **Sunken gray** (`bg-sunken`): secondary controls, presets, and hover support.
- **Ink** (`fg`): names and primary interface text; **muted ink** (`fg-muted`) supports secondary state text, and **faint ink** (`fg-faint`) supports low-priority chrome.
- **Structural stroke** (`border`): restrained card, field, navigation, and control boundaries.
- **Timer track** (`timer-track`): the inactive portion of a remaining-time arc.
- **Focus teal** (`focus`): the visible keyboard-focus outline; it follows activity teal in each theme.

Explicit `data-theme="light"` and `data-theme="dark"` win over the OS preference. System mode follows `prefers-color-scheme`; it uses the same dark overrides as explicit dark mode. The alarm pair is deliberately unchanged.

**The Local Ring Rule.** Use the solid alarm face only when the item is due, is not snoozed, and has no held delivery reason. Snoozed and held timers use the quieter ringing surface and state text.

## Typography

**Display Font:** Source Sans 3 Bold, with Segoe UI and sans-serif fallbacks for timer faces.
**Body Font:** Source Sans 3, with Segoe UI and sans-serif fallbacks.
**Character:** One humanist family gives countdowns and interface text a consistent, mature voice. Bold countdown numerals use lining tabular figures and tight tracking; regular and semibold text make names and controls easy to scan. Labels use meaningful sentence case.

### Hierarchy

- **Display / display-long:** expired or snoozed countdowns. The short face uses the responsive display token; long clocks use the smaller display-long token. The type retains its natural proportions with a 0.9 line height; day prefixes remain separate and smaller.
- **Countdown / countdown-long:** remaining time inside the dial. At widths up to 420px, the normal clock becomes 2rem and the long clock 1.35rem.
- **Reminder-countdown / reminder-countdown-compact:** one semibold `YYy MMm DDd HH:MM:SS` line in the right side of the reminder row, at 1.5rem with a 1.2 line height. Use 1.25rem at widths up to 600px and for scaled portrait layouts at least 760px wide with an aspect ratio no greater than 3:4. All six fields remain visible. Leading empty units and the padding zero of every positive unit below 10 use 0.42 opacity; meaningful zeros after the first nonzero unit stay readable, and an all-zero readout retains legible seconds. Use lining tabular figures at natural width, with only y/m/d suffixes beside the calendar units.
- **Setter:** inline numeric Hours / Minutes / Seconds entry. At widths up to 399px, the input numerals become 1.7rem.
- **Headline:** the compact Dun wordmark and shell heading.
- **Title:** timer name; a due timer title is slightly larger (1.65rem).
- **Section-title:** New timer and Ready to start headings. Ready to start uses the same size and weight with a 1.2 line height.
- **Alarm-state / state:** Time’s up or Snoozed on the expiry face, and Running / Paused / Ready beside the dial.
- **Body / label:** ordinary reading and navigation; narrow navigation uses 0.9rem at widths up to 600px.
- **Timer-action:** firm labeled timer controls. Narrow row actions use 1.1rem at widths up to 420px.
- **Unit-label / preset:** compact field descriptions and preset name-duration pairs.

The root size is 16px with a 1.4 line height. There is no imposed geometric type-scale ratio. At viewport widths of at least 760px and aspect ratios of at most 3/4, the portrait presentation uses `min(3.3vw, 32px)` as the root size.

Source Sans 3 normal 400/600/700 is self-hosted for offline consistency in `SourceSans3-Regular.woff2`, `SourceSans3-Semibold.woff2`, and `SourceSans3-Bold.woff2`. Keep these unmodified files with `public/fonts/ORIGIN.md` and `SourceSans3-OFL.md`; the family carries the SIL OFL 1.1 license.

**The Humanist Type Rule.** Use self-hosted Source Sans 3 throughout. Timer countdowns and duration entry use weight 700; compact reminder countdowns use weight 600. Both use lining tabular numerals. Names, states, navigation, and actions use the same family at their established weights.

## Layout

The shell is a centered single column capped at 960px and fills `100dvh`. The header and underlined tab strip stay above a scrolling main area. Main content uses the page-inline token with a compact top inset and safe-area-aware bottom padding. Ordinary desktop widening preserves the same column and makes the action regions wider; it does not add timer columns. The portrait scaling rule removes the shell cap while scaling rem-based UI.

Timer lists use the row-gap token. Each non-due row places a circular dial to the left of a flexible name, state, source link, and action region. The normal dial is 8.625rem with the dial-gap token; up to 420px it is 7.75rem, the gap becomes 1rem, and padding becomes 0.75rem. Names can wrap to two lines and long unbroken names wrap within the row.

A due timer changes to a stacked face: name and menu, centered time and state, then three equal action columns. The New timer form keeps its three numeric fields inline beside Start timer. Its field-to-action grid is 1.83fr to at least 7rem; up to 399px it becomes 1.7fr to at least 6.4rem with tighter separator spacing. Presets form a horizontal strip of three equal minimum slots and scroll when more are present.

Quick entry remains visible above both Timers and Reminders. The timer surface promotes expired entries first, then keeps running and paused timers together in immutable creation order (newest first, ID as the deterministic tie-breaker). Pause, resume, countdown updates, title changes, and snapshot order do not reshuffle active timers. Starting a ready timer moves it into the active group; ready timers follow the setter. Timers is the default tab. These are the current surface’s composition, not a required order for every future screen.

The approved visual floor is 360px. The CSS body minimum remains 320px as a compatibility declaration; it does not establish a reviewed 320px design target. At widths up to 600px, tab badges participate inline beside the text; wider tabs position the badge to the right. Keep this responsive rule after the desktop badge rule.

The arc represents remaining time divided by the scheduled duration and decreases with the existing application clock. Pausing changes the stroke to muted ink; it does not replace the remaining-time value.

## Elevation & Depth

Resting cards, fields, presets, and controls use flat tonal layering and restrained one-pixel boundaries. The shared diffuse shadow appears on floating menus, notices, errors, and the reminder add action. It uses the source `--shadow` variable, including its darker theme override; the sidecar records both values. No timer face gains a raised-card shadow.

**The Flat Instrument Rule.** Timer rows and the duration setter remain flat. Reserve the shared diffuse shadow for floating menus, notices, errors, and the reminder add action.

## Shapes

The form language is rectangular with gently eased corners: the radius token for cards and floating containers, radius-sm for controls and fields, and compact corners for navigation details and badges. Circular geometry carries timer data, tag dots, and switches; it is not a general card shape. The dial is an inline SVG with a 44-unit radius in a 100-unit view box, a seven-unit stroke, and a round progress cap.

## Components

### Buttons

Confident labeled controls give the action a clear name. Primary actions use the activity pair; secondary actions use the sunken surface and structural border. General controls use label typography and the primary variant raises weight to 700. Timer actions use the timer-action typography, and their horizontal space grows with the row. Hover mixes the current surface with ink; active and disabled states remain visible. Icon controls retain accessible names and a 44px target.

Global focus is a two-pixel outline with a three-pixel offset. On the solid alarm face, focus uses the alarm foreground. State changes use the fast motion token; a pressed control scales to 0.97 so a tap is answered at once. Hover styles apply only under `(hover: hover)`, so a tapped control does not keep a hover color on touch screens.

### Chips

Tag filters are compact rectangular controls on raised paper. The selected chip uses activity teal with its paired foreground and bold text. Tags can include a small user-color dot; never infer status solely from that dot. The strip scrolls horizontally when space is limited.

### Cards / Containers

Timer rows and the setter share raised paper, a structural border, and the radius token. Due timers use the quiet expiry surface; only locally ringing timers use the solid alarm fill. State-driven fill changes replace an ornamental elevation effect.

### Inputs / Fields

Quick entry is always available above Timers and Reminders, preserving the phrase parser, schedule preview, Enter to add, and Shift+Enter or More to open the full form. Ctrl+K focuses it. Standard fields use raised paper, a structural border, radius-sm, and an accent caret. The duration setter centers firm tabular numerals and labels every field. Hours accepts 0–999, minutes and seconds accept 0–59, and at least one second is required. Busy input is disabled; errors appear as adjacent text with an alert role. Padding short fields to two digits happens on blur.

### Navigation

Timers, Reminders, and History divide the tab strip equally. The selected tab uses teal, weight 700, and a single bottom underline that travels to the selected tab. The incoming panel enters from the side of travel, each tab keeps its own scroll position, and tapping the current tab scrolls back to the top. Hover uses the sunken surface; arrow keys, Home, and End move between tabs. Ringing counts appear on inactive tabs only and use local expiry red. Preserve the compact inline badge cascade on narrow screens.

### Timer instrument

Running and paused rows share a stable creation-based order and show remaining time with direct Pause / Resume and Reset controls. Expired timers move ahead of that group; pausing and resuming do not change position within it. The SVG progress uses a remaining-duration fraction. While running, its stroke offset transitions for 1s linearly so it drains continuously between clock ticks; other changes (start, reset, edit) sweep once with the ease-out curve. Reduced motion snaps to the data. A finished face fades and settles into place once; it never pulses. The ticking numeral itself does not enter a live region.

A due face distinguishes Time’s up, Snoozed, Quiet hours, Alerts muted, and Waiting on phone. A snoozed face shows time until the alert resumes; a held face reports elapsed time since completion of the countdown. Done, Snooze, and Restart remain separate controls, and the title still opens editing. The visible state has a polite, atomic status announcement outside the ticking clock.

### Timer start and presets

The setter and primary launch button form one compact instrument. Preset name and duration share a labeled launch control; management is exposed through the existing menu action and separate edit controls. Preserve source links, context actions, swipe archive, and completion Undo when adapting timer presentation.

### Reminder countdown

Reminder rows keep the title, scheduled date, recurrence, Done, Snooze, and context actions. A single `YYy MMm DDd HH:MM:SS` countdown sits to their right, before the grouped Snooze and context actions; all six fields remain visible even when zero, without additional labels. Desktop rows use Done, flexible title/details, a max-content numeric column, and the action group. At widths up to 600px, and in scaled portrait layouts at least 760px wide with an aspect ratio no greater than 3:4, title/details occupy the top row beside Done, with actions below on the left and the countdown line below on the right. Retain two rows rather than stacking actions beneath the readout. Leading empty units, including their separators and suffixes, inherit the semantic text color at 0.42 opacity. The padding zero of each positive unit below 10 also uses 0.42 opacity. Apply opacity once rather than compounding nested spans. Meaningful zeros after the first nonzero unit retain normal emphasis so clocks such as 10:00 remain clear; all-zero readouts keep seconds at normal emphasis. Years, months, and days follow the user's local calendar with end-of-month clamping; the remaining clock reflects actual elapsed time across daylight-saving changes. Future durations round seconds up; elapsed durations round them down.

Upcoming reminders count toward the next occurrence; snoozed reminders count toward alert resumption. Unsnoozed overdue reminders count elapsed time from the first missed occurrence, or the current occurrence when none is recorded, and use the ringing text color. Preserve Due in, Alerts resume in, Overdue by, Due now, and Resuming alerts in the complete spoken aria-label without printing these labels beside the digits. At an elapsed deadline, clamp pending status to zero until the core updates it. Idle and unreadable schedules have no countdown. Use the shared app clock rather than a separate interval; the timer role has aria-live off, so ticks do not repeatedly announce.

## Motion & Gestures

Motion explains a change the person caused or needs to notice: where a row went, which tab is showing, that a sheet can be put away. Nothing loops, and no element animates for decoration. Tokens live on `:root` (`--ease-out`, `--ease-in-out`, `--dur-fast` 120ms, `--dur` 200ms, `--dur-slow` 280ms); Svelte transitions share them through `src/lib/motion.ts`, which also makes every transition instant under reduced motion.

- **Lists.** Rows rise 8px into place when added, glide to new positions when the order changes (an expired timer moving to the top), and collapse with their gap when completed or archived. Completing a reminder fills its check and strikes the title as the row leaves.
- **Swipe to archive.** Touch, mouse drag, and trackpad two-finger swipes all move a row. Releasing past 40% of the 88px button opens it; a quick flick decides on its own; a long swipe (55% of the row, at least 176px) archives outright with a haptic tick on Android as it arms. Only one row is open at a time, the first tap on an open row closes it, and Escape closes it from the keyboard. Archive keeps its Undo.
- **Dialogs.** Desktop dialogs fade and rise slightly; on phones (≤540px) they are bottom sheets with a handle that can be dragged down to dismiss. Close and Escape animate out before the dialog closes.
- **Menus, toasts, and badges.** Context menus scale in from the corner they open from. Toasts rise in and fade out. A new ringing count on an inactive tab arrives with a short pop.
- **Pointer conveniences.** Duration fields select on focus, step with Up/Down (Shift for ten) and with the wheel once focused. Preset and tag strips scroll horizontally with an ordinary mouse wheel. The reminder add button tucks its label away while the list scrolls down.

**The Explained Motion Rule.** Animate a change of place or state the person caused or must notice; never animate for ornament, and never repeat.

## Do's and Don'ts

### Do:

- **Do** use the semantic CSS variables so light, dark, and system themes retain the same state hierarchy.
- **Do** keep the alarm red constant across themes, with white text and a high-contrast Done action.
- **Do** preserve large firm numerals, readable state text, and labeled Pause, Resume, Reset, Done, Snooze, and Restart actions.
- **Do** retain two-column dial rows at the approved phone floor, reducing the dial and gaps through the existing narrow-width rules.
- **Do** make duration entry inline with Hours, Minutes, and Seconds labels and keep preset launch separate from preset editing.
- **Do** use accessible names, visible focus, semantic controls, and reduced-motion behavior with data-driven SVG arcs.
- **Do** preserve visible quick entry, stable active timer positions, context menus, source links, filters, swipe archive, and Undo when extending this visual world.

### Don't:

- **Don’t** give snoozed or held expiry the locally ringing solid red face, or describe either state as Done.
- **Don’t** reintroduce decorative display lettering or distort the humanist family’s natural proportions.
- **Don’t** add a decorative pulse, looping motion, simulated progress, gradient material, or raster artwork to the interval instrument.
- **Don’t** convert the approved single-column timer rows into a desktop tile dashboard.
- **Don’t** let a navigation count badge cover or displace the tab label at narrow widths.
- **Don’t** claim native audio, background delivery, or PC–phone sync has been verified by a browser preview.

This record includes the user’s refinement restoring visible quick entry, stabilizing timer order, and choosing humanist typography. The earlier portrait comp remains a layout reference; its original typography and absent quick-entry field are superseded by that correction. This record describes the current code and its reviewed visual matrix. Browser screenshots provide visual and interface-state evidence; they do not verify native sound, background alerts, or device sync. The prior numeral-weight and badge-cascade findings are resolved in the recorded implementation and are not reusable exceptions. No new bitmap UI assets were introduced; timers use text, semantic controls, and inline SVG.
