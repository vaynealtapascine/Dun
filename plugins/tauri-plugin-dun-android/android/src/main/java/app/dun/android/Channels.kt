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

    /** Bundled chimes, matching `res/raw/chime_*.wav` and the desktop's list. */
    private val CHIMES = listOf(
        Triple("bell", "Bell", R.raw.chime_bell),
        Triple("rise", "Rise", R.raw.chime_rise),
        Triple("pulse", "Pulse", R.raw.chime_pulse),
        Triple("soft", "Soft", R.raw.chime_soft),
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
            ringing(RING_DEFAULT, "Reminders", systemSound, attrs),
            NotificationChannel(RING_SILENT, "Reminders (quiet)", NotificationManager.IMPORTANCE_LOW).apply {
                description = "Reminders shown without a sound, because Dun is muted or your PC is ringing"
                setSound(null, null)
                enableVibration(false)
            },
            NotificationChannel(ERRORS, "Problems", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "Shown when Dun could not check your reminders"
                setSound(systemSound, attrs)
            },
            alarming(TIMER_DEFAULT, "Timers", systemAlarm, alarmAttrs),
            NotificationChannel(TIMERS_RUNNING, "Running timers", NotificationManager.IMPORTANCE_LOW).apply {
                description = "The countdown a running timer keeps on screen"
                setSound(null, null)
                enableVibration(false)
                setShowBadge(false)
            },
        )
        for ((id, label, res) in CHIMES) {
            val sound = rawUri(context, res)
            channels += ringing("ring_$id", "Reminders ($label)", sound, attrs)
            channels += alarming("timer_$id", "Timers ($label)", sound, alarmAttrs)
        }
        if (nm.isNotificationPolicyAccessGranted) {
            channels += alarming(TIMER_DEFAULT + DND_SUFFIX, "Timers", systemAlarm, alarmAttrs)
            for ((id, label, res) in CHIMES) {
                channels += alarming(
                    "timer_$id$DND_SUFFIX", "Timers ($label)", rawUri(context, res), alarmAttrs
                )
            }
        }
        nm.createNotificationChannels(channels)
    }

    /**
     * The channel to actually post on for the one Rust asked for.
     *
     * Rust knows nothing about Do Not Disturb access, so it names the plain
     * timer channel and this swaps in the bypassing twin when there is one.
     */
    fun effective(context: Context, channel: String): String {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O || !channel.startsWith("timer_")) {
            return channel
        }
        val nm = context.getSystemService(NotificationManager::class.java)
        val twin = channel + DND_SUFFIX
        return if (nm.getNotificationChannel(twin) != null) twin else channel
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

    private fun rawUri(context: Context, resId: Int): Uri =
        Uri.parse("android.resource://${context.packageName}/$resId")
}
