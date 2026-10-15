package app.dun.android

import android.appwidget.AppWidgetManager
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.Bundle
import android.view.View
import android.widget.Chronometer
import android.widget.TextView
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.GregorianCalendar
import java.util.TimeZone

@RunWith(AndroidJUnit4::class)
class CountdownWidgetTest {
    @Test fun countdownDatesHandleRepeatsDirectionsAndDaylightSaving() {
        val utc = TimeZone.getTimeZone("UTC")
        fun instant(year: Int, month: Int, day: Int, hour: Int = 0, minute: Int = 0): Long =
            GregorianCalendar(utc).apply { clear(); set(year, month - 1, day, hour, minute) }.timeInMillis
        val monthly = JSONObject().put("date", "2026-01-31").put("repeat", "monthly").put("direction", "auto")
        assertEquals(instant(2026, 2, 28), WidgetCountdown.reading(monthly, instant(2026, 2, 1), utc).at)
        assertEquals(instant(2026, 3, 31), WidgetCountdown.reading(monthly, instant(2026, 3, 1), utc).at)
        monthly.put("direction", "since")
        assertEquals(instant(2026, 2, 28), WidgetCountdown.reading(monthly, instant(2026, 3, 1), utc).at)
        assertTrue(WidgetCountdown.reading(monthly, instant(2026, 3, 1), utc).since)
        val yearly = JSONObject().put("date", "2024-02-29").put("repeat", "yearly")
        assertEquals(instant(2027, 2, 28), WidgetCountdown.reading(yearly, instant(2026, 3, 1), utc).at)
        assertEquals(instant(2028, 2, 29), WidgetCountdown.reading(yearly, instant(2028, 1, 1), utc).at)
        val once = JSONObject().put("date", "2026-01-01").put("repeat", "none")
        assertTrue(WidgetCountdown.reading(once, instant(2026, 2, 1), utc).since)
        once.put("direction", "since")
        assertFalse(WidgetCountdown.reading(once, instant(2025, 12, 1), utc).since)
        val ny = TimeZone.getTimeZone("America/New_York")
        val fold = JSONObject().put("date", "2026-11-01").put("time", "01:30:00")
        assertEquals(instant(2026, 11, 1, 5, 30), WidgetCountdown.reading(fold, instant(2026, 10, 1), ny).at)
        val gap = JSONObject().put("date", "2026-03-08").put("time", "02:30:00")
        assertEquals(instant(2026, 3, 8, 7, 30), WidgetCountdown.reading(gap, instant(2026, 3, 1), ny).at)
        gap.put("repeat", "monthly")
        assertEquals(instant(2026, 4, 8, 6, 30), WidgetCountdown.reading(gap, instant(2026, 4, 1), ny).at)
        val boundary = instant(2027, 2, 28)
        assertEquals(boundary + 1000, WidgetCountdown.reading(yearly, boundary, utc).nextChange)
    }

