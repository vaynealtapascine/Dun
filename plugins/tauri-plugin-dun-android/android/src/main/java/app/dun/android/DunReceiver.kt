package app.dun.android

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.SystemClock
import android.util.Log
import org.json.JSONObject
import java.util.concurrent.Executors

/**
 * Handles our own PendingIntents: the single wake alarm, notification buttons
 * and swipe-away. Work runs off the main thread inside goAsync(), which gives
 * us under 10 s; Rust keeps its own budget well below that.
 */
class DunReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val event = toEvent(context, intent) ?: return
        val pending = goAsync()
        val app = context.applicationContext
        val received = SystemClock.elapsedRealtime()
        EXECUTOR.execute {
            try {
                val response = DunNative.dispatch(app, event)
                PlanExecutor.applyResponse(app, response)
                Log.i(
                    TAG,
                    "${event.optString("type")} handled in ${SystemClock.elapsedRealtime() - received} ms " +
                        "(rust ${response.optLong("handlerMs", -1)} ms, lib load ${DunNative.loadMs} ms)"
                )
            } catch (t: Throwable) {
                Log.e(TAG, "receiver failed", t)
                PlanExecutor.failLoud(app, t.toString())
            } finally {
                pending.finish()
            }
        }
    }

    private fun toEvent(context: Context, intent: Intent): JSONObject? = when (intent.action) {
        PlanExecutor.ACTION_ALARM -> {
            val scheduled = AlarmLog.scheduledAt(context)
            val lateBy = if (scheduled > 0) System.currentTimeMillis() - scheduled else -1
            AlarmLog.recordFired(context, lateBy)
            JSONObject().put("type", "alarm").put("lateByMs", lateBy)
        }
        PlanExecutor.ACTION_BUTTON -> JSONObject()
            .put("type", "action")
            .put("itemId", intent.getStringExtra(PlanExecutor.EXTRA_ITEM))
            .put("occ", intent.getLongExtra(PlanExecutor.EXTRA_OCC, 0))
            .put("button", intent.getStringExtra(PlanExecutor.EXTRA_BUTTON))
        PlanExecutor.ACTION_DISMISSED -> JSONObject()
            .put("type", "dismissed")
            .put("itemId", intent.getStringExtra(PlanExecutor.EXTRA_ITEM))
            .put("occ", intent.getLongExtra(PlanExecutor.EXTRA_OCC, 0))
        else -> {
            Log.w(TAG, "ignoring unexpected action ${intent.action}")
            null
        }
    }

    companion object {
        private const val TAG = "DunReceiver"

        /** One thread: events are applied strictly in arrival order. */
        internal val EXECUTOR = Executors.newSingleThreadExecutor()
    }
}
