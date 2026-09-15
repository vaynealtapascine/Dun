package app.dun.android

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import org.json.JSONObject

/**
 * Reboot, app update, clock and time-zone changes all invalidate the pending
 * alarm or the notion of "now", so each one asks Rust to re-evaluate and
 * re-arm. Overdue items ring immediately from here after a reboot.
 */
class SystemEventReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val reason = when (intent.action) {
            Intent.ACTION_BOOT_COMPLETED -> "boot"
            Intent.ACTION_MY_PACKAGE_REPLACED -> "updated"
            Intent.ACTION_TIME_CHANGED -> "time_set"
            Intent.ACTION_TIMEZONE_CHANGED -> "timezone"
            "android.app.action.SCHEDULE_EXACT_ALARM_PERMISSION_STATE_CHANGED" -> "exact_alarm_permission"
            "app.dun.RESCHEDULE" -> "manual"
            else -> return
        }
        val pending = goAsync()
        val app = context.applicationContext
        DunReceiver.EXECUTOR.execute {
            try {
                val event = JSONObject().put("type", "reschedule").put("reason", reason)
                PlanExecutor.applyResponse(app, DunNative.dispatch(app, event))
                Log.i(TAG, "rescheduled after $reason")
            } catch (t: Throwable) {
                Log.e(TAG, "reschedule failed", t)
                PlanExecutor.failLoud(app, t.toString())
            } finally {
                pending.finish()
            }
        }
    }

    companion object {
        private const val TAG = "DunSystemEvent"
    }
}