    @Test fun filterAndCountdownBannersWorkWithoutAlarmActions() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        instrumentation.runOnMainSync {
            val base = instrumentation.targetContext
            val now = System.currentTimeMillis()
            val repeating = JSONObject().put("id", "repeat").put("title", "Daily reminder")
                .put("at", now + 1000).put("repeatingReminder", true).put("state", "upcoming")
            val timer = JSONObject().put("id", "timer").put("title", "Tea")
                .put("at", now + 600000).put("timer", true).put("state", "upcoming")
            val spec = JSONObject().put("date", "2090-01-01").put("repeat", "yearly").put("color", "#287d87")
            val countdown = JSONObject().put("id", "countdown:birthday").put("title", "Birthday").put("countdown", spec)
            val options = Bundle().apply {
                putInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH, 260)
                putInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT, 160)
            }
            val all = listOf(repeating, timer, countdown)
            fun title(ignore: Boolean, pin: String) = CountdownWidget.views(base, 42, options, all, pin, ignore)
                .apply(base, null).findViewById<TextView>(R.id.widget_title).text.toString()
            assertEquals("Daily reminder", title(false, ""))
            assertEquals("Tea", title(true, ""))
            assertEquals("Daily reminder", title(true, "repeat"))
            val nextCountdown = CountdownWidget.views(base, 42, options, listOf(countdown), "", true).apply(base, null)
            assertEquals("Birthday", nextCountdown.findViewById<TextView>(R.id.widget_title).text.toString())
            spec.put("date", "2000-01-01").put("repeat", "none")
            val past = CountdownWidget.views(base, 42, options, listOf(countdown), "", false).apply(base, null)
            assertEquals("Next due · Nothing scheduled", past.findViewById<TextView>(R.id.widget_status).text.toString())
            val pinnedPast = CountdownWidget.views(base, 42, options, listOf(countdown), "countdown:birthday", true).apply(base, null)
            assertFalse(pinnedPast.findViewById<Chronometer>(R.id.widget_clock).isCountDown)
            spec.put("date", "2090-01-01")
            for (night in listOf(false, true)) {
                val config = Configuration(base.resources.configuration).apply {
                    uiMode = (uiMode and Configuration.UI_MODE_NIGHT_MASK.inv()) or
                        if (night) Configuration.UI_MODE_NIGHT_YES else Configuration.UI_MODE_NIGHT_NO
                }
                val context = base.createConfigurationContext(config)
                for (fill in listOf("solid", "soft", "outline")) {
                    spec.put("fill", fill)
                    val view = CountdownWidget.views(context, 42, options, all, "countdown:birthday", true).apply(context, null)
                    assertEquals(View.GONE, view.findViewById<View>(R.id.widget_actions).visibility)
                    if (fill == "solid") assertEquals(android.graphics.Color.WHITE, view.findViewById<TextView>(R.id.widget_title).currentTextColor)
                    val width = (260 * context.resources.displayMetrics.density).toInt()
                    val height = (160 * context.resources.displayMetrics.density).toInt()
                    view.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY))
                    view.layout(0, 0, width, height)
                    val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
                    view.draw(Canvas(bitmap))
                    File(base.getExternalFilesDir(null), "banner-${if (night) "dark" else "light"}-$fill.png").outputStream()
                        .use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
                    bitmap.recycle()
                }
            }
            // Missing colors and short CSS hex colors remain safe to render.
            for (color in listOf("#fed", "invalid")) {
                spec.put("color", color).put("fill", "solid")
                CountdownWidget.views(base, 42, options, all, "countdown:birthday").apply(base, null)
            }
            CountdownWidget.prefs(base).edit().putString("item:42", "repeat").putBoolean("ignoreRepeating:42", true).commit()
            CountdownWidget().onRestored(base, intArrayOf(42), intArrayOf(43))
            assertTrue(CountdownWidget.prefs(base).getBoolean("ignoreRepeating:43", false))
            assertEquals("repeat", CountdownWidget.prefs(base).getString("item:43", ""))
            CountdownWidget().onDeleted(base, intArrayOf(43))
            assertFalse(CountdownWidget.prefs(base).contains("ignoreRepeating:43"))
        }
    }

    @Test fun remoteViewsRenderBothSizesAndRespectSelectionAndStates() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        instrumentation.runOnMainSync {
            val base = instrumentation.targetContext
            val now = System.currentTimeMillis()
            fun item(id: String, state: String, at: Long?) = JSONObject()
                .put("id", id).put("title", if (id == "tea") "Tea timer" else "Take medication")
                .put("timer", id == "tea").put("state", state)
                .put("at", at ?: JSONObject.NULL).put("remainingMs", if (state == "paused") 125000 else JSONObject.NULL)
                .put("occurrence", if (state == "snoozed" || state == "due") now - 1000 else JSONObject.NULL)
            for (night in listOf(false, true)) {
                val config = Configuration(base.resources.configuration).apply {
                    uiMode = (uiMode and Configuration.UI_MODE_NIGHT_MASK.inv()) or
                        if (night) Configuration.UI_MODE_NIGHT_YES else Configuration.UI_MODE_NIGHT_NO
                }
                val context = base.createConfigurationContext(config)
                for (expanded in listOf(false, true)) {
                    val options = Bundle().apply { putInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT, if (expanded) 160 else 80) }
                    val all = listOf(item("tea", "upcoming", now + 600000), item("meds", "snoozed", now + 300000))
                    fun render(pinned: String, data: List<JSONObject> = all): View =
                        CountdownWidget.views(context, 42, options, data, pinned).apply(context, null)
                    val next = render("")
                    assertEquals("Take medication", next.findViewById<TextView>(R.id.widget_title).text.toString())
                    assertTrue(next.findViewById<Chronometer>(R.id.widget_clock).isCountDown)
                    assertEquals(if (expanded) View.VISIBLE else View.GONE, next.findViewById<View>(R.id.widget_actions).visibility)
                    assertTrue(next.findViewById<View>(R.id.widget_done).isEnabled)
                    val pinned = render("tea")
                    assertEquals("Tea timer", pinned.findViewById<TextView>(R.id.widget_title).text.toString())
                    assertFalse(pinned.findViewById<View>(R.id.widget_done).isEnabled)
                    val paused = render("tea", listOf(item("tea", "paused", null)))
                    assertEquals("2:05", paused.findViewById<TextView>(R.id.widget_static).text.toString())
                    val missing = render("removed")
                    assertEquals("Pinned · Item unavailable", missing.findViewById<TextView>(R.id.widget_status).text.toString())
                    assertEquals(View.GONE, missing.findViewById<View>(R.id.widget_clock).visibility)
                    val due = render("tea", listOf(item("tea", "due", now - 10000)))
                    assertFalse(due.findViewById<Chronometer>(R.id.widget_clock).isCountDown)
                    val empty = render("", emptyList())
                    assertEquals("Next due · Nothing scheduled", empty.findViewById<TextView>(R.id.widget_status).text.toString())
                    val width = (260 * context.resources.displayMetrics.density).toInt()
                    val height = ((if (expanded) 160 else 80) * context.resources.displayMetrics.density).toInt()
                    next.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY))
                    next.layout(0, 0, width, height)
                    val status = next.findViewById<TextView>(R.id.widget_status)
                    assertTrue("Status must fit inside the content area", status.bottom <= (status.parent as View).height)
                    val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
                    next.draw(Canvas(bitmap))
                    File(base.getExternalFilesDir(null), "widget-${if (night) "dark" else "light"}-${if (expanded) "3x2" else "3x1"}.png")
                        .outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
                    bitmap.recycle()
                }
            }
        }
    }
}
