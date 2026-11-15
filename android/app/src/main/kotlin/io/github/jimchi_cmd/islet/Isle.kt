package io.github.jimchi_cmd.islet

import android.app.Notification
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.drawable.Drawable
import android.media.AudioAttributes
import android.media.MediaMetadata
import android.media.RingtoneManager
import android.media.session.MediaController
import android.media.session.PlaybackState
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.os.VibrationEffect
import android.os.Vibrator
import android.service.notification.StatusBarNotification
import islet.island.CalendarEvent
import islet.island.Colors
import islet.island.FocusTimer
import islet.island.Island
import islet.island.Live
import islet.island.Lyrics
import islet.island.Meetings
import islet.island.NoteFacts
import islet.island.NoteKind
import islet.island.Notes
import islet.island.Pop
import islet.island.Stopwatch
import islet.island.Weather
import java.util.concurrent.Executors

/** What a notification's activity can do when tapped: open it, or press one of its buttons. */
class NoteExtra(
    val pkg: String,
    val open: PendingIntent?,
    val actions: List<Pair<String, PendingIntent>>,
    val icon: Drawable?,
)

/**
 * The phone island's state and everything that changes it. Lives on the
 * main thread: the services, receivers and the island's view all post here.
 */
object Isle {
    private val main = Handler(Looper.getMainLooper())
    private val work = Executors.newSingleThreadExecutor()
    lateinit var app: Context
        private set
    lateinit var prefs: Prefs
        private set

    val island = Island()
    val focus = FocusTimer()
    val stopwatch = Stopwatch()

    // ---- Music ----
    var controller: MediaController? = null
        private set
    var art: Bitmap? = null
        internal set
    var artColor: Int? = null
        internal set
    var lyrics: Lyrics? = null
        internal set
    private var lyricsFor = ""
    private var artFor = ""

    // ---- Notifications ----
    val extras = HashMap<String, NoteExtra>()
    private val icons = HashMap<String, Drawable?>()

    // ---- The rest ----
    var weather: Weather? = null
    var events: List<CalendarEvent> = emptyList()
    var nextAlarm: Long? = null
    var airplane = false
    var dnd = false
    var cameraOn = false
    var micOn = false
    var torchOn = false
    var battery = -1
    var charging = false
    var screenOn = true

    /** The view that draws the island, while one is attached. */
    var onChange: (() -> Unit)? = null

    private var started = false

    fun init(context: Context) {
        if (started) return
        started = true
        app = context.applicationContext
        prefs = Prefs(app)
        island.showEmpty = prefs.showEmpty
        focus.focusMinutes = prefs.focusMinutes
        focus.breakMinutes = prefs.breakMinutes
    }

    fun now() = System.currentTimeMillis()

    /** Something changed: redraw, and come back when the next change is due. */
    fun changed() {
        if (Looper.myLooper() != Looper.getMainLooper()) {
            main.post { changed() }
            return
        }
        island.showEmpty = prefs.showEmpty
        updateEvent()
        island.tick(now())
        onChange?.invoke()
        main.removeCallbacks(due)
        val next = listOfNotNull(island.nextChange(), focusEnd(), Meetings.nextCheck(events, now()))
            .minOrNull()
        if (next != null) main.postDelayed(due, (next - now()).coerceIn(50, 15 * 60_000))
    }

    private val due = Runnable {
        checkFocus()
        changed()
    }

    fun post(r: () -> Unit) {
        main.post(r)
    }

    fun background(r: () -> Unit) {
        work.execute(r)
    }

    // ---- Pop-ups ---------------------------------------------------------------

    fun pop(p: Pop) {
        main.post {
            island.pop(p, now())
            changed()
        }
    }

    // ---- Notifications -----------------------------------------------------------

    fun appName(pkg: String): String = try {
        val pm = app.packageManager
        pm.getApplicationLabel(pm.getApplicationInfo(pkg, 0)).toString()
    } catch (e: Exception) {
        pkg.substringAfterLast('.').replaceFirstChar { it.uppercase() }
    }

    fun appIcon(pkg: String): Drawable? = icons.getOrPut(pkg) {
        try {
            app.packageManager.getApplicationIcon(pkg)
        } catch (e: Exception) {
            null
        }
    }

