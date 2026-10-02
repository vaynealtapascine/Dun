package app.dun.android

import android.app.NotificationManager
import android.app.Notification
import android.app.Service
import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaPlayer
import android.media.RingtoneManager
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import android.os.VibrationEffect
import android.os.Vibrator
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

/** Short alarm playback, independent of the OEM's notification sound policy.
 * The scheduler still owns cadence; this service never loops or changes volume.
 */
class TimerSoundService : Service() {
    private val handler = Handler(Looper.getMainLooper())
    private var player: MediaPlayer? = null
    private var focus: AudioFocusRequest? = null
    private var currentId: Int? = null
    private var fallback: Notification? = null
    private val finish = Runnable { stopPlayback(); stopSelf() }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        instance = this
        val notification = NotificationCompat.Builder(this, Channels.id(Channels.TIMERS_RUNNING))
            .setSmallIcon(R.drawable.ic_stat_dun)
            .setContentTitle("Timer sounding")
            .setContentText("Use Done or Snooze on the timer notification")
            .setSilent(true).setOngoing(true).build()
        val id = intent?.getIntExtra("id", 0) ?: 0
        val token = intent?.getLongExtra("token", 0) ?: 0
        @Suppress("DEPRECATION")
        val original = intent?.getParcelableExtra<Notification>("notification")
        try {
            startForeground(-2, notification)
        } catch (e: Exception) {
            Log.w(TAG, "foreground playback unavailable", e)
            if (pending.remove(id, token) && original != null) postFallback(id, original)
            stopSelf()
            return START_NOT_STICKY
        }
        if (pending.remove(id, token)) {
            stopPlayback()
            currentId = id
            fallback = original
            val nm = getSystemService(NotificationManager::class.java)
            val channel = if (Build.VERSION.SDK_INT >= 26)
                nm.getNotificationChannel(intent!!.getStringExtra("channel")) else null
            val sound = if (Build.VERSION.SDK_INT >= 26) channel?.sound
                else RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM)
            if (sound == null || (channel != null && channel.importance < NotificationManager.IMPORTANCE_DEFAULT) ||
                !NotificationManagerCompat.from(this).areNotificationsEnabled()) {
                finish.run()
                return START_NOT_STICKY
            }
            val attrs = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ALARM)
                .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION).build()
            try {
                // Foreground before requesting focus: required when targeting Android 15+.
                if (Build.VERSION.SDK_INT >= 26) {
                    val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK)
                        .setAudioAttributes(attrs)
                        .setOnAudioFocusChangeListener({ change ->
                            if (change < 0) finish.run()
                        }, handler).build()
                    focus = request
                    if (getSystemService(AudioManager::class.java).requestAudioFocus(request) != AudioManager.AUDIOFOCUS_REQUEST_GRANTED) {
                        finish.run()
                        return START_NOT_STICKY
                    }
                }
                val p = MediaPlayer()
                player = p // Own it before setDataSource/prepare can throw.
                p.let {
                    p.setAudioAttributes(attrs)
                    p.setWakeMode(this, PowerManager.PARTIAL_WAKE_LOCK)
                    p.setDataSource(this, sound)
                    p.setOnPreparedListener {
                        it.start()
                        if (channel?.shouldVibrate() != false) {
                            val vibrator = getSystemService(Vibrator::class.java)
                            val pattern = channel?.vibrationPattern ?: longArrayOf(0, 250, 150, 250)
                            if (Build.VERSION.SDK_INT >= 26) {
                                vibrator.vibrate(VibrationEffect.createWaveform(pattern, -1), attrs)
                            } else {
                                @Suppress("DEPRECATION")
                                vibrator.vibrate(pattern, -1, attrs)
                            }
                        }
                        Log.i(TAG, "alarm playback started id=$id")
                    }
                    p.setOnCompletionListener { finish.run() }
                    p.setOnErrorListener { _, what, extra ->
                        Log.e(TAG, "alarm playback failed $what/$extra")
                        recoverSound()
                        finish.run(); true
                    }
                    p.prepareAsync()
                }
                handler.postDelayed(finish, 10_000)
            } catch (e: Exception) {
                Log.e(TAG, "could not play timer", e)
                recoverSound()
                finish.run()
            }
        } else if (player == null) finish.run()
        return START_NOT_STICKY
    }

    private fun postFallback(id: Int, notification: Notification) {
        try {
            NotificationManagerCompat.from(this).notify(id, notification)
        } catch (e: SecurityException) {
            Log.w(TAG, "notification permission revoked", e)
        }
    }

    private fun recoverSound() {
        currentId?.let { id -> fallback?.let { postFallback(id, it) } }
    }

    private fun stopPlayback() {
        handler.removeCallbacks(finish)
        player?.release()
        getSystemService(Vibrator::class.java).cancel()
        player = null
        currentId = null
        fallback = null
        if (Build.VERSION.SDK_INT >= 26) focus?.let {
            getSystemService(AudioManager::class.java).abandonAudioFocusRequest(it)
        }
        focus = null
    }

    override fun onDestroy() {
        stopPlayback()
        if (instance === this) instance = null
        super.onDestroy()
    }

    companion object {
        private const val TAG = "DunTimerSound"
        private val sequence = AtomicLong()
        private val pending = ConcurrentHashMap<Int, Long>()
        private var instance: TimerSoundService? = null

        fun play(context: Context, id: Int, channel: String, notification: Notification): Boolean {
            val token = sequence.incrementAndGet()
            pending[id] = token
            return try {
                ContextCompat.startForegroundService(context, Intent(context, TimerSoundService::class.java)
                    .putExtra("id", id).putExtra("token", token).putExtra("channel", channel)
                    .putExtra("notification", notification))
                true
            } catch (e: Exception) {
                pending.remove(id, token)
                Log.w(TAG, "alarm service unavailable; using notification sound", e)
                false
            }
        }

        fun cancel(id: Int) {
            pending.remove(id)
            Handler(Looper.getMainLooper()).post {
                instance?.let { if (it.currentId == id) it.finish.run() }
            }
        }
    }
}
