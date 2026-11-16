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
import android.net.TrafficStats
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import islet.island.CalendarEvent
import islet.island.Colors
import islet.island.FocusTimer
import islet.island.Format
import islet.island.Island
import islet.island.Live
import islet.island.Lyrics
import islet.island.Meetings
import islet.island.NoteFacts
import islet.island.NoteKind
import islet.island.Notes
import islet.island.Pop
import islet.island.Stopwatch
import islet.island.Streak
import islet.island.Timetable
import islet.island.Weather
import java.util.concurrent.Executors

/** What a notification's activity can do when tapped: open it, or press one of its buttons. */
class NoteExtra(
    val pkg: String,
    val open: PendingIntent?,
    val actions: List<Pair<String, PendingIntent>>,
    val icon: Drawable?,
    /** The buttons as Android gave them (a "Reply" one takes typed text). */
    val raw: List<Notification.Action> = emptyList(),
)

/**
 * The phone island's state and everything that changes it. Lives on the
 * main thread: the services, receivers and the island's view all post here.
 */
object Isle {
    private val main = Handler(Looper.getMainLooper())
    private val work = Executors.newSingleThreadExecutor()

    /**
     * Covers and lyrics get a worker of their own, so they never wait
     * behind the weather, the calendar or a speed test.
     */
    private val mediaWork = Executors.newSingleThreadExecutor()

    /** Runs cover and lyrics work (tests run it straight away). */
    internal var onMediaWorker: (() -> Unit) -> Unit = { r -> mediaWork.execute(r) }
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

    /** Lyrics were looked for and this song has none. */
    var lyricsMissing = false
        internal set
    private var lyricsFor = ""
    private var artFor = ""

    /** The cover came from the song's details (not from its notification's picture). */
    private var artFromSong = false

    /**
     * Each player's media notification picture, with the song it was for:
     * the cover, for players that don't put it in the song's details.
     */
    private val mediaPictures = HashMap<String, Pair<String, Bitmap>>()

    /** Counts cover lookups, so a slow one for an old song never replaces a newer one. */
    private var artSeq = 0

    /** When the music was paused; it leaves the island [PAUSE_LINGER] later, as on the computer. */
    private var pausedSince: Long? = null
    const val PAUSE_LINGER = 15_000L

    // ---- Notifications ----
    val extras = HashMap<String, NoteExtra>()

    /** When each app last sent a notification, for the pop-up's count. */
    private val sent = HashMap<String, ArrayList<Long>>()
    private val icons = HashMap<String, Drawable?>()

    // ---- The rest ----
    var weather: Weather? = null
    var events: List<CalendarEvent> = emptyList()

    /** Calendar events in the next two weeks that look like deadlines (exam, essay, due…). */
    var deadlines: List<CalendarEvent> = emptyList()
    var nextAlarm: Long? = null
    var airplane = false
    var dnd = false
    var cameraOn = false
    var micOn = false
    var torchOn = false
    var battery = -1
    var charging = false
    var screenOn = true
    var wifiOn = false
    /** The Wi-Fi network's name, where Android shares it (it needs location access). */
    var wifiName: String? = null
    var btOn = false
    /** The Bluetooth device that last connected, while it stays connected. */
    var btName: String? = null

    /** When the music sleep timer pauses what's playing (null: off). */
    var sleepAt: Long? = null
        private set

    /** The live bars' levels (0..1) from the sound playing, and when they were read. */
    var levels = FloatArray(4)
        internal set
    var levelsAt = 0L
        internal set

    /** Islet turned Do Not Disturb on for a lesson, so turns it off after. */
    private var quietForLesson = false

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

    /** The wall clock (tests move it on). */
    internal var clock: () -> Long = { System.currentTimeMillis() }

    fun now() = clock()

    /** Something changed: redraw, and come back when the next change is due. */
    fun changed() {
        if (Looper.myLooper() != Looper.getMainLooper()) {
            main.post { changed() }
            return
        }
        island.showEmpty = prefs.showEmpty
        updateEvent()
        val timetable = prefs.timetable
        updateLesson(timetable)
        dropPausedMusic()
        island.tick(now())
        onChange?.invoke()
        LiveBars.update()
        main.removeCallbacks(due)
        val next = listOfNotNull(
            island.nextChange(),
            focusEnd(),
            Meetings.nextCheck(events, now()),
            timetable.nextCheck(now()).takeIf { prefs.lessons || prefs.lessonQuiet },
            sleepAt,
            pausedSince?.takeIf { !island.isOpen }?.let { it + PAUSE_LINGER },
        ).minOrNull()
        if (next != null) main.postDelayed(due, (next - now()).coerceIn(50, 15 * 60_000))
    }

