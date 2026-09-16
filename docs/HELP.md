# Dun: how it works

Dun holds reminders and countdown timers, and when one comes due it **keeps
nagging until you mark it done**. That is the whole idea: a notification you
ignore is a notification that comes straight back.

## Adding things

Type into the box at the top and Dun works out what you mean, showing a preview
before you commit:

| You type | You get |
|---|---|
| `Laundry in 45m` | a 45-minute timer, started |
| `Bread in 1h 30m`, `Tea for 4 min`, `Nap in half an hour` | the same, other ways |
| `Pay rent tomorrow at 9` | a one-off reminder |
| `Gym every mon, wed and fri at 7am` | a repeating reminder |
| `Water cactus every other day at 8` | every second day |
| `Stand up every 45 minutes` | an interval repeat, starting on the next minute |
| `Rent monthly on the 1st at 10am` | monthly |
| `Mum's birthday every year on Mar 3` | yearly |
| `Take pill every 8 hours after I finish` | the next one is counted from when you tap Done |
| `Bins #home tomorrow` | tagged |
| `Laundry in 45m nag 5m` | nags every 5 minutes instead of every minute |
| `FYI meeting tomorrow at 2pm ring once` | rings once and stops |

A date with no time uses 9:00 by default. If a time has already passed today,
Dun rolls it to tomorrow. Anything it can't place stays as the title, and
**More…** opens the full form.

On the PC, **Ctrl+Alt+N** (rebindable in Settings) opens a quick-add bar
wherever you are.

## When something rings

- **Windows** shows a notification with **Done · +1m · +5m · +15m** and plays a
  chime. Each nag replaces the last one rather than stacking, and Done makes it
  disappear — including from Action Center, even if Dun has been quit.
- **Android** shows the same thing with **Done · +5m · +15m**. Swiping it away
  is not "done": it will be back at the next nag.

Marking something done from a notification works even when Dun isn't running.

If a reminder was due while the device was asleep or off, it rings as soon as
the device is back and says so — "missed at 4:05 PM", or "missed 3 times since
4:05 PM" for a repeat that came round more than once. If more than three things
are newly missed, the two newest ring on their own and the rest are collapsed
into one summary you can snooze in a single tap.

## PC and phone together

Pair them and both show one list, syncing directly over your network — nothing
goes to a server on the internet.

1. On the PC: **Settings → Sync**, turn it on (Windows asks once for permission
   to accept connections), then **Pair a phone…**
2. On the phone: **Settings → Sync with your PC**, and either scan the QR code
   or paste the link shown under "Can't scan?".

The code is good for five minutes and works once.

**Which one rings?** Whichever is more likely to reach you:

- You're at the PC → the PC rings, and the phone stays quiet.
- You're away, locked, or idle for more than five minutes → the phone rings.
- The phone can't be reached, or hasn't confirmed it is covering the reminder →
  the PC rings anyway, two minutes after the item was due. Dun would rather be
  annoying than silent.

Done on the phone stops the PC within seconds. Done on the PC clears the phone
by its next nag.

### If the phone never gets through

- **Windows firewall.** Dun adds its own rule, but only for networks Windows
  considers *private*. If Settings says your network is set to Public, open
  Windows' network settings and change it to Private — otherwise Windows blocks
  the connection no matter what Dun does.
- Both devices need to be on the same network. Away from home, install
  Tailscale on both and they'll find each other through it.

## Quiet hours, mute and sounds

- **Quiet hours** hold reminders silently overnight and let them through when
  the window ends. Timers are exempt by default; any item can be set to ring
  anyway.
- **Mute** for 15 minutes, an hour, or until tomorrow, from the tray or
  Settings. The first alert still appears silently so nothing is lost, and mute
  applies to both devices.
- Each item can have **its own chime**, or use the default. On the PC you can
  import your own WAV, MP3 or OGG. On the phone the four bundled chimes are
  notification channels, so Android's own per-channel volume and Do Not Disturb
  settings apply to each of them.

## Keeping Dun alive

**Windows.** Dun starts at login (Settings) and lives in the tray. Closing the
window hides it; **Quit** from the tray menu is the real exit. The tray icon
shows a badge while something is ringing, and its tooltip says what's next.

**Android.** Open **Settings** on the phone and work through *Ringing reliably*
— it shows live status for each item:

- **Notifications** must be allowed, or Dun can't tell you anything.
- **Alarms & reminders** must be allowed, or nothing can wake the phone at an
  exact time.
- **Unrestricted battery use**: on Samsung phones this is the single most common
  cause of a missed reminder overnight.
- Also worth doing on Samsung: **Settings → Battery → Background usage limits**,
  and make sure Dun is not in *Sleeping apps* or *Deep sleeping apps*.
- **Never force-stop Dun.** Android cancels an app's alarms when you force-stop
  it, and nothing will ring until you open Dun again.
- After a reboot, alarms are restored once you unlock the phone for the first
  time. Before that first unlock, Android keeps Dun's data encrypted and nothing
  can run.

## History, undo and backups

The **History** tab lists what you've finished. Undo puts an item back as if it
had never been done; Restart runs a timer again from the top.

**Settings → Backup** writes everything to a JSON file. Importing offers two
ways:

- **Merge** keeps whatever is newer on either side.
- **Replace** makes this device — and, once they sync, your other devices —
  match the file exactly.

## Small print

- Repeating reminders can count **from the schedule** (every Monday at 9,
  whatever you do) or **after you finish** (eight hours after you took the last
  pill).
- Editing a repeat's rule never makes the old schedule ring retroactively.
- Reminders written as clock times follow the device's time zone. A reminder set
  for a clock time that a daylight-saving change removes shifts forward; one in
  a repeated hour uses the first.
- "Monthly on the 31st" falls on the last day of shorter months.
