package app.dun.android

import android.Manifest
import android.app.Activity
import android.app.AlarmManager
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import android.content.res.Configuration
import android.view.View
import android.webkit.WebView
import androidx.core.app.ActivityCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONObject
import java.util.TimeZone

@TauriPlugin
class DunPlugin(private val activity: Activity) : Plugin(activity) {

    override fun load(webView: WebView) {
        Channels.ensure(activity)
        fitSystemBars(webView)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(activity, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            ActivityCompat.requestPermissions(activity, arrayOf(Manifest.permission.POST_NOTIFICATIONS), 7001)
        }
    }

    /**
     * Android draws the app edge to edge, so pad the web view by the status
     * bar, navigation bar, any display cutout and the keyboard. Without this
     * the first row of the UI sits under the clock.
     */
    private fun fitSystemBars(webView: WebView) {
        val night = activity.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK ==
            Configuration.UI_MODE_NIGHT_YES
        // Matches the page background so the padded strip isn't a white band.
        webView.setBackgroundColor(if (night) 0xFF141416.toInt() else 0xFFF4F4F2.toInt())
        // The listener goes on the activity's content view: Tauri's own layout
        // sits between it and the web view and would otherwise swallow them.
        val content: View = activity.findViewById(android.R.id.content)
        ViewCompat.setOnApplyWindowInsetsListener(content) { view: View, insets: WindowInsetsCompat ->
            val bars = insets.getInsets(
                WindowInsetsCompat.Type.systemBars() or
                    WindowInsetsCompat.Type.displayCutout() or
                    WindowInsetsCompat.Type.ime()
            )
            view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            WindowInsetsCompat.CONSUMED
        }
        ViewCompat.requestApplyInsets(content)
    }

    /** In-app path: Rust already computed the plan; carry it out like a receiver would. */
    @Command
    fun applyPlan(invoke: Invoke) {
        try {
            PlanExecutor.apply(activity.applicationContext, JSONObject(invoke.getRawArgs()))
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("applyPlan failed: $e")
        }
    }

    @Command
    fun deviceInfo(invoke: Invoke) {
        val ret = JSObject()
        ret.put("dataDir", activity.applicationContext.dataDir.absolutePath)
        ret.put("tzId", TimeZone.getDefault().id)
        ret.put("sdkInt", Build.VERSION.SDK_INT)
        ret.put("manufacturer", Build.MANUFACTURER)
        ret.put("model", Build.MODEL)
        invoke.resolve(ret)
    }

    @Command
    fun setupStatus(invoke: Invoke) {
        val ctx = activity.applicationContext
        val am = ctx.getSystemService(AlarmManager::class.java)
        val pm = ctx.getSystemService(PowerManager::class.java)
        val ret = JSObject()
        ret.put("notificationsEnabled", NotificationManagerCompat.from(ctx).areNotificationsEnabled())
        ret.put(
            "exactAlarms",
            Build.VERSION.SDK_INT < Build.VERSION_CODES.S || am.canScheduleExactAlarms()
        )
        ret.put("ignoringBatteryOptimizations", pm.isIgnoringBatteryOptimizations(ctx.packageName))
        ret.put("scheduledAt", AlarmLog.scheduledAt(ctx))
        ret.put("lastFiredAt", AlarmLog.lastFiredAt(ctx))
        ret.put("lastLateByMs", AlarmLog.lastLateByMs(ctx))
        ret.put("manufacturer", Build.MANUFACTURER)
        invoke.resolve(ret)
    }

    @Command
    fun openSetting(invoke: Invoke) {
        val key = JSONObject(invoke.getRawArgs()).optString("key")
        val pkg = Uri.parse("package:${activity.packageName}")
        val intent = when (key) {
            "notifications" -> Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                .putExtra(Settings.EXTRA_APP_PACKAGE, activity.packageName)
            "exactAlarms" -> if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S)
                Intent(Settings.ACTION_REQUEST_SCHEDULE_EXACT_ALARM, pkg) else null
            "battery" -> Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, pkg)
            "appDetails" -> Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, pkg)
            else -> null
        }
        if (intent == null) {
            invoke.reject("unknown setting '$key'")
            return
        }
        try {
            activity.startActivity(intent)
            invoke.resolve()
        } catch (e: Exception) {
            activity.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, pkg))
            invoke.resolve()
        }
    }
}
