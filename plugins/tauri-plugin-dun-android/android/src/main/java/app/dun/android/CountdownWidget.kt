package app.dun.android

import android.app.PendingIntent
import android.app.AlarmManager
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.SystemClock
import android.util.Log
import android.view.View
import android.widget.RemoteViews
import org.json.JSONArray
import org.json.JSONObject

/** Launcher-owned chronometers tick without a service or per-second alarms. */
class CountdownWidget : AppWidgetProvider() {
    override fun onReceive(context: Context, intent: Intent) {
        super.onReceive(context, intent)
        if (intent.action == ACTION_REFRESH) {
            val manager = AppWidgetManager.getInstance(context)
            manager.getAppWidgetIds(ComponentName(context, CountdownWidget::class.java)).forEach { render(context, it) }
        }
        if (intent.action == AppWidgetManager.ACTION_APPWIDGET_UPDATE) {
            val pending = goAsync()
            DunReceiver.EXECUTOR.execute {
                try { refresh(context.applicationContext) }
                catch (e: Exception) { Log.w("DunWidget", "refresh failed", e) }
                finally { pending.finish() }
            }
        }
    }

    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        ids.forEach { render(context, it) }
    }

    override fun onAppWidgetOptionsChanged(context: Context, manager: AppWidgetManager, id: Int, options: Bundle) {
        render(context, id)
    }

    override fun onDeleted(context: Context, ids: IntArray) {
        prefs(context).edit().apply { ids.forEach { remove("item:$it"); remove("ignoreRepeating:$it") } }.apply()
        scheduleRefresh(context)
    }

    override fun onRestored(context: Context, oldIds: IntArray, newIds: IntArray) {
        val prefs = prefs(context)
        prefs.edit().apply {
            oldIds.zip(newIds).forEach { (old, new) ->
                putString("item:$new", prefs.getString("item:$old", ""))
                putBoolean("ignoreRepeating:$new", prefs.getBoolean("ignoreRepeating:$old", false))
                remove("item:$old")
                remove("ignoreRepeating:$old")
            }
        }.apply()
        newIds.forEach { render(context, it) }
    }

    companion object {
        private const val ACTION_REFRESH = "app.dun.WIDGET_REFRESH"
        private const val FLAGS = PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        fun prefs(context: Context) = context.getSharedPreferences("countdown_widgets", Context.MODE_PRIVATE)
        fun items(context: Context): List<JSONObject> {
            val array = JSONArray(prefs(context).getString("items", "[]"))
            return (0 until array.length()).map { array.getJSONObject(it) }
        }

        fun refresh(context: Context): Boolean {
            val response = DunNative.dispatch(context, JSONObject().put("type", "reschedule").put("reason", "widget"))
            if (!response.optBoolean("ok")) return false
            PlanExecutor.applyResponse(context, response)
            return true
        }

        fun accept(context: Context, plan: JSONObject) {
            val items = plan.optJSONArray("widgetItems") ?: return
            prefs(context).edit().putString("items", items.toString()).apply()
            val manager = AppWidgetManager.getInstance(context)
            manager.getAppWidgetIds(ComponentName(context, CountdownWidget::class.java)).forEach { render(context, it) }
        }

        fun render(context: Context, id: Int) {
            val manager = AppWidgetManager.getInstance(context)
            val pinned = prefs(context).getString("item:$id", "").orEmpty()
            val all = items(context)
            val ignoreRepeating = prefs(context).getBoolean("ignoreRepeating:$id", false)
            manager.updateAppWidget(id, views(context, id, manager.getAppWidgetOptions(id), all, pinned, ignoreRepeating))
            scheduleRefresh(context)
        }

        // Date rollovers are display-only: never create an alarm-clock indicator or wake the phone.
        private fun scheduleRefresh(context: Context) {
            val now = System.currentTimeMillis()
            val manager = AppWidgetManager.getInstance(context)
            val ids = manager.getAppWidgetIds(ComponentName(context, CountdownWidget::class.java))
            val operation = PendingIntent.getBroadcast(context, 0,
                Intent(context, CountdownWidget::class.java).setAction(ACTION_REFRESH), FLAGS)
            val alarm = context.getSystemService(AlarmManager::class.java)
            val next = if (ids.isEmpty()) null else items(context).mapNotNull { item ->
                item.optJSONObject("countdown")?.let { spec ->
                    runCatching { WidgetCountdown.reading(spec, now).nextChange }.getOrNull()
                }
            }.minOrNull()
            if (next == null) alarm.cancel(operation)
            else alarm.set(AlarmManager.RTC, next.coerceAtLeast(now + 1000) + 1000, operation)
        }

        internal fun views(context: Context, id: Int, options: Bundle, all: List<JSONObject>, pinned: String,
                           ignoreRepeating: Boolean = false): RemoteViews {
            val now = System.currentTimeMillis()
            val projected = all.map { WidgetCountdown.project(it, now) }
            val item = if (pinned.isEmpty()) projected.filter {
                !it.isNull("at") && (!ignoreRepeating || !it.optBoolean("repeatingReminder")) &&
                    (it.optJSONObject("countdown") == null || (it.optString("state") == "until" && it.getLong("at") >= now))
            }
                .minWithOrNull(compareBy<JSONObject> { it.getLong("at") }.thenBy { it.getString("id") })
                else projected.find { it.getString("id") == pinned }
            val expanded = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT) >= 128
            val views = RemoteViews(context.packageName, R.layout.widget_countdown)
            val at = item?.takeUnless { it.isNull("at") }?.getLong("at")
            val countdown = item?.optJSONObject("countdown")
            val state = item?.optString("state")
            val status = when {
                item == null -> if (pinned.isEmpty()) "Nothing scheduled" else "Item unavailable"
                countdown != null && at != null -> if (at <= now) "Countdown · Since" else "Countdown · Until"
                at != null && at <= now -> "Due"
                state == "snoozed" -> "Snoozed"
                state == "paused" -> "Paused"
                state == "ready" -> "Ready"
                state == "done" -> "Done"
                state == "unavailable" -> "Unavailable"
                item.optBoolean("timer") -> "Timer"
                else -> "Reminder"
            }
            views.setTextViewText(R.id.widget_title, item?.optString("title") ?: "Dun")
            views.setTextViewText(R.id.widget_status, "${if (pinned.isEmpty()) "Next due" else "Pinned"} · $status")
            views.setViewVisibility(R.id.widget_clock, if (at != null) View.VISIBLE else View.GONE)
            views.setViewVisibility(R.id.widget_static, if (at == null) View.VISIBLE else View.GONE)
            if (at != null) {
                views.setChronometer(R.id.widget_clock, SystemClock.elapsedRealtime() + at - now, null, true)
                // Once due, count elapsed time with an explicit label instead of a negative countdown.
                views.setChronometerCountDown(R.id.widget_clock, at > now)
                views.setContentDescription(R.id.widget_clock, if (at > now) "Time remaining" else if (countdown != null) "Time elapsed" else "Time overdue")
            } else {
                val remaining = item?.takeUnless { it.isNull("remainingMs") }?.getLong("remainingMs")
                views.setTextViewText(R.id.widget_static, remaining?.let { duration(it) } ?: "—")
            }
            context.packageManager.getLaunchIntentForPackage(context.packageName)?.let {
                views.setOnClickPendingIntent(R.id.widget_content, PendingIntent.getActivity(context, id, it, FLAGS))
            }
            val configure = Intent(context, WidgetConfigureActivity::class.java)
                .setData(Uri.parse("dun-widget://configure/$id"))
                .putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, id)
            views.setOnClickPendingIntent(R.id.widget_settings, PendingIntent.getActivity(context, id, configure, FLAGS))
            views.setViewVisibility(R.id.widget_actions, if (expanded && countdown == null) View.VISIBLE else View.GONE)
            val actionable = countdown == null && item != null && !item.isNull("occurrence")
            for ((view, button) in listOf(R.id.widget_done to "done", R.id.widget_snooze to "snooze5")) {
                views.setBoolean(view, "setEnabled", actionable)
                if (actionable) {
                    val action = Intent(context, DunReceiver::class.java).setAction(PlanExecutor.ACTION_BUTTON)
                        .setData(Uri.parse("dun-widget://action/$id/$button"))
                        .putExtra(PlanExecutor.EXTRA_ITEM, item!!.getString("id"))
                        .putExtra(PlanExecutor.EXTRA_OCC, item.getLong("occurrence"))
                        .putExtra(PlanExecutor.EXTRA_BUTTON, button)
                    views.setOnClickPendingIntent(view, PendingIntent.getBroadcast(context, id, action, FLAGS))
                }
            }
            if (countdown != null) WidgetCountdown.style(context, views, countdown,
                options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH, 260),
                options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT, 80))
            return views
        }

        private fun duration(ms: Long): String {
            val seconds = (ms.coerceAtLeast(0) + 999) / 1000
            return if (seconds >= 3600) "%d:%02d:%02d".format(seconds / 3600, seconds / 60 % 60, seconds % 60)
                else "%d:%02d".format(seconds / 60, seconds % 60)
        }
    }
}
