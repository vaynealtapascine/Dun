package app.dun.android

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.media.AudioAttributes
import android.media.RingtoneManager
import android.net.Uri
import android.os.Build

/**
 * Android ties a sound to a channel, not to a notification, so each bundled
 * chime gets its own channel. That also means the user can set volume, vibration
 * and Do Not Disturb behaviour per chime in Android's own settings.
 *
 * Channel ids and names must match what Rust puts in a plan: `ring_<chime>`
 * for reminders, `timer_<chime>` for the same sounds on the alarm stream.
 */
object Channels {
    /** Falls back to the system notification sound. */
    const val RING_DEFAULT = "ring_default"

    /** Used for alerts the scheduler marked silent (muted, or the PC has it). */
    const val RING_SILENT = "ring_silent"

    /** Fallback when Rust fails: always audible so a failure is never silent. */
    const val ERRORS = "errors"

    /** Timers with no bundled chime of their own. */
    const val TIMER_DEFAULT = "timer_default"

    /** The ongoing countdown a running timer shows. Seen, never heard. */
    const val TIMERS_RUNNING = "timers_running"

    /**
     * Suffix of the twin channel that bypasses Do Not Disturb.
     *
     * A channel's settings are fixed once Android has seen it, and a request to
     * bypass Do Not Disturb is dropped unless the app already had the access
     * when the channel was made. Granting the access later therefore can't
     * change the channel we already created — so the bypassing version is a
     * channel of its own, made the moment the access appears.
     */
    private const val DND_SUFFIX = "_dnd"

    /**
     * Bumped whenever a channel's definition changes.
     *
     * Android freezes a channel the first time it sees it, and deleting one
     * only to recreate it under the same id brings the old settings back — so
     * a fix to a channel can only reach a phone that already has it under a
     * new id. Generation 2 repairs generation 1, whose chime sounds pointed at
     * numeric resource ids: those are renumbered by every build, so the ids
     * baked into the channels drifted onto whatever resource happened to take
     * them (a dialog style, as it turned out) and every ring was silent.
     */
    private const val GENERATION = 2

    /** The concrete channel id for one of the names Rust uses. */
    fun id(base: String, dnd: Boolean = false): String =
        base + (if (dnd) DND_SUFFIX else "") + "_g" + GENERATION

    /** Bundled chimes, matching `res/raw/chime_*.wav` and the desktop's list. */
    private val CHIMES = listOf(
        "bell" to "Bell",
        "rise" to "Rise",
        "pulse" to "Pulse",
        "soft" to "Soft",
    )

    fun ensure(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val nm = context.getSystemService(NotificationManager::class.java)
        val attrs = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_NOTIFICATION_EVENT)
            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
            .build()
        // Timers ring on the alarm stream. A phone on silent still plays it,
        // and Do Not Disturb lets alarms through by default — which is what
        // someone who set a countdown asked for. Reminders stay on the
        // notification stream, where silent means silent.
        val alarmAttrs = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_ALARM)
            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
            .build()
        val systemSound = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_NOTIFICATION)
        val systemAlarm = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM) ?: systemSound

        val channels = mutableListOf(
            ringing(id(RING_DEFAULT), "Reminders", systemSound, attrs),
            NotificationChannel(id(RING_SILENT), "Reminders (quiet)", NotificationManager.IMPORTANCE_LOW).apply {
                description = "Reminders shown without a sound, because Dun is muted or your PC is ringing"
                setSound(null, null)
                enableVibration(false)
            },
            NotificationChannel(id(ERRORS), "Problems", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "Shown when Dun could not check your reminders"
                setSound(systemSound, attrs)
            },
            alarming(id(TIMER_DEFAULT), "Timers", systemAlarm, alarmAttrs),
            NotificationChannel(id(TIMERS_RUNNING), "Running timers", NotificationManager.IMPORTANCE_LOW).apply {
                description = "The countdown a running timer keeps on screen"
                setSound(null, null)
                enableVibration(false)
                setShowBadge(false)
            },
        )
        for ((chime, label) in CHIMES) {
            val sound = rawUri(context, "chime_$chime")
            channels += ringing(id("ring_$chime"), "Reminders ($label)", sound, attrs)
            channels += alarming(id("timer_$chime"), "Timers ($label)", sound, alarmAttrs)
        }
        if (nm.isNotificationPolicyAccessGranted) {
            channels += alarming(id(TIMER_DEFAULT, dnd = true), "Timers", systemAlarm, alarmAttrs)
            for ((chime, label) in CHIMES) {
                channels += alarming(
                    id("timer_$chime", dnd = true),
                    "Timers ($label)",
                    rawUri(context, "chime_$chime"),
                    alarmAttrs
                )
            }
        }
        nm.createNotificationChannels(channels)

        // Channels from an older generation are broken, not merely stale, and
        // leaving them behind fills the user's notification settings with
        // duplicates that do nothing.
        val keep = channels.map { it.id }.toSet()
        for (existing in nm.notificationChannels) {
            if (existing.id !in keep && MINE.any { existing.id.startsWith(it) }) {
                nm.deleteNotificationChannel(existing.id)
            }
        }
    }

    /** Prefixes of every channel Dun has ever created. */
    private val MINE = listOf("ring_", "timer_", "timers_", "errors")

    /**
     * The channel to actually post on for the one Rust asked for.
     *
     * Rust knows nothing about Do Not Disturb access, so it names the plain
     * timer channel and this swaps in the bypassing twin when there is one.
     */
    fun effective(context: Context, channel: String): String {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return channel
        val nm = context.getSystemService(NotificationManager::class.java)
        if (channel.startsWith("timer_") && nm.getNotificationChannel(id(channel, dnd = true)) != null) {
            return id(channel, dnd = true)
        }
        return id(channel)
    }

    /**
     * Like [ringing], but on the alarm stream and asking to be heard through
     * Do Not Disturb.
     *
     * The bypass only takes effect once the user has given Dun Do Not Disturb
     * access; without it Android quietly ignores the flag, and the alarm usage
     * is doing the work on its own.
     */
    private fun alarming(id: String, name: String, sound: Uri?, attrs: AudioAttributes) =
        NotificationChannel(id, name, NotificationManager.IMPORTANCE_HIGH).apply {
            description = "Rings until you mark the timer done, even on silent"
            enableVibration(true)
            setBypassDnd(true)
            setSound(sound, attrs)
        }

    private fun ringing(id: String, name: String, sound: Uri?, attrs: AudioAttributes) =
        NotificationChannel(id, name, NotificationManager.IMPORTANCE_HIGH).apply {
            description = "Rings and nags until you mark it done"
            enableVibration(true)
            setSound(sound, attrs)
        }

    /**
     * By name, never by number: resource ids are renumbered by every build, and
     * a channel keeps the URI it was made with, so a numeric one goes stale the
     * moment anything else in the app changes.
     */
    private fun rawUri(context: Context, name: String): Uri =
        Uri.parse("android.resource://${context.packageName}/raw/$name")
}