    private fun facts(sbn: StatusBarNotification): NoteFacts? {
        val n = sbn.notification ?: return null
        val x = n.extras ?: return null
        val template = x.getString(Notification.EXTRA_TEMPLATE).orEmpty()
        return NoteFacts(
            key = sbn.key,
            pkg = sbn.packageName,
            app = appName(sbn.packageName),
            category = n.category,
            ongoing = sbn.isOngoing,
            groupSummary = n.flags and Notification.FLAG_GROUP_SUMMARY != 0,
            secret = n.visibility == Notification.VISIBILITY_SECRET,
            title = x.getCharSequence(Notification.EXTRA_TITLE)?.toString().orEmpty(),
            text = (x.getCharSequence(Notification.EXTRA_BIG_TEXT) ?: x.getCharSequence(Notification.EXTRA_TEXT))?.toString().orEmpty(),
            progress = x.getInt(Notification.EXTRA_PROGRESS, 0),
            progressMax = x.getInt(Notification.EXTRA_PROGRESS_MAX, 0),
            indeterminate = x.getBoolean(Notification.EXTRA_PROGRESS_INDETERMINATE, false),
            chronometer = x.getBoolean(Notification.EXTRA_SHOW_CHRONOMETER, false),
            countDown = x.getBoolean(Notification.EXTRA_CHRONOMETER_COUNT_DOWN, false),
            whenMs = n.`when`,
            media = template.contains("MediaStyle"),
            callStyle = template.contains("CallStyle"),
            actions = n.actions.orEmpty().mapNotNull { it.title?.toString()?.lowercase() },
        )
    }

    /** A notification arrived or changed (on the main thread). */
    fun posted(sbn: StatusBarNotification, quiet: Boolean = false) {
        if (!started) return
        val f = facts(sbn) ?: return
        val n = sbn.notification
        val id = "note:${sbn.key}"
        val actions = n.actions.orEmpty().mapNotNull { a -> a.actionIntent?.let { (a.title?.toString() ?: "") to it } }
        val icon = try {
            n.getLargeIcon()?.loadDrawable(app)
        } catch (e: Exception) {
            null
        }
        extras[sbn.key] = NoteExtra(sbn.packageName, n.contentIntent, actions, icon)
        val t = now()
        when (val kind = Notes.classify(f, app.packageName)) {
            is NoteKind.Call -> if (prefs.calls) {
                val wasThere = island.get(id) != null
                island.put(Live.Call(id, kind.name, f.app, kind.incoming, kind.since))
                if (kind.incoming && !wasThere && !quiet) island.open(id)
            }
            is NoteKind.Navigation -> if (prefs.navigation) island.put(Live.Navigation(id, kind.title, kind.text, f.app))
            is NoteKind.Countdown -> island.put(Live.Countdown(id, kind.label, kind.endsAt, f.app))
            is NoteKind.Progress -> if (prefs.progress) island.put(Live.Progress(id, kind.title, f.app, kind.percent))
            NoteKind.Popup -> {
                island.remove(id)
                if (!quiet && prefs.popNotes && sbn.packageName !in prefs.quietApps && !dnd) {
                    island.pop(Pop.Note(sbn.key, f.app, f.title.trim(), f.text.trim()), t)
                }
            }
            NoteKind.Ignore -> island.remove(id)
        }
        changed()
    }

    fun removed(sbn: StatusBarNotification) {
        if (!started) return
        val id = "note:${sbn.key}"
        val live = island.get(id)
        island.remove(id)
        island.forget(sbn.key, now())
        // A finished download says so once.
        if (live is Live.Progress && live.percent >= 90 && prefs.progress) {
            island.pop(Pop.Info("${live.title}: done"), now())
        }
        extras.remove(sbn.key)
        changed()
    }

    /** Open what an activity or pop-up came from. */
    fun open(key: String) {
        val e = extras[key.removePrefix("note:")] ?: return
        send(e.open) || launch(e.pkg)
    }

    fun send(intent: PendingIntent?): Boolean {
        if (intent == null) return false
        return try {
            if (Build.VERSION.SDK_INT >= 34) {
                val opts = android.app.ActivityOptions.makeBasic()
                    .setPendingIntentBackgroundActivityStartMode(android.app.ActivityOptions.MODE_BACKGROUND_ACTIVITY_START_ALLOWED)
                intent.send(app, 0, null, null, null, null, opts.toBundle())
            } else {
                intent.send()
            }
            true
        } catch (e: PendingIntent.CanceledException) {
            false
        }
    }

    fun launch(pkg: String): Boolean {
        val i = app.packageManager.getLaunchIntentForPackage(pkg) ?: return false
        return start(i)
    }

