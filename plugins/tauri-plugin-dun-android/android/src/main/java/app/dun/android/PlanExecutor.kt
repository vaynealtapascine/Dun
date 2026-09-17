package app.dun.android

import android.app.AlarmManager
import android.app.Notification
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import org.json.JSONArray
import org.json.JSONObject

/**
 * Carries out a plan computed by Rust. Kotlin makes no scheduling decisions of
 * its own: it posts and cancels exactly what the plan says and keeps a single
 * alarm for `nextWakeAt`.
 */
object PlanExecutor {
    private const val TAG = "DunPlan"
    private const val ALARM_REQUEST_CODE = 0
    private const val ERROR_NOTIFICATION_ID = Int.MAX_VALUE
    private const val ERROR_RETRY_MS = 60_000L

    const val ACTION_ALARM = "app.dun.ALARM"
    const val ACTION_BUTTON = "app.dun.BUTTON"
    const val ACTION_DISMISSED = "app.dun.DISMISSED"
    const val EXTRA_ITEM = "item"
    const val EXTRA_OCC = "occ"
    const val EXTRA_BUTTON = "button"

    private val BUTTON_LABELS = linkedMapOf("done" to "Done", "snooze5" to "+5m", "snooze15" to "+15m")

    /** Applies a `handleEvent` response. Falls back loud when Rust reported an error. */
    fun applyResponse(context: Context, response: JSONObject) {
        if (!response.optBoolean("ok", false)) {
            failLoud(context, response.optString("error", "unknown error"))
            return
        }
        NotificationManagerCompat.from(context).cancel(ERROR_NOTIFICATION_ID)
        apply(context, response.getJSONObject("plan"))
    }

    /**
     * [fromWorker] is set by [PendingPushWorker], which decides its own fate by
     * returning retry or success — cancelling its own job from in here would
     * interrupt it mid-run.
     */
    @JvmOverloads
    fun apply(context: Context, plan: JSONObject, fromWorker: Boolean = false) {
        Channels.ensure(context)
        val nm = NotificationManagerCompat.from(context)

        plan.optJSONArray("cancel")?.let { ids ->
            for (i in 0 until ids.length()) nm.cancel(ids.getInt(i))
        }

        plan.optJSONArray("post")?.let { posts ->
            for (i in 0 until posts.length()) post(context, nm, posts.getJSONObject(i))
        }

        if (plan.isNull("nextWakeAt")) {
            cancelAlarm(context)
        } else {
            setAlarm(context, plan.getLong("nextWakeAt"))
        }

        // Slow catch-up for changes made on the PC while nothing here is due.
        PendingPushWorker.ensurePeriodic(context)

        // Changes the PC hasn't acknowledged need a way through that doesn't
        // depend on something else being due.
        if (!fromWorker) {
            if (plan.optBoolean("pendingPush", false)) {
                PendingPushWorker.ensure(context)
            } else {
                PendingPushWorker.cancel(context)
            }
        }

        plan.optString("log").takeIf { it.isNotEmpty() }?.let { Log.i(TAG, it) }
    }

    private fun post(context: Context, nm: NotificationManagerCompat, p: JSONObject) {
        val notifId = p.getInt("notifId")
        val item = p.getString("itemId")
        val occ = p.getLong("occ")

        val channel = Channels.effective(context, p.optString("channel", Channels.RING_DEFAULT))
        val ongoing = p.optBoolean("ongoing", false)
        val builder = NotificationCompat.Builder(context, channel)
            .setSmallIcon(R.drawable.ic_stat_dun)
            .setContentTitle(p.getString("title"))
            .setContentText(p.optString("text"))
            // A timer is an alarm as far as the system is concerned: that is
            // what keeps it audible on silent and through Do Not Disturb.
            .setCategory(
                when {
                    channel == Channels.TIMERS_RUNNING -> NotificationCompat.CATEGORY_PROGRESS
                    channel.startsWith("timer_") -> NotificationCompat.CATEGORY_ALARM
                    else -> NotificationCompat.CATEGORY_REMINDER
                }
            )
            .setPriority(if (ongoing) NotificationCompat.PRIORITY_LOW else NotificationCompat.PRIORITY_HIGH)
            .setAutoCancel(false)
            .setOnlyAlertOnce(ongoing)
            .setOngoing(ongoing)
            .setSilent(p.optBoolean("silent", false))
            .setContentIntent(launchIntent(context, notifId))

        // A countdown is not something to dismiss, so it gets no delete intent
        // that would tell Rust the user swiped an alert away.
        if (!ongoing) {
            builder.setDeleteIntent(broadcast(context, ACTION_DISMISSED, notifId * 8 + 7, item, occ, null))
        }

        // Android ticks a chronometer by itself, so a running timer stays
        // honest on screen without Dun waking up once a second to redraw it.
        if (!p.isNull("countdownTo")) {
            builder.setWhen(p.getLong("countdownTo"))
                .setUsesChronometer(true)
                .setChronometerCountDown(true)
                .setShowWhen(true)
        } else if (!p.isNull("when")) {
            builder.setWhen(p.getLong("when")).setShowWhen(true)
        }

        val notes = p.optString("notes")
        if (notes.isNotEmpty() && notes != "null") {
            builder.setStyle(NotificationCompat.BigTextStyle().bigText("${p.optString("text")}\n$notes"))
        }

        val actions: JSONArray = p.optJSONArray("actions") ?: JSONArray()
        for (i in 0 until actions.length()) {
            val key = actions.getString(i)
            val label = BUTTON_LABELS[key] ?: continue
            val pi = broadcast(context, ACTION_BUTTON, notifId * 8 + i, item, occ, key)
            builder.addAction(0, label, pi)
        }

        try {
            nm.notify(notifId, builder.build())
        } catch (e: SecurityException) {
            Log.w(TAG, "notification permission missing; cannot post $notifId", e)
        }
    }

