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
 * Channel ids and names must match what Rust puts in a plan (`ring_<chime>`).
 */
object Channels {
    /** Falls back to the system notification sound. */
    const val RING_DEFAULT = "ring_default"

    /** Used for alerts the scheduler marked silent (muted, or the PC has it). */
    const val RING_SILENT = "ring_silent"

    /** Fallback when Rust fails: always audible so a failure is never silent. */
    const val ERRORS = "errors"

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
        val systemSound = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_NOTIFICATION)

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
        )
        for ((id, label, res) in CHIMES) {
            channels += ringing("ring_$id", "Reminders ($label)", rawUri(context, res), attrs)
        }
        nm.createNotificationChannels(channels)
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
