package app.dun.android

import android.content.Context

/**
 * Tiny health record kept outside the database so it survives Rust failures:
 * when the next alarm is due and when an alarm last actually fired. The setup
 * checklist shows it, which is how a user notices OEM battery killing.
 */
object AlarmLog {
    private const val PREFS = "dun_alarm_log"

    fun recordScheduled(context: Context, atMs: Long?) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putLong("scheduled_at", atMs ?: -1)
            .apply()
    }

    fun recordFired(context: Context, lateByMs: Long) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putLong("last_fired_at", System.currentTimeMillis())
            .putLong("last_late_by_ms", lateByMs)
            .apply()
    }

    fun scheduledAt(context: Context): Long =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getLong("scheduled_at", -1)

    fun lastFiredAt(context: Context): Long =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getLong("last_fired_at", -1)

    fun lastLateByMs(context: Context): Long =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getLong("last_late_by_ms", -1)
}