    private fun broadcast(
        context: Context, action: String, requestCode: Int, item: String, occ: Long, button: String?
    ): PendingIntent {
        val intent = Intent(context, DunReceiver::class.java).apply {
            this.action = action
            putExtra(EXTRA_ITEM, item)
            putExtra(EXTRA_OCC, occ)
            if (button != null) putExtra(EXTRA_BUTTON, button)
        }
        return PendingIntent.getBroadcast(
            context, requestCode, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
    }

    private fun launchIntent(context: Context, requestCode: Int): PendingIntent? {
        val intent = context.packageManager.getLaunchIntentForPackage(context.packageName) ?: return null
        intent.flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP
        return PendingIntent.getActivity(
            context, requestCode, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
    }

    private fun alarmIntent(context: Context, flags: Int): PendingIntent? {
        val intent = Intent(context, DunReceiver::class.java).setAction(ACTION_ALARM)
        return PendingIntent.getBroadcast(context, ALARM_REQUEST_CODE, intent, flags or PendingIntent.FLAG_IMMUTABLE)
    }

    fun setAlarm(context: Context, atMs: Long) {
        val am = context.getSystemService(AlarmManager::class.java)
        val operation = alarmIntent(context, PendingIntent.FLAG_UPDATE_CURRENT)!!
        val canExact = Build.VERSION.SDK_INT < Build.VERSION_CODES.S || am.canScheduleExactAlarms()
        try {
            if (canExact) {
                // setAlarmClock is exempt from Doze and wakes the device just before,
                // which is what lets a nag fire every minute with the screen off.
                val show = launchIntent(context, ALARM_REQUEST_CODE + 1)
                am.setAlarmClock(AlarmManager.AlarmClockInfo(atMs, show), operation)
            } else {
                Log.w(TAG, "exact alarms not permitted; nags may be delayed in Doze")
                am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, atMs, operation)
            }
        } catch (e: SecurityException) {
            Log.e(TAG, "cannot schedule alarm", e)
            am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, atMs, operation)
        }
        AlarmLog.recordScheduled(context, atMs)
    }

    fun cancelAlarm(context: Context) {
        val am = context.getSystemService(AlarmManager::class.java)
        alarmIntent(context, PendingIntent.FLAG_NO_CREATE)?.let {
            am.cancel(it)
            it.cancel()
        }
        AlarmLog.recordScheduled(context, null)
    }

    /** Rust could not produce a plan. Tell the user and try again in a minute. */
    fun failLoud(context: Context, error: String) {
        Log.e(TAG, "handler failed: $error")
        Channels.ensure(context)
        val n: Notification = NotificationCompat.Builder(context, Channels.ERRORS)
            .setSmallIcon(R.drawable.ic_stat_dun)
            .setContentTitle("Dun: check your reminders")
            .setContentText("Dun hit a problem working out what's due. Retrying every minute.")
            .setStyle(NotificationCompat.BigTextStyle().bigText("Dun hit a problem working out what's due. Retrying every minute.\n\n$error"))
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setCategory(NotificationCompat.CATEGORY_ERROR)
            .setContentIntent(launchIntent(context, ERROR_NOTIFICATION_ID - 1))
            .build()
        try {
            NotificationManagerCompat.from(context).notify(ERROR_NOTIFICATION_ID, n)
        } catch (e: SecurityException) {
            Log.w(TAG, "notification permission missing; cannot report failure", e)
        }
        setAlarm(context, System.currentTimeMillis() + ERROR_RETRY_MS)
    }
}
