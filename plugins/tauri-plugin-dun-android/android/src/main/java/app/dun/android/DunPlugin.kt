package app.dun.android

import android.Manifest
import android.app.Activity
import android.app.AlarmManager
import android.app.NotificationManager
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import android.content.res.Configuration
import android.view.View
import android.webkit.WebView
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
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

    private var notificationPermission: ActivityResultLauncher<String>? = null
    private var askedForExactAlarms = false

    override fun load(webView: WebView) {
        Channels.ensure(activity)
        fitSystemBars(webView)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(activity, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            // Register without a LifecycleOwner because plugins load after STARTED.
            // The result callback also runs for denial or an already-denied prompt;
            // onPause/onResume cannot reliably tell whether a prompt was answered.
            notificationPermission = (activity as ComponentActivity).activityResultRegistry.register(
                "dun-notification-permission", ActivityResultContracts.RequestPermission()
            ) { requestExactAlarms() }
            notificationPermission?.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            requestExactAlarms()
        }
    }

    override fun onDestroy(activity: androidx.appcompat.app.AppCompatActivity) {
        notificationPermission?.unregister()
        notificationPermission = null
    }

    /**
     * "Alarms & reminders" is special access: there is no runtime prompt, only
     * a settings page with a switch. Without it alarms fall back to inexact
     * delivery and Doze can hold a timer for many minutes, so take the user
     * there once per launch until it's on.
     */
    private fun requestExactAlarms() {
        if (askedForExactAlarms || Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return
        askedForExactAlarms = true
        val am = activity.getSystemService(AlarmManager::class.java)
        if (am.canScheduleExactAlarms()) return
        try {
            activity.startActivity(
                Intent(
                    Settings.ACTION_REQUEST_SCHEDULE_EXACT_ALARM,
                    Uri.parse("package:${activity.packageName}")
                )
            )
        } catch (e: Exception) {
            android.util.Log.w("DunPlugin", "couldn't open the exact alarm setting", e)
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

    /**
     * Hands a downloaded package to Android's installer.
     *
     * Dun never installs anything itself: this opens the system's own installer
     * with the file, and the user approves it there. Android gates even that
     * behind a per-app "install unknown apps" switch, so if it hasn't been
     * granted, take the user to it instead of failing quietly.
     */
    @Command
    fun installUpdate(invoke: Invoke) {
        val path = JSONObject(invoke.getRawArgs()).optString("path")
        val file = java.io.File(path)
        if (!file.exists()) {
            invoke.reject("the download is missing")
            return
        }
        val ctx = activity.applicationContext
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
            !ctx.packageManager.canRequestPackageInstalls()
        ) {
            activity.startActivity(
                Intent(
                    Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                    Uri.parse("package:${activity.packageName}")
                )
            )
            invoke.reject("Allow Dun to install apps, then tap Install again")
            return
        }
        val uri = androidx.core.content.FileProvider.getUriForFile(
            ctx, "${ctx.packageName}.fileprovider", file
        )
        val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, "application/vnd.android.package-archive")
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        try {
            activity.startActivity(intent)
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("couldn't open the installer: $e")
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
        val nm = ctx.getSystemService(NotificationManager::class.java)
        // Keys are the checklist's, not Android's: the UI reads them directly.
        val ret = JSObject()
        ret.put("notifications", NotificationManagerCompat.from(ctx).areNotificationsEnabled())
        ret.put(
            "exactAlarms",
            Build.VERSION.SDK_INT < Build.VERSION_CODES.S || am.canScheduleExactAlarms()
        )
        ret.put("batteryUnrestricted", pm.isIgnoringBatteryOptimizations(ctx.packageName))
        // Without this, a channel asking to bypass Do Not Disturb is ignored.
        ret.put("dndAccess", nm.isNotificationPolicyAccessGranted)
        val audio = ctx.getSystemService(android.media.AudioManager::class.java)
        ret.put("alarmVolume", audio.getStreamVolume(android.media.AudioManager.STREAM_ALARM) > 0 &&
            !audio.isStreamMute(android.media.AudioManager.STREAM_ALARM))
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
            "dnd" -> Intent(Settings.ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS)
            "sound" -> Intent(Settings.ACTION_SOUND_SETTINGS)
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
