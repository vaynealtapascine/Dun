package app.dun.android

import android.content.Context
import androidx.work.Constraints
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/**
 * Delivers a change the phone made while it couldn't reach the PC.
 *
 * Marking something done on the phone has to get through, or the PC goes on
 * nagging about it. While anything is still due the alarm path retries every
 * nag anyway — but the common case is the opposite: the last reminder is done,
 * nothing is due, the alarm is cancelled, and the push has nowhere to be
 * retried from. So a job takes over until it succeeds.
 *
 * It waits for a network rather than waking on a timer, and never rings:
 * `setAlarmClock` would put an alarm icon in the status bar for something the
 * user shouldn't have to know about.
 */
class PendingPushWorker(context: Context, params: WorkerParameters) : Worker(context, params) {

    override fun doWork(): Result {
        val context = applicationContext
        // On the receiver's thread, so this can't interleave with an alarm or a
        // button press landing at the same moment.
        val stillPending = DunReceiver.EXECUTOR.submit<Boolean> {
            val response = DunNative.dispatch(context, JSONObject().put("type", "syncOnly"))
            // A broken core is the alarm path's problem to shout about; here,
            // just come back later.
            if (!response.optBoolean("ok", false)) return@submit true
            val plan = response.getJSONObject("plan")
            // The answer may carry changes from the PC, so apply it like any other.
            PlanExecutor.apply(context, plan, fromWorker = true)
            plan.optBoolean("pendingPush", false)
        }.get()
        return if (stillPending) Result.retry() else Result.success()
    }

    companion object {
        private const val NAME = "dun-pending-push"
        private const val RETRY_MINUTES = 2L

        /** Starts the job, or leaves a running one alone so its backoff holds. */
        fun ensure(context: Context) {
            val request = OneTimeWorkRequestBuilder<PendingPushWorker>()
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .setBackoffCriteria(androidx.work.BackoffPolicy.LINEAR, RETRY_MINUTES, TimeUnit.MINUTES)
                .build()
            WorkManager.getInstance(context).enqueueUniqueWork(NAME, ExistingWorkPolicy.KEEP, request)
        }

        fun cancel(context: Context) {
            WorkManager.getInstance(context).cancelUniqueWork(NAME)
        }
    }
}