    fun start(i: Intent): Boolean = try {
        app.startActivity(i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        true
    } catch (e: Exception) {
        false
    }

    fun openUrl(url: String) = start(Intent(Intent.ACTION_VIEW, Uri.parse(url)))

    // ---- Music ---------------------------------------------------------------------

    /** The media session to follow changed, or its song or state did. */
    fun media(c: MediaController?) {
        if (!started) return
        controller = c
        val meta = c?.metadata
        val state = c?.playbackState
        val title = meta?.getString(MediaMetadata.METADATA_KEY_TITLE).orEmpty()
        val st = state?.state ?: PlaybackState.STATE_NONE
        val gone = title.isEmpty() || st == PlaybackState.STATE_NONE || st == PlaybackState.STATE_STOPPED || st == PlaybackState.STATE_ERROR
        if (gone) {
            island.remove("music")
            changed()
            return
        }
        val artist = meta?.getString(MediaMetadata.METADATA_KEY_ARTIST)
            ?: meta?.getString(MediaMetadata.METADATA_KEY_ALBUM_ARTIST) ?: ""
        val duration = (meta?.getLong(MediaMetadata.METADATA_KEY_DURATION) ?: 0L).coerceAtLeast(0L)
        val playing = st == PlaybackState.STATE_PLAYING || st == PlaybackState.STATE_BUFFERING
        val position = (state?.position ?: 0L).coerceAtLeast(0L)
        val at = state?.lastPositionUpdateTime?.takeIf { it > 0 } ?: SystemClock.elapsedRealtime()
        island.put(Live.Music(title, artist, appName(c?.packageName.orEmpty()), playing, duration, position, at))
        val song = "$title|$artist"
        if (song != artFor) {
            artFor = song
            val bmp = meta?.getBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART)
                ?: meta?.getBitmap(MediaMetadata.METADATA_KEY_ART)
                ?: meta?.getBitmap(MediaMetadata.METADATA_KEY_DISPLAY_ICON)
            art = bmp?.let { scaled(it) }
            artColor = art?.let { colourOf(it) }
        }
        if (prefs.lyrics && song != lyricsFor) {
            lyricsFor = song
            lyrics = null
            fetchLyrics(title, artist, duration, song)
        } else if (!prefs.lyrics) {
            lyrics = null
            lyricsFor = ""
        }
        changed()
    }

    private fun scaled(b: Bitmap): Bitmap? = try {
        val side = 160
        if (b.width <= side && b.height <= side) b.copy(Bitmap.Config.ARGB_8888, false) else Bitmap.createScaledBitmap(b, side, side * b.height / maxOf(1, b.width), true)
    } catch (e: Exception) {
        null
    }

    private fun colourOf(b: Bitmap): Int? {
        val small = Bitmap.createScaledBitmap(b, 24, 24, true)
        val px = IntArray(24 * 24)
        small.getPixels(px, 0, 24, 0, 0, 24, 24)
        return Colors.dominant(px)?.let { Colors.forBorder(it) }
    }

    private fun fetchLyrics(title: String, artist: String, duration: Long, song: String) {
        background {
            val found = Net.get(Lyrics.lrclibUrl(title, artist, duration))?.let { Lyrics.fromLrclib(it) }
            post {
                if (lyricsFor == song) {
                    lyrics = found
                    changed()
                }
            }
        }
    }

    fun mediaPositionNow(): Long {
        val m = island.get("music") as? Live.Music ?: return 0
        return m.positionAt(SystemClock.elapsedRealtime())
    }

    fun playPause() {
        val c = controller ?: return
        if (c.playbackState?.state == PlaybackState.STATE_PLAYING) c.transportControls.pause() else c.transportControls.play()
    }

    fun next() = controller?.transportControls?.skipToNext()

    fun previous() = controller?.transportControls?.skipToPrevious()

    fun seek(ms: Long) = controller?.transportControls?.seekTo(ms.coerceAtLeast(0))

    fun openPlayer() {
        val c = controller ?: return
        send(c.sessionActivity) || launch(c.packageName)
    }

    // ---- Timers -------------------------------------------------------------------

    private fun focusEnd(): Long? = focus.live()?.takeIf { it.pausedLeft == null }?.endsAt

    fun startFocus(minutes: Double, label: String = "Timer") {
        focus.start(minutes, now(), label)
        syncTimers()
    }

    fun startPomodoro() {
        focus.focusMinutes = prefs.focusMinutes
        focus.breakMinutes = prefs.breakMinutes
        focus.startPomodoro(now())
        syncTimers()
    }

    fun syncTimers() {
        focus.live()?.let { island.put(it) } ?: island.remove("timer")
        stopwatch.live()?.let { island.put(it) } ?: island.remove("stopwatch")
        changed()
    }

    private fun checkFocus() {
        val done = focus.check(now()) ?: return
        val label = if (focus.active) "$done done · ${focus.live()?.label} now" else "$done done"
        island.pop(Pop.TimerDone(label), now())
        alert()
        syncTimers()
    }

    /** A short buzz and the notification sound, for a finished timer. */
    private fun alert() {
        try {
            val v = app.getSystemService(Vibrator::class.java)
            v?.vibrate(VibrationEffect.createWaveform(longArrayOf(0, 250, 150, 250), -1))
        } catch (e: Exception) {
            // No vibrator.
        }
        try {
            val r = RingtoneManager.getRingtone(app, RingtoneManager.getDefaultUri(RingtoneManager.TYPE_NOTIFICATION))
            r?.audioAttributes = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ALARM).build()
            r?.play()
        } catch (e: Exception) {
            // No sound.
        }
    }

    // ---- Calendar -------------------------------------------------------------------

    private fun updateEvent() {
        if (!prefs.calendar) {
            island.retain("event:", emptySet())
            return
        }
        val e = Meetings.current(events, now())
        if (e == null) {
            island.retain("event:", emptySet())
        } else {
            val id = "event:${e.id}"
            island.retain("event:", setOf(id))
            island.put(Live.Event(id, e.title, e.begin, Meetings.joinLink(e)))
        }
    }
}
