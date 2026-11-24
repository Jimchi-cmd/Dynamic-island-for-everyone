package io.github.jimchi_cmd.islet

import android.app.PendingIntent
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import android.os.SystemClock
import android.view.WindowManager
import islet.island.CalendarEvent
import islet.island.Live
import islet.island.Lyrics
import islet.island.Pop
import islet.island.Streak
import islet.island.Timetable
import islet.island.Weather
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowLooper
import java.io.File
import java.util.Calendar
import java.util.concurrent.TimeUnit

/**
 * Frames for the promo videos, drawn by the island's own code with its
 * real springs and timings, 30 a second. Off unless ISLET_PROMO is set:
 *
 *     ISLET_PROMO=1 gradle :app:testDebugUnitTest --tests '*PromoFrames*'
 *
 * then scripts/promo/make-videos.py lays them over a phone screen with
 * captions and encodes each video.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w360dp-h780dp-xxhdpi")
class PromoFrames {
    private val ctx = RuntimeEnvironment.getApplication()
    private val fps = 30

    /** One video: how long it runs, and what happens when (seconds). */
    private class Reel(val name: String, val seconds: Double, val steps: List<Pair<Double, () -> Unit>>)

    private var start = 0L
    private fun now() = Isle.now()

    private fun cover(a: Int, b: Int): Bitmap {
        val bmp = Bitmap.createBitmap(160, 160, Bitmap.Config.ARGB_8888)
        val p = Paint().apply { shader = LinearGradient(0f, 0f, 160f, 160f, a, b, Shader.TileMode.CLAMP) }
        Canvas(bmp).drawRect(0f, 0f, 160f, 160f, p)
        return bmp
    }

    private fun reset() {
        Isle.init(ctx)
        val i = Isle.island
        i.all.forEach { i.remove(it.id) }
        i.close(0)
        i.dismissPop(0)
        Isle.extras.clear()
        Isle.art = null
        Isle.artColor = null
        Isle.lyrics = null
        Isle.weather = Weather(24.0, 1, true, 27.0, 17.0)
        Isle.airplane = false
        Isle.dnd = false
        Isle.battery = 84
        Isle.charging = false
        Isle.wifiOn = true
        Isle.wifiName = "Home Wi-Fi"
        Isle.btOn = true
        Isle.btName = null
        Isle.events = emptyList()
        Isle.deadlines = emptyList()
        Isle.nextAlarm = null
        Isle.focus.stop()
        Isle.stopwatch.reset()
        Isle.setSleep(null)
        val p = Isle.prefs
        p.style = "black"
        p.ring = true
        p.idleClock = true
        p.lyrics = false
        p.timetable = Timetable()
        p.stripNetwork = false
        p.streak = Streak()
        p.worldClocks = emptyList()
        p.ccStyle = "classic"
        p.ccPhoto = false
        p.tint = ""
        p.noteLength = "normal"
        T.load(ctx, "en")
        // A fixed morning, so every video shows the same time of day.
        val c = Calendar.getInstance().apply {
            set(Calendar.HOUR_OF_DAY, 10)
            set(Calendar.MINUTE, 8)
            set(Calendar.SECOND, 0)
        }
        start = c.timeInMillis
        Isle.clock = { start }
    }

    private fun music(title: String = "Midnight Drive", artist: String = "The Example Band", at: Long = 42_000) =
        Live.Music(title, artist, "Player", true, 215_000, at, SystemClock.elapsedRealtime())

    private fun song() {
        Isle.island.put(music())
        Isle.art = cover(0xffe0446a.toInt(), 0xff5b2ad6.toInt())
        Isle.artColor = 0xe0446a
    }

    private val pi get() = PendingIntent.getActivity(ctx, 0, Intent(), PendingIntent.FLAG_IMMUTABLE)

    private fun reels(): List<Reel> = listOf(
        Reel("01-music", 13.0, listOf(
            1.0 to { song() },
            4.0 to {
                Isle.prefs.lyrics = true
                Isle.lyrics = Lyrics.parse(
                    "[00:42.00]Headlights on the empty road\n[00:44.60]Every sign is glowing gold\n" +
                        "[00:47.20]Turn it up and let it go\n[00:49.80]We're nearly home",
                )
                Isle.island.open("music")
            },
            11.0 to { Isle.island.close(now()) },
        )),
        Reel("02-calls", 11.0, listOf(
            1.0 to {
                Isle.extras["c"] = NoteExtra("com.phone", pi, listOf("Decline" to pi, "Answer" to pi), null)
                Isle.island.put(Live.Call("note:c", "Mum", "Phone", true, 0))
                Isle.island.open("note:c")
            },
            5.0 to {
                Isle.island.put(Live.Call("note:c", "Mum", "Phone", false, now()))
                Isle.island.close(now())
            },
            8.0 to { Isle.island.open("note:c") },
        )),
        Reel("03-notifications", 12.0, listOf(
            1.0 to {
                Isle.extras["m"] = NoteExtra("com.chat", pi, emptyList(), null)
                Isle.island.pop(Pop.Note("m", "Chat", "Anna", "Are you coming tonight?", listOf("Reply", "Mark as read"), 1), now())
            },
            4.2 to { Isle.island.pop(Pop.Note("m", "Chat", "Anna", "Bring the speaker!", listOf("Reply", "Mark as read"), 2), now()) },
            7.4 to { Isle.island.pop(Pop.Payment("Coffee Corner", "$4.50"), now()) },
            9.6 to { Isle.island.pop(Pop.Bluetooth("Buds Pro", true), now()) },
        )),
        Reel("04-tap-the-island", 13.0, listOf(
            1.2 to { Isle.island.tap(now()) },
            3.4 to { Isle.island.tap(now()) },
            6.6 to { Isle.island.showCalendar() },
            9.6 to {
                Isle.startFocus(25.0)
                Isle.island.close(now())
            },
        )),
        Reel("05-control-centre", 11.0, listOf(
            1.2 to { Isle.island.longPress() },
            5.6 to { Isle.island.close(now()) },
            6.8 to { Isle.dnd = true; Isle.island.pop(Pop.DoNotDisturb(true), now()) },
            9.0 to { Isle.island.pop(Pop.Ringer(Pop.RingerMode.Silent), now()) },
        )),
        Reel("06-students", 13.0, listOf(
            0.5 to {
                val m = Timetable.minuteOfDay(now())
                Isle.prefs.timetable = Timetable().add(Timetable.weekday(now()), m + 6, m + 56, "Maths", "B12").getOrThrow()
                Isle.changed()
            },
            3.0 to { Isle.island.open(Isle.island.all.first().id) },
            6.4 to { Isle.island.close(now()) },
            7.6 to {
                Isle.deadlines = listOf(CalendarEvent(2, "Biology essay due", now() + 3 * 86_400_000L, now() + 3 * 86_400_000L + 3_600_000, false))
                Isle.events = listOf(CalendarEvent(1, "Study group", now() + 5 * 3_600_000L, now() + 6 * 3_600_000L, false))
                Isle.nextAlarm = now() + 20 * 3_600_000L
                Isle.island.showCalendar()
            },
        )),
        Reel("07-directions", 11.0, listOf(
            0.8 to { Isle.island.put(Live.Navigation("note:n", "200 m", "Turn left onto Market Street", "Maps")) },
            3.2 to { Isle.island.open("note:n") },
            6.8 to { Isle.island.close(now()) },
            7.8 to { song() },
            9.6 to { Isle.island.cycle() },
        )),
        Reel("08-focus-streak", 11.0, listOf(
            0.8 to {
                Isle.prefs.streak = Streak(islet.island.Streak.dayOf(now()) - 1, 6, 6, 31, 1)
                Isle.startPomodoro()
            },
            3.0 to { Isle.island.open("timer") },
            7.0 to { Isle.island.close(now()) },
            8.4 to { Isle.island.pop(Pop.TimerDone("Focus done · Break now"), now()) },
        )),
        Reel("09-quick-tour", 12.0, listOf(
            0.6 to { song() },
            1.6 to { Isle.island.open("music") },
            3.0 to { Isle.island.close(now()); Isle.island.remove("music") },
            3.6 to { Isle.island.pop(Pop.Charging(64, true), now()) },
            5.0 to { Isle.island.pop(Pop.Note("q", "Chat", "Sam", "See you at 6!", emptyList(), 1), now()) },
            6.6 to { Isle.island.dismissPop(now()); Isle.island.longPress() },
            8.4 to { Isle.island.close(now()); Isle.island.showCalendar() },
            10.2 to { Isle.island.close(now()) },
        )),
    )

    /** "Islet in real life": a class, a commute, the gym. */
    private fun life(): List<Reel> = listOf(
        Reel("life-class", 16.0, listOf(
            0.3 to {
                val m = Timetable.minuteOfDay(now())
                Isle.prefs.timetable = Timetable().add(Timetable.weekday(now()), m + 6, m + 56, "Maths", "B12").getOrThrow()
                Isle.changed()
            },
            2.6 to { Isle.island.open(Isle.island.all.first().id) },
            5.4 to { Isle.island.close(now()) },
            6.0 to { Isle.dnd = true; Isle.island.pop(Pop.DoNotDisturb(true), now()) },
            8.2 to {
                Isle.deadlines = listOf(CalendarEvent(2, "Biology essay due", now() + 3 * 86_400_000L, now() + 3 * 86_400_000L + 3_600_000, false))
                Isle.events = listOf(CalendarEvent(1, "Study group", now() + 5 * 3_600_000L, now() + 6 * 3_600_000L, false))
                Isle.island.showCalendar()
            },
            11.6 to {
                Isle.island.close(now())
                Isle.prefs.timetable = Timetable()
                Isle.prefs.streak = Streak(Streak.dayOf(now()) - 1, 6, 6, 31, 1)
                Isle.startPomodoro()
            },
            13.2 to { Isle.island.open("timer") },
        )),
        Reel("life-commute", 16.0, listOf(
            0.5 to { Isle.island.put(Live.Navigation("note:n", "200 m", "Turn left onto Market Street", "Maps")) },
            2.4 to { Isle.island.open("note:n") },
            5.0 to { Isle.island.close(now()) },
            5.6 to { song() },
            7.6 to {
                Isle.extras["c"] = NoteExtra("com.phone", pi, listOf("Decline" to pi, "Answer" to pi), null)
                Isle.island.put(Live.Call("note:c", "Alex", "Phone", true, 0))
                Isle.island.open("note:c")
            },
            10.6 to {
                Isle.island.put(Live.Call("note:c", "Alex", "Phone", false, now()))
                Isle.island.close(now())
            },
            13.2 to { Isle.island.cycle() },
        )),
        Reel("life-gym", 16.0, listOf(
            0.5 to {
                Isle.island.put(music("Run It Up", "The Example Band", 61_000))
                Isle.art = cover(0xff22c55e.toInt(), 0xff0ea5e9.toInt())
                Isle.artColor = 0x22c55e
            },
            2.0 to { Isle.island.pop(Pop.Bluetooth("Buds Pro", true), now()) },
            4.2 to {
                Isle.prefs.lyrics = true
                Isle.lyrics = Lyrics.parse(
                    "[01:04.00]One more rep, don't stop now\n[01:06.40]Feel the beat, we're breaking out\n" +
                        "[01:08.80]Push it up, push it loud\n[01:11.20]Run it up",
                )
                Isle.island.open("music")
            },
            7.4 to { Isle.island.close(now()) },
            8.0 to {
                Isle.stopwatch.reset()
                Isle.stopwatch.start(now())
                Isle.syncTimers()
                Isle.island.open("stopwatch")
            },
            9.6 to { Isle.stopwatch.lap(now()); Isle.syncTimers() },
            11.0 to { Isle.stopwatch.lap(now()); Isle.syncTimers() },
            12.2 to { Isle.stopwatch.pause(now()); Isle.syncTimers(); Isle.island.close(now()) },
            12.8 to { Isle.startFocus(0.04, "Rest") },
        )),
    )

    /** A made-up photo for Control Centre: a dusk sky over hills. */
    private fun photo() {
        val bmp = Bitmap.createBitmap(900, 600, Bitmap.Config.ARGB_8888)
        val c = Canvas(bmp)
        val sky = Paint().apply {
            shader = LinearGradient(0f, 0f, 0f, 600f, intArrayOf(0xff283c8c.toInt(), 0xffe6785a.toInt(), 0xfffac878.toInt()), floatArrayOf(0f, 0.6f, 1f), Shader.TileMode.CLAMP)
        }
        c.drawRect(0f, 0f, 900f, 600f, sky)
        val hill = android.graphics.Path().apply {
            moveTo(0f, 600f)
            lineTo(0f, 430f)
            quadTo(260f, 330f, 520f, 450f)
            quadTo(720f, 380f, 900f, 420f)
            lineTo(900f, 600f)
            close()
        }
        c.drawPath(hill, Paint().apply { color = 0xff1e1932.toInt(); isAntiAlias = true })
        File(ctx.filesDir, IslandView.CC_PHOTO).outputStream().use { bmp.compress(Bitmap.CompressFormat.JPEG, 92, it) }
    }

    /** What's new: Control Centre's layouts, a photo, a tint, a language, shorter pop-ups. */
    private fun features(): List<Reel> = listOf(
        Reel("new-phone", 16.0, listOf(
            0.4 to { Isle.island.longPress() },
            2.8 to { Isle.island.close(now()); Isle.prefs.ccStyle = "modules" },
            3.2 to { Isle.island.longPress() },
            5.6 to { Isle.island.close(now()); Isle.prefs.ccStyle = "sliders" },
            6.0 to { Isle.island.longPress() },
            8.4 to {
                Isle.island.close(now())
                photo()
                Isle.prefs.ccStyle = "dock"
                Isle.prefs.ccPhoto = true
            },
            8.8 to { Isle.island.longPress() },
            11.2 to {
                Isle.island.close(now())
                Isle.prefs.ccPhoto = false
                Isle.prefs.ccStyle = "modules"
                Isle.prefs.style = "tinted"
                Isle.prefs.tint = "#d6336c"
                T.load(ctx, "es")
            },
            11.6 to { Isle.island.longPress() },
            13.8 to { Isle.island.close(now()); T.load(ctx, "en"); Isle.prefs.style = "black"; Isle.prefs.noteLength = "brief" },
            14.0 to { Isle.island.pop(Pop.Note("q", "Chat", "Sam", "See you at 6!", emptyList(), 1), now()) },
        )),
    )

    @Test
    fun render() {
        assumeTrue("set ISLET_PROMO to draw the promo frames", System.getenv("ISLET_PROMO") != null)
        val only = System.getenv("ISLET_PROMO").takeIf { it.length > 1 }
        for (reel in (reels() + life() + features()).filter { only == null || it.name.startsWith(only) }) draw(reel)
    }

    private fun draw(reel: Reel) {
        reset()
        val out = File("build/promo/${reel.name}").apply { deleteRecursively(); mkdirs() }
        val w = IslandWindow(ctx, WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY)
        val v = w.view
        Isle.onChange = { v.update() }
        Isle.changed()
        v.update()
        v.settle()
        val dm = ctx.resources.displayMetrics
        val bmp = Bitmap.createBitmap(dm.widthPixels, (400 * dm.density).toInt(), Bitmap.Config.ARGB_8888)
        val c = Canvas(bmp)
        val steps = reel.steps.toMutableList()
        val frames = (reel.seconds * fps).toInt()
        var shown = 0L
        for (i in 0 until frames) {
            val t = i.toDouble() / fps
            while (steps.isNotEmpty() && steps.first().first <= t) {
                steps.removeAt(0).second()
                Isle.changed()
            }
            // Move every clock on by one frame: the springs, the bars, the timers.
            val ms = (i + 1) * 1000L / fps - shown
            shown += ms
            ShadowLooper.idleMainLooper(ms, TimeUnit.MILLISECONDS)
            Isle.clock = { start + shown }
            if (i % fps == 0) Isle.changed()
            bmp.eraseColor(Color.TRANSPARENT)
            v.draw(c)
            File(out, "f%04d.png".format(i)).outputStream().use { bmp.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
        Isle.onChange = null
    }
}
