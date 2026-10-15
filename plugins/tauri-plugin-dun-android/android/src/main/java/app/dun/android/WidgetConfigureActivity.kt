package app.dun.android

import android.app.Activity
import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Intent
import android.os.Bundle
import android.view.View
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.CheckBox
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.TextView

/** Each launcher instance owns an independent, durable item selection. */
class WidgetConfigureActivity : Activity() {
    private var widgetId = AppWidgetManager.INVALID_APPWIDGET_ID

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setResult(RESULT_CANCELED)
        widgetId = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        val info = AppWidgetManager.getInstance(this).getAppWidgetInfo(widgetId)
        if (info?.provider != ComponentName(this, CountdownWidget::class.java)) { finish(); return }
        title = "Countdown widget"
        setContentView(TextView(this).apply {
            text = "Loading timers, reminders and countdowns…"
            textSize = 18f
            setPadding(24, 48, 24, 24)
        })
        DunReceiver.EXECUTOR.execute {
            val refreshed = runCatching { CountdownWidget.refresh(applicationContext) }.getOrDefault(false)
            runOnUiThread { if (!isFinishing && !isDestroyed) showChoices(!refreshed) }
        }
    }

    private fun showChoices(refreshFailed: Boolean) {
        val pad = (16 * resources.displayMetrics.density).toInt()
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            fitsSystemWindows = true
            setPadding(pad, pad, pad, pad)
        }
        root.addView(TextView(this).apply {
            text = "Choose what to count down"
            textSize = 24f
            setPadding(0, pad, 0, pad)
            accessibilityHeadingCompat()
        })
        root.addView(TextView(this).apply {
            text = if (refreshFailed) "Couldn’t refresh. Showing saved items; open Dun to update them."
                else "Next due follows timers, reminders, snoozes and upcoming countdowns. Resize taller for Done and +5m Snooze on reminders and timers."
            textSize = 16f
            setPadding(0, 0, 0, pad)
        })
        val ignoreRepeating = CheckBox(this).apply {
            text = "Ignore repeating reminders"
            contentDescription = "Ignore repeating reminders in Next due"
            isChecked = CountdownWidget.prefs(this@WidgetConfigureActivity).getBoolean("ignoreRepeating:$widgetId", false)
            minHeight = (48 * resources.displayMetrics.density).toInt()
        }
        root.addView(ignoreRepeating)
        root.addView(TextView(this).apply {
            text = "Applies to Next due. You can still pin a repeating reminder."
            setPadding(0, 0, 0, pad)
        })
        val items = CountdownWidget.items(this)
        val ids = listOf("") + items.map { it.getString("id") }
        val labels = listOf("Next due") + items.map {
            val kind = when {
                it.optJSONObject("countdown") != null -> "Countdown"
                it.optBoolean("timer") -> "Timer"
                it.optBoolean("repeatingReminder") -> "Repeating reminder"
                else -> "Reminder"
            }
            "${it.getString("title")} · $kind"
        }
        var selected = ids.indexOf(CountdownWidget.prefs(this).getString("item:$widgetId", ""))
        val list = ListView(this).apply {
            choiceMode = ListView.CHOICE_MODE_SINGLE
            adapter = ArrayAdapter(this@WidgetConfigureActivity, android.R.layout.simple_list_item_single_choice, labels)
            if (selected >= 0) setItemChecked(selected, true)
        }
        root.addView(list, LinearLayout.LayoutParams(-1, 0, 1f))
        val save = Button(this).apply {
            text = "Save widget"
            isEnabled = selected >= 0
            setOnClickListener {
                CountdownWidget.prefs(this@WidgetConfigureActivity).edit()
                    .putString("item:$widgetId", ids[selected])
                    .putBoolean("ignoreRepeating:$widgetId", ignoreRepeating.isChecked).apply()
                CountdownWidget.render(this@WidgetConfigureActivity, widgetId)
                setResult(RESULT_OK, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId))
                finish()
            }
        }
        list.setOnItemClickListener { _, _, position, _ -> selected = position; save.isEnabled = true }
        root.addView(save, LinearLayout.LayoutParams(-1, -2))
        setContentView(root)
    }

    private fun View.accessibilityHeadingCompat() {
        if (android.os.Build.VERSION.SDK_INT >= 28) isAccessibilityHeading = true
    }
}
