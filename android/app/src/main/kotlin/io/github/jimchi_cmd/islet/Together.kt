package io.github.jimchi_cmd.islet

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.RemoteInput
import android.content.BroadcastReceiver
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.media.AudioManager
import android.media.MediaPlayer
import android.media.RingtoneManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import islet.island.Pop

/**
 * The phone and the computer together: the computer can ring the phone,
 * send it text, and answer its messages. What it sends shows on the
 * phone's island, and as a notification when Islet may post them.
 */
object Together {
    private const val CHANNEL = "computer"
    private const val RING_ID = 7001
    private const val SENT_ID = 7002
    /** Ringing stops by itself after this long. */
    private const val RING_MS = 60_000L

    private val main = Handler(Looper.getMainLooper())
    private var player: MediaPlayer? = null
    private var volumeBefore = -1

    val ringing: Boolean get() = player != null

    /** May Islet post notifications? (Android 13 asks; earlier ones allow it.) */
    fun mayNotify(context: Context): Boolean {
        if (Build.VERSION.SDK_INT >= 33 &&
            context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            return false
        }
        return context.getSystemService(NotificationManager::class.java)?.areNotificationsEnabled() == true
    }

    private fun channel(context: Context): NotificationManager? {
        val nm = context.getSystemService(NotificationManager::class.java) ?: return null
        if (Build.VERSION.SDK_INT >= 26 && nm.getNotificationChannel(CHANNEL) == null) {
            nm.createNotificationChannel(
                NotificationChannel(CHANNEL, T.t("From your computer"), NotificationManager.IMPORTANCE_HIGH).apply {
                    setSound(null, null)
                },
            )
        }
        return nm
    }

    private fun builder(context: Context): Notification.Builder =
        if (Build.VERSION.SDK_INT >= 26) Notification.Builder(context, CHANNEL) else @Suppress("DEPRECATION") Notification.Builder(context)

    // ---- Ringing --------------------------------------------------------------

    /** Ring loudly, even on silent (as an alarm does), until stopped or a minute has passed. */
    fun ring(context: Context) {
        val app = context.applicationContext
        stopRinging(app)
        val tone = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM)
            ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE)
            ?: return
        val audio = app.getSystemService(AudioManager::class.java)
        try {
            if (audio != null) {
                volumeBefore = audio.getStreamVolume(AudioManager.STREAM_ALARM)
                audio.setStreamVolume(AudioManager.STREAM_ALARM, audio.getStreamMaxVolume(AudioManager.STREAM_ALARM), 0)
            }
            player = MediaPlayer().apply {
                setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_ALARM)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                        .build(),
                )
                setDataSource(app, tone)
                isLooping = true
                prepare()
                start()
            }
        } catch (e: Exception) {
            stopRinging(app)
            return
        }
        main.postDelayed({ stopRinging(app) }, RING_MS)
        Isle.pop(Pop.Info(T.t("Your computer is looking for this phone")))
        if (!mayNotify(app)) return
        val stop = PendingIntent.getBroadcast(
            app, 0, Intent(app, StopRinging::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val n = builder(app)
            .setSmallIcon(android.R.drawable.ic_lock_idle_alarm)
            .setContentTitle(T.t("Found it?"))
            .setContentText(T.t("Your computer is looking for this phone"))
            .setCategory(Notification.CATEGORY_ALARM)
            .setOngoing(true)
            .setDeleteIntent(stop)
            .setContentIntent(stop)
            .addAction(Notification.Action.Builder(null, T.t("Stop"), stop).build())
            .build()
        channel(app)?.notify(RING_ID, n)
    }

    fun stopRinging(context: Context) {
        main.removeCallbacksAndMessages(null)
        player?.let {
            try {
                it.stop()
            } catch (e: IllegalStateException) {
                // Never started.
            }
            it.release()
        }
        player = null
        if (volumeBefore >= 0) {
            context.getSystemService(AudioManager::class.java)?.setStreamVolume(AudioManager.STREAM_ALARM, volumeBefore, 0)
            volumeBefore = -1
        }
        context.getSystemService(NotificationManager::class.java)?.cancel(RING_ID)
    }

    // ---- Text from the computer ---------------------------------------------------

    /** A link (https only): opened by a tap. */
    fun linkIn(text: String): String? =
        text.trim().takeIf { !it.contains(' ') && !it.contains('\n') && it.startsWith("https://") && it.length <= 2000 }

    /** Text or a link from the computer: onto the clipboard, and say so. */
    fun received(context: Context, computer: String, text: String) {
        val app = context.applicationContext
        app.getSystemService(ClipboardManager::class.java)?.setPrimaryClip(ClipData.newPlainText(computer, text))
        Isle.pop(Pop.Info(T.f("Copied from {}", computer)))
        if (!mayNotify(app)) return
        val link = linkIn(text)
        val open = PendingIntent.getActivity(
            app,
            1,
            if (link != null) {
                Intent(Intent.ACTION_VIEW, Uri.parse(link)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            } else {
                Intent(app, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            },
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val n = builder(app)
            .setSmallIcon(android.R.drawable.ic_menu_share)
            .setContentTitle(T.f("From {}", computer))
            .setContentText(text.take(200))
            .setStyle(Notification.BigTextStyle().bigText(text.take(1000)))
            .setSubText(T.t("Copied"))
            .setAutoCancel(true)
            .setContentIntent(open)
            .apply { if (link != null) addAction(Notification.Action.Builder(null, T.t("Open"), open).build()) }
            .build()
        channel(app)?.notify(SENT_ID, n)
    }

    // ---- Answering a message --------------------------------------------------------

    /** A notification's reply button: one that takes typed text. */
    fun replyAction(n: Notification): Notification.Action? =
        n.actions?.firstOrNull { a -> a.remoteInputs?.any { it.allowFreeFormInput } == true }

    /** Send `text` through a notification's reply button, as its own app would. */
    fun answer(context: Context, action: Notification.Action, text: String): Boolean {
        val inputs = action.remoteInputs ?: return false
        if (text.isBlank() || inputs.isEmpty()) return false
        val results = Bundle().apply { inputs.forEach { putCharSequence(it.resultKey, text.trim()) } }
        val fill = Intent()
        RemoteInput.addResultsToIntent(inputs, fill, results)
        return try {
            action.actionIntent.send(context, 0, fill)
            true
        } catch (e: PendingIntent.CanceledException) {
            false // The conversation was closed meanwhile.
        }
    }
}

/** The ringing notification's Stop button. */
class StopRinging : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent?) = Together.stopRinging(context)
}
