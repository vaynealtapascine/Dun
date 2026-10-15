package app.dun.android

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.widget.RemoteViews
import androidx.core.graphics.ColorUtils
import org.json.JSONObject
import java.util.Calendar
import java.util.GregorianCalendar
import java.util.TimeZone

/** Non-alerting civil dates; anchored month/year arithmetic matches the banner. */
internal object WidgetCountdown {
    data class Reading(val at: Long, val since: Boolean, val nextChange: Long?)

    fun reading(spec: JSONObject, now: Long, zone: TimeZone = TimeZone.getDefault()): Reading {
        val date = spec.getString("date").split('-').map { it.toInt() }
        val time = spec.optString("time", "").takeUnless { it.isEmpty() || it == "null" }
            ?.split(':')?.map { it.toDouble().toInt() } ?: listOf(0, 0, 0)
        val repeat = spec.optString("repeat", "none").takeIf { it in listOf("weekly", "monthly", "yearly") } ?: "none"
        fun occurrence(k: Int): Long {
            // Anchor to the original civil date/time, never a prior DST-normalized occurrence.
            val calendar = GregorianCalendar(zone).apply {
                clear()
                set(date[0], date[1] - 1, date[2], 12, 0, 0)
                when (repeat) {
                    "weekly" -> add(Calendar.DAY_OF_MONTH, 7 * k)
                    "monthly" -> add(Calendar.MONTH, k)
                    "yearly" -> add(Calendar.YEAR, k)
                }
                set(Calendar.HOUR_OF_DAY, time[0])
                set(Calendar.MINUTE, time[1])
                set(Calendar.SECOND, time.getOrElse(2) { 0 })
            }
            val instant = calendar.timeInMillis
            // Calendar chooses the later side of a DST fold; Dun uses the earlier one.
            val offsetBefore = zone.getOffset(instant - 6 * 3600000L)
            val candidate = instant - (offsetBefore - zone.getOffset(instant))
            val earlier = GregorianCalendar(zone).apply { timeInMillis = candidate }
            val fields = listOf(Calendar.YEAR, Calendar.DAY_OF_YEAR, Calendar.HOUR_OF_DAY, Calendar.MINUTE, Calendar.SECOND)
            return if (candidate < instant && fields.all { earlier.get(it) == calendar.get(it) }) candidate else instant
        }
        var last = -1
        if (now >= occurrence(0)) {
            val current = GregorianCalendar(zone).apply { timeInMillis = now }
            last = when (repeat) {
                "weekly" -> ((now - occurrence(0)) / (7 * 86400000L)).toInt()
                "monthly" -> (current.get(Calendar.YEAR) - date[0]) * 12 + current.get(Calendar.MONTH) - (date[1] - 1)
                "yearly" -> current.get(Calendar.YEAR) - date[0]
                else -> 0
            }.coerceAtLeast(0)
            if (repeat != "none") {
                while (occurrence(last + 1) <= now) last++
                while (last > 0 && occurrence(last) > now) last--
            }
        }
        val previous = if (last >= 0) occurrence(last) else null
        val next = when {
            previous == now -> previous
            repeat == "none" && last >= 0 -> null
            else -> occurrence(last + 1)
        }
        val since = when (spec.optString("direction", "auto")) {
            "since" -> previous != null
            else -> next == null
        }
        val boundary = when {
            previous == now -> now + 1000
            next != null && next > now -> next
            repeat != "none" -> occurrence(last + 1)
            else -> null
        }
        return Reading(if (since) previous!! else next!!, since, boundary)
    }

    fun project(item: JSONObject, now: Long): JSONObject {
        val spec = item.optJSONObject("countdown") ?: return item
        return JSONObject(item.toString()).apply {
            // Malformed synced metadata must not stop other widgets from rendering.
            val reading = runCatching { reading(spec, now) }.getOrNull()
            put("at", reading?.at ?: JSONObject.NULL)
            put("state", if (reading == null) "unavailable" else if (reading.since) "since" else "until")
        }
    }

    fun style(context: Context, views: RemoteViews, spec: JSONObject, widthDp: Int, heightDp: Int) {
        val surface = context.getColor(R.color.widget_surface)
        val normal = context.getColor(R.color.widget_ink)
        val accent = runCatching {
            val raw = spec.getString("color")
            // Android does not accept CSS's three-digit hex form.
            Color.parseColor(if (raw.matches(Regex("#[0-9a-fA-F]{3}"))) "#" + raw.drop(1).map { "$it$it" }.joinToString("") else raw)
        }.getOrDefault(context.getColor(R.color.widget_accent))
        val fill = spec.optString("fill", "solid")
        val solid = fill != "soft" && fill != "outline"
        val background = when (fill) {
            "soft" -> ColorUtils.blendARGB(surface, accent, 0.15f)
            "outline" -> surface
            else -> accent
        }
        val ink = if (solid) {
            if (ColorUtils.calculateLuminance(accent) > 0.4) Color.rgb(16, 32, 42) else Color.WHITE
        } else normal
        val numeric = if (solid) ink else ColorUtils.blendARGB(normal, accent, 0.78f)
        val density = context.resources.displayMetrics.density
        val width = (widthDp.coerceAtLeast(180) * density).toInt().coerceAtMost(1200)
        val height = (heightDp.coerceAtLeast(80) * density).toInt().coerceAtMost(1200)
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = background }
        val rect = RectF(0f, 0f, width.toFloat(), height.toFloat())
        canvas.drawRoundRect(rect, 16 * density, 16 * density, paint)
        if (!solid) {
            paint.color = if (fill == "outline") accent else ColorUtils.blendARGB(context.getColor(R.color.widget_border), accent, 0.35f)
            paint.style = Paint.Style.STROKE
            paint.strokeWidth = (if (fill == "outline") 2 else 1) * density
            rect.inset(paint.strokeWidth / 2, paint.strokeWidth / 2)
            canvas.drawRoundRect(rect, 16 * density, 16 * density, paint)
        }
        views.setImageViewBitmap(R.id.widget_background, bitmap)
        views.setTextColor(R.id.widget_title, ink)
        views.setTextColor(R.id.widget_status, ink)
        views.setTextColor(R.id.widget_clock, numeric)
        views.setTextColor(R.id.widget_static, numeric)
        val icon = context.getDrawable(R.drawable.ic_widget_settings)!!.mutate().apply { setTint(ink) }
        val iconSize = (24 * density).toInt()
        val iconBitmap = Bitmap.createBitmap(iconSize, iconSize, Bitmap.Config.ARGB_8888)
        icon.setBounds(0, 0, iconSize, iconSize)
        icon.draw(Canvas(iconBitmap))
        views.setImageViewBitmap(R.id.widget_settings, iconBitmap)
    }
}