    private val due = Runnable {
        checkFocus()
        checkSleep()
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
        if (f.media) mediaPicture(sbn)
        val n = sbn.notification
        val id = "note:${sbn.key}"
        val actions = n.actions.orEmpty().mapNotNull { a -> a.actionIntent?.let { (a.title?.toString() ?: "") to it } }
        val icon = try {
            n.getLargeIcon()?.loadDrawable(app)
        } catch (e: Exception) {
            null
        }
        val raw = n.actions.orEmpty().filter { it.actionIntent != null && !it.title.isNullOrBlank() }
        extras[sbn.key] = NoteExtra(sbn.packageName, n.contentIntent, actions, icon, raw)
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
                    val recent = sent.getOrPut(sbn.packageName) { ArrayList() }
                    recent.removeAll { t - it > 10 * 60_000L }
                    recent.add(t)
                    val labels = raw.take(2).map { it.title.toString() }
                    island.pop(Pop.Note(sbn.key, f.app, f.title.trim(), f.text.trim(), labels, recent.size), t)
                }
            }
            is NoteKind.Payment -> {
                island.remove(id)
                if (!quiet && prefs.popNotes) island.pop(Pop.Payment(kind.merchant, kind.amount), t)
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

    /**
     * Press a notification's button from its pop-up. "Reply" and the like
     * take text: a small box opens to type it in.
     */
    fun act(key: String, index: Int) {
        val action = extras[key]?.raw?.getOrNull(index) ?: return
        island.dismissPop(now())
        if (!action.remoteInputs.isNullOrEmpty()) {
            ReplyActivity.pending = action
            start(Intent(app, ReplyActivity::class.java))
        } else {
            send(action.actionIntent)
        }
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
        song(c?.packageName.orEmpty(), c?.metadata, c?.playbackState)
    }

    /** What a player says it's playing (split out so it can be tested without a real player). */
    internal fun song(pkg: String, meta: MediaMetadata?, state: PlaybackState?) {
        val title = meta?.getString(MediaMetadata.METADATA_KEY_TITLE).orEmpty()
        val st = state?.state ?: PlaybackState.STATE_NONE
        val gone = title.isEmpty() || st == PlaybackState.STATE_NONE || st == PlaybackState.STATE_STOPPED || st == PlaybackState.STATE_ERROR
        if (gone) {
            island.remove("music")
            pausedSince = null
            changed()
            return
        }
        val artist = meta?.getString(MediaMetadata.METADATA_KEY_ARTIST)
            ?: meta?.getString(MediaMetadata.METADATA_KEY_ALBUM_ARTIST) ?: ""
        val duration = (meta?.getLong(MediaMetadata.METADATA_KEY_DURATION) ?: 0L).coerceAtLeast(0L)
        val playing = st == PlaybackState.STATE_PLAYING || st == PlaybackState.STATE_BUFFERING
        val position = (state?.position ?: 0L).coerceAtLeast(0L)
        val at = state?.lastPositionUpdateTime?.takeIf { it > 0 } ?: SystemClock.elapsedRealtime()
        // As on the computer: paused music stays a little while, then the
        // empty island (and its time) comes back. Music already paused when
        // first seen doesn't come up at all.
        val shown = island.get("music") as? Live.Music
        if (playing) {
            pausedSince = null
        } else if (shown == null) {
            changed()
            return
        } else if (pausedSince == null) {
            pausedSince = now()
        }
        val speed = state?.playbackSpeed?.takeIf { it > 0f } ?: 1f
        island.put(Live.Music(title, artist, appName(pkg), playing, duration, position, at, speed))
        val song = "$title|$artist"
        // The cover: from the song's details if the player puts it there
        // (some only add it a moment later), else its notification's picture
        // (only if that picture is for this song, never the last one's).
        if (song != artFor || !artFromSong) {
            val bmp = meta?.getBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART)
                ?: meta?.getBitmap(MediaMetadata.METADATA_KEY_ART)
                ?: meta?.getBitmap(MediaMetadata.METADATA_KEY_DISPLAY_ICON)
            val fresh = song != artFor
            artFor = song
            val seq = ++artSeq
            when {
                bmp != null -> {
                    // Small enough to do straight away, so the new cover shows with the new song.
                    art = scaled(bmp)
                    artColor = art?.let { colourOf(it) }
                    artFromSong = true
                }
                fresh || art == null -> {
                    art = mediaPictures[pkg]?.takeIf { it.first == title }?.second
                    artColor = art?.let { colourOf(it) }
                    artFromSong = false
                    // A cover the player keeps as a file: read it off the main thread.
                    if (meta != null) {
                        onMediaWorker {
                            val local = localArt(meta)?.let { scaled(it) }
                            val colour = local?.let { colourOf(it) }
                            post {
                                if (local != null && seq == artSeq && artFor == song) {
                                    art = local
                                    artColor = colour
                                    artFromSong = true
                                    changed()
                                }
                            }
                        }
                    }
                }
            }
        }
        if (prefs.lyrics && song != lyricsFor) {
            lyricsFor = song
            lyrics = null
            lyricsMissing = false
            fetchLyrics(title, artist, duration, song)
        } else if (!prefs.lyrics) {
            lyrics = null
            lyricsMissing = false
            lyricsFor = ""
        }
        changed()
    }

    /**
     * The cover at 160 px. Players often hand it over as a GPU-only
     * ("hardware") bitmap, which can't be scaled or read directly: copy it
     * to an ordinary one first, or the cover silently goes missing.
     */
    private fun scaled(b: Bitmap): Bitmap? = try {
        val side = 160
        val soft = if (Build.VERSION.SDK_INT >= 26 && b.config == Bitmap.Config.HARDWARE) b.copy(Bitmap.Config.ARGB_8888, false) else b
        when {
            soft == null -> null
            soft.width <= side && soft.height <= side -> soft.copy(Bitmap.Config.ARGB_8888, false)
            else -> Bitmap.createScaledBitmap(soft, side, side * soft.height / maxOf(1, soft.width), true)
        }
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
        onMediaWorker {
            val found = Net.get(Lyrics.lrclibUrl(title, artist, duration))?.let { Lyrics.fromLrclib(it) }
            post {
                if (lyricsFor == song) {
                    lyrics = found?.takeIf { it.lines.isNotEmpty() }
                    lyricsMissing = lyrics == null
                    changed()
                }
            }
        }
    }

    /** The lyrics button on the music card: switching it on is what lets Islet ask LRCLIB. */
    fun toggleLyrics() {
        prefs.lyrics = !prefs.lyrics
        if (prefs.lyrics) island.pop(Pop.Info("Lyrics on · from LRCLIB"), now())
        media(controller)
    }

    /** A cover the player shares as a file on the phone (never fetched from the internet). */
    private fun localArt(meta: MediaMetadata?): Bitmap? {
        meta ?: return null
        val uri = listOf(
            MediaMetadata.METADATA_KEY_ALBUM_ART_URI,
            MediaMetadata.METADATA_KEY_ART_URI,
            MediaMetadata.METADATA_KEY_DISPLAY_ICON_URI,
        ).firstNotNullOfOrNull { meta.getString(it)?.takeIf { s -> s.isNotBlank() } } ?: return null
        val u = Uri.parse(uri)
        if (u.scheme !in setOf("content", "file", "android.resource")) return null
        return try {
            app.contentResolver.openInputStream(u)?.use { android.graphics.BitmapFactory.decodeStream(it) }
        } catch (e: Exception) {
            null
        }
    }

    /** A player's media notification: keep its picture (the cover) for players that don't share it otherwise. */
    private fun mediaPicture(sbn: StatusBarNotification) {
        val icon = sbn.notification.getLargeIcon() ?: return
        val pkg = sbn.packageName
        val title = sbn.notification.extras?.getCharSequence(Notification.EXTRA_TITLE)?.toString().orEmpty()
        // Loading another app's picture can be slow: off the main thread.
        onMediaWorker {
            val small = try {
                val d = icon.loadDrawable(app)
                val bmp = (d as? android.graphics.drawable.BitmapDrawable)?.bitmap ?: d?.let {
                    Bitmap.createBitmap(160, 160, Bitmap.Config.ARGB_8888).also { b ->
                        it.setBounds(0, 0, 160, 160)
                        it.draw(android.graphics.Canvas(b))
                    }
                }
                bmp?.let { scaled(it) }
            } catch (e: Exception) {
                null
            } ?: return@onMediaWorker
            val colour = colourOf(small)
            post {
                mediaPictures[pkg] = title to small
                if (mediaPictures.size > 6) mediaPictures.remove(mediaPictures.keys.first { it != pkg })
                // The playing song has no cover of its own and this picture is for it: use it.
                val m = island.get("music") as? Live.Music
                if (controller?.packageName == pkg && !artFromSong && m != null && m.title == title) {
                    art = small
                    artColor = colour
                    changed()
                }
            }
        }
    }

    /** Paused music leaves once its time is up (not while its card is open). */
    private fun dropPausedMusic() {
        val since = pausedSince ?: return
        val m = island.get("music") as? Live.Music
        if (m == null || m.playing) {
            pausedSince = null
            return
        }
        if (now() - since >= PAUSE_LINGER && !island.isOpen) {
            island.remove("music")
            pausedSince = null
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

    /** The sleep timer button: off → 15 → 30 → 45 → 60 minutes → off. */
    fun stepSleep() {
        val left = sleepAt?.let { ((it - now() + 59_999) / 60_000).toInt() }
        sleepAt = Format.nextSleep(left)?.let { now() + it * 60_000L }
        changed()
    }

    fun setSleep(minutes: Int?) {
        sleepAt = minutes?.let { now() + it * 60_000L }
        changed()
    }

    fun sleepMinutesLeft(): Int? = sleepAt?.let { ((it - now() + 59_999) / 60_000).toInt().coerceAtLeast(0) }

    private fun checkSleep() {
        val at = sleepAt ?: return
        if (now() < at) return
        sleepAt = null
        val c = controller ?: return
        if (c.playbackState?.state == PlaybackState.STATE_PLAYING) {
            c.transportControls.pause()
            island.pop(Pop.Info("Sleep timer: paused"), now())
        }
    }

    // ---- Network speed ---------------------------------------------------------------------

    private var netAt = 0L
    private var netRx = -1L
    private var netTx = -1L

    /** Download and upload speed (bytes a second) across the whole phone, from the last two readings. */
    var netRate: Pair<Double, Double>? = null
        private set

    /** Read the phone's byte counters; call about once a second while the speed shows. */
    fun sampleNet() {
        val rx = TrafficStats.getTotalRxBytes()
        val tx = TrafficStats.getTotalTxBytes()
        val t = SystemClock.elapsedRealtime()
        if (rx < 0 || tx < 0) {
            netRate = null
            return
        }
        if (netRx >= 0 && t - netAt >= 500) {
            val dt = (t - netAt) / 1000.0
            netRate = ((rx - netRx).coerceAtLeast(0) / dt) to ((tx - netTx).coerceAtLeast(0) / dt)
        }
        if (netRx < 0 || t - netAt >= 500) {
            netRx = rx
            netTx = tx
            netAt = t
        }
    }

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
        // A finished focus round (not a break, not a plain timer) counts toward the streak.
        if (done == "Focus") prefs.streak = prefs.streak.record(Streak.dayOf(now()))
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

    // ---- Class timetable ----------------------------------------------------------------

    private fun updateLesson(timetable: Timetable) {
        val now = now()
        val current = timetable.current(now)
        if (current == null || !prefs.lessons) {
            island.retain("lesson:", emptySet())
        } else {
            val (l, begin) = current
            val id = "lesson:${l.day}:${l.start}"
            island.retain("lesson:", setOf(id))
            island.put(Live.Lesson(id, l.name, l.room, begin, begin + (l.end - l.start) * 60_000L))
        }
        // Do Not Disturb while a lesson is on, if asked.
        val inLesson = current != null && current.second <= now
        val listener = IsletListener.instance ?: return
        if (prefs.lessonQuiet && inLesson && !dnd && !quietForLesson) {
            quietForLesson = true
            listener.requestInterruptionFilter(NotificationListenerService.INTERRUPTION_FILTER_PRIORITY)
        } else if (quietForLesson && (!inLesson || !prefs.lessonQuiet)) {
            quietForLesson = false
            if (dnd) listener.requestInterruptionFilter(NotificationListenerService.INTERRUPTION_FILTER_ALL)
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
