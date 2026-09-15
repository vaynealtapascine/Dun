package app.dun.android

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.media.AudioAttributes
import android.media.RingtoneManager
import android.os.Build

object Channels {
    /** Rings with the system notification sound. Chime-specific channels come later. */
    const val RING_DEFAULT = "ring_default"

    /** Fallback when Rust fails: always audible so a failure is never silent. */
    const val ERRORS = "errors"

    fun ensure(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val nm = context.getSystemService(NotificationManager::class.java)
        val attrs = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_NOTIFICATION_EVENT)
            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
            .build()
        val sound = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_NOTIFICATION)

        val ring = NotificationChannel(RING_DEFAULT, "Reminders", NotificationManager.IMPORTANCE_HIGH).apply {
            description = "Rings and nags for due reminders and timers"
            enableVibration(true)
            setSound(sound, attrs)
        }
        val errors = NotificationChannel(ERRORS, "Problems", NotificationManager.IMPORTANCE_HIGH).apply {
            description = "Shown when Dun could not check your reminders"
            setSound(sound, attrs)
        }
        nm.createNotificationChannels(listOf(ring, errors))
    }
}
