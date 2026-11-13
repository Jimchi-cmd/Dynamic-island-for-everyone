package io.github.jimchi_cmd.islet

import android.app.Notification
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.MediaMetadata
import android.media.session.MediaController
import android.media.session.MediaSessionManager
import android.media.session.PlaybackState
import android.net.ConnectivityManager
import android.net.Network
import android.os.BatteryManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import androidx.core.content.ContextCompat
import islet.protocol.FromPhone
import islet.protocol.Link
import islet.protocol.LinkListener
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

/**
 * The part that runs in the background. Android starts it once the user
 * allows notification access, and keeps it running; it holds the link to
 * the computer and sends it three things: new notifications, what is
 * playing, and the battery level. It also presses play, next and previous
 * when the island's buttons ask.
 */
class IsletListener : NotificationListenerService(), LinkListener {
    private var link: Link? = null
    private var sender: ExecutorService? = null
    private val main = Handler(Looper.getMainLooper())
    private var controller: MediaController? = null
    private var lastMedia = ""
    private var lastBattery = ""
    private var watching = false

    /** The last text sent for each notification, so an update that changes nothing isn't sent again. */
    private val sentNotes = object : LinkedHashMap<String, String>() {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, String>?) = size > 60
    }

    private val mediaCallback = object : MediaController.Callback() {
        override fun onPlaybackStateChanged(state: PlaybackState?) = sendMedia()

        override fun onMetadataChanged(metadata: MediaMetadata?) = sendMedia()

        override fun onSessionDestroyed() = pickSession()
    }

    private val sessionsChanged = MediaSessionManager.OnActiveSessionsChangedListener { pickSession() }

    private val batteryReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            if (intent != null) sendBattery(intent)
        }
    }

    private val networkCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            link?.retryNow()
        }
    }

    override fun onListenerConnected() {
        instance = this
        sender = Executors.newSingleThreadExecutor()
        watch()
        reload()
    }

    override fun onListenerDisconnected() {
        teardown()
    }

    override fun onDestroy() {
        teardown()
        super.onDestroy()
    }

    /** (Re)start the link with the pairing as it now stands. */
    fun reload() {
        link?.stop()
        link = null
        Hub.computer = null
        Hub.problem = ""
        val pairing = Store.pairing(this)
        if (pairing != null) {
            link = Link(pairing, phoneName(), this).also { it.start() }
        }
        Hub.changed()
    }

    private fun watch() {
        if (watching) return
        watching = true
        ContextCompat.registerReceiver(
            this,
            batteryReceiver,
            IntentFilter(Intent.ACTION_BATTERY_CHANGED),
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        try {
            getSystemService(MediaSessionManager::class.java)
                ?.addOnActiveSessionsChangedListener(sessionsChanged, ComponentName(this, IsletListener::class.java))
        } catch (e: SecurityException) {
            // Notification access was just withdrawn.
        }
        try {
            getSystemService(ConnectivityManager::class.java)?.registerDefaultNetworkCallback(networkCallback)
        } catch (e: RuntimeException) {
            // Too many callbacks registered on this device: the link retries on its own timer.
        }
        pickSession()
    }

    private fun teardown() {
        if (instance === this) instance = null
        link?.stop()
        link = null
        Hub.computer = null
        Hub.changed()
        if (watching) {
            watching = false
            try {
                unregisterReceiver(batteryReceiver)
            } catch (e: IllegalArgumentException) {
                // Wasn't registered.
            }
            getSystemService(MediaSessionManager::class.java)?.removeOnActiveSessionsChangedListener(sessionsChanged)
            try {
                getSystemService(ConnectivityManager::class.java)?.unregisterNetworkCallback(networkCallback)
            } catch (e: RuntimeException) {
                // Wasn't registered.
            }
        }
        controller?.unregisterCallback(mediaCallback)
        controller = null
        sender?.shutdownNow()
        sender = null
    }

    /** Send off the main thread: the network isn't allowed on it. */
    private fun post(json: String) {
        val current = link ?: return
        try {
            sender?.execute { current.send(json) }
        } catch (e: java.util.concurrent.RejectedExecutionException) {
            // Shutting down.
        }
    }

    private fun phoneName(): String {
        val model = Build.MODEL ?: "Phone"
        val maker = Build.MANUFACTURER ?: ""
        return if (maker.isEmpty() || model.startsWith(maker, ignoreCase = true)) model else "$maker $model"
    }

    private fun appName(packageName: String?): String {
        if (packageName.isNullOrEmpty()) return ""
        return try {
            packageManager.getApplicationLabel(packageManager.getApplicationInfo(packageName, 0)).toString()
        } catch (e: Exception) {
            packageName.substringAfterLast('.').replaceFirstChar { it.uppercase() }
        }
    }

    // ---- The link's events ------------------------------------------------

    override fun onConnected(computer: String) {
        Hub.computer = computer
        Hub.problem = ""
        Hub.changed()
        main.post {
            // Tell the computer where things stand.
            lastBattery = ""
            lastMedia = ""
            registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))?.let { sendBattery(it) }
            sendMedia()
        }
    }

    override fun onDisconnected(reason: String) {
        Hub.computer = null
        Hub.problem = reason
        Hub.changed()
    }

    override fun onCommand(action: String) {
        main.post {
            val c = controller ?: return@post
            val controls = c.transportControls
            when (action) {
                "play_pause" -> if (c.playbackState?.state == PlaybackState.STATE_PLAYING) controls.pause() else controls.play()
                "next" -> controls.skipToNext()
                "previous" -> controls.skipToPrevious()
            }
        }
    }

    // ---- Notifications ----------------------------------------------------

    override fun onNotificationPosted(sbn: StatusBarNotification?) {
        if (sbn == null || !Store.sendNotes(this)) return
        // Not our own, not the lasting ones (music players, downloads,
        // "app is running"), and not ones marked secret.
        if (sbn.packageName == packageName || sbn.isOngoing) return
        val n = sbn.notification ?: return
        if (n.flags and Notification.FLAG_GROUP_SUMMARY != 0) return
        if (n.visibility == Notification.VISIBILITY_SECRET) return
        when (n.category) {
            Notification.CATEGORY_TRANSPORT, Notification.CATEGORY_SERVICE, Notification.CATEGORY_PROGRESS -> return
        }
        val extras = n.extras ?: return
        val title = extras.getCharSequence(Notification.EXTRA_TITLE)?.toString()?.trim().orEmpty()
        val text = (extras.getCharSequence(Notification.EXTRA_BIG_TEXT) ?: extras.getCharSequence(Notification.EXTRA_TEXT))
            ?.toString()?.trim().orEmpty()
        if (title.isEmpty() && text.isEmpty()) return
        val shown = "$title\n$text"
        if (sentNotes[sbn.key] == shown) return
        sentNotes[sbn.key] = shown
        post(FromPhone.note(appName(sbn.packageName), title.take(120), text.take(300)))
    }

    // ---- Music --------------------------------------------------------------

    /** Follow the session that is playing, or else the first one. */
    private fun pickSession() {
        val sessions = try {
            getSystemService(MediaSessionManager::class.java)
                ?.getActiveSessions(ComponentName(this, IsletListener::class.java))
                .orEmpty()
        } catch (e: SecurityException) {
            emptyList()
        }
        val chosen = sessions.firstOrNull { it.playbackState?.state == PlaybackState.STATE_PLAYING } ?: sessions.firstOrNull()
        if (chosen?.sessionToken != controller?.sessionToken) {
            controller?.unregisterCallback(mediaCallback)
            controller = chosen
            chosen?.registerCallback(mediaCallback, main)
        }
        sendMedia()
    }

    private fun sendMedia() {
        val c = controller
        val metadata = c?.metadata
        val state = c?.playbackState
        val title = if (Store.sendMedia(this)) metadata?.getString(MediaMetadata.METADATA_KEY_TITLE).orEmpty() else ""
        if (title.isEmpty()) {
            if (lastMedia != "none") {
                lastMedia = "none"
                post(FromPhone.media("", "", "", false, 0.0, 0.0))
            }
            return
        }
        val artist = metadata?.getString(MediaMetadata.METADATA_KEY_ARTIST)
            ?: metadata?.getString(MediaMetadata.METADATA_KEY_ALBUM_ARTIST)
            ?: ""
        val duration = (metadata?.getLong(MediaMetadata.METADATA_KEY_DURATION) ?: 0L).coerceAtLeast(0L) / 1000.0
        val playing = state?.state == PlaybackState.STATE_PLAYING
        // The position is as of its last update: bring it up to now.
        var position = (state?.position ?: 0L).coerceAtLeast(0L) / 1000.0
        if (state != null && playing && state.lastPositionUpdateTime > 0) {
            position += (SystemClock.elapsedRealtime() - state.lastPositionUpdateTime) / 1000.0 * state.playbackSpeed
        }
        if (duration > 0) position = position.coerceIn(0.0, duration)
        // The same song in the same state within a second or two isn't news.
        val key = "$title|$artist|$playing|$duration|${(position / 3).toLong()}"
        if (key == lastMedia) return
        lastMedia = key
        post(FromPhone.media(title.take(200), artist.take(200), appName(c?.packageName), playing, duration, position))
    }

    // ---- Battery ------------------------------------------------------------

    private fun sendBattery(intent: Intent) {
        val level = intent.getIntExtra(BatteryManager.EXTRA_LEVEL, -1)
        val scale = intent.getIntExtra(BatteryManager.EXTRA_SCALE, 100)
        if (level < 0 || scale <= 0) return
        val status = intent.getIntExtra(BatteryManager.EXTRA_STATUS, -1)
        val charging = status == BatteryManager.BATTERY_STATUS_CHARGING || status == BatteryManager.BATTERY_STATUS_FULL
        val percent = level * 100 / scale
        val key = "$percent|$charging"
        if (key == lastBattery) return
        lastBattery = key
        post(FromPhone.battery(percent, charging))
    }

    companion object {
        /** The running service, if Android has started it. */
        @Volatile var instance: IsletListener? = null
    }
}
