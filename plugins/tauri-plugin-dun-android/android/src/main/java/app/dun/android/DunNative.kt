package app.dun.android

import android.content.Context
import android.os.SystemClock
import android.util.Log
import org.json.JSONObject
import java.util.TimeZone

/**
 * The only way into Rust from Kotlin. Loading the library does not start Tauri
 * or need an Activity, so background receivers can call it with the app's UI
 * process dead.
 */
object DunNative {
    private const val TAG = "DunNative"

    /** Wall time spent in System.loadLibrary, or -1 if it was already loaded. */
    @Volatile
    var loadMs: Long = -1
        private set

    init {
        val t = SystemClock.elapsedRealtime()
        System.loadLibrary("dun_lib")
        loadMs = SystemClock.elapsedRealtime() - t
        Log.i(TAG, "loaded dun_lib in ${loadMs} ms")
    }

    /**
     * Applies [eventJson] to the shared core and returns
     * `{"ok":true,"plan":{...},"handlerMs":n}` or `{"ok":false,"error":"..."}`.
     */
    @JvmStatic
    external fun handleEvent(dataDir: String, tzId: String, nowMs: Long, eventJson: String): String

    /** Convenience wrapper that never throws; failures come back as ok=false. */
    fun dispatch(context: Context, event: JSONObject): JSONObject {
        return try {
            val raw = handleEvent(
                context.dataDir.absolutePath,
                TimeZone.getDefault().id,
                System.currentTimeMillis(),
                event.toString()
            )
            JSONObject(raw)
        } catch (t: Throwable) {
            Log.e(TAG, "handleEvent threw", t)
            JSONObject().put("ok", false).put("error", t.toString())
        }
    }
}
