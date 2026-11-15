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
import org.robolectric.RuntimeEnvironment
import islet.island.CalendarEvent
import islet.island.Live
import islet.island.Lyrics
import islet.island.Pop
import islet.island.Streak
import islet.island.Timetable
import islet.island.Todo
import islet.island.Weather
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * Draws every kind of island scene off-device and saves the pictures in
 * build/renders, one sheet with them all, so a change to the drawing can be
 * looked at without a phone. Each scene must also draw something.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w411dp-h891dp-xxhdpi")
class IslandRenderTest {
    private val ctx = RuntimeEnvironment.getApplication()

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
        Isle.weather = null
        Isle.airplane = false
        Isle.dnd = false
        Isle.prefs.style = "black"
        Isle.prefs.timetable = Timetable()
        Isle.prefs.stripNetwork = false
        Isle.prefs.streak = Streak()
        Isle.deadlines = emptyList()
        Isle.events = emptyList()
        Isle.nextAlarm = null
        Isle.setSleep(null)
        Isle.prefs.lyrics = false
    }

    /** A lesson today from `from` to `to` minutes from now. */
    private fun lessonAround(from: Int, to: Int, name: String = "Physics", room: String = "Lab 2"): Timetable {
        val now = Isle.now()
        val start = Timetable.minuteOfDay(now) + from
        return Timetable().add(Timetable.weekday(now), start.coerceIn(0, 1438), (Timetable.minuteOfDay(now) + to).coerceIn(1, 1439), name, room).getOrThrow()
    }

    private fun cover(): Bitmap {
        val b = Bitmap.createBitmap(160, 160, Bitmap.Config.ARGB_8888)
        val c = Canvas(b)
        val p = Paint().apply { shader = LinearGradient(0f, 0f, 160f, 160f, 0xffe0446a.toInt(), 0xff5b2ad6.toInt(), Shader.TileMode.CLAMP) }
        c.drawRect(0f, 0f, 160f, 160f, p)
        return b
    }

    private fun render(name: String, setup: () -> Unit): Bitmap {
        reset()
        setup()
        val w = IslandWindow(ctx, WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY)
        val v = w.view
        v.update()
        v.settle()
        val dm = ctx.resources.displayMetrics
        val out = Bitmap.createBitmap(dm.widthPixels, (400 * dm.density).toInt(), Bitmap.Config.ARGB_8888)
        val c = Canvas(out)
        c.drawColor(0xffd9dde6.toInt())
        v.draw(c)
        v.draw(c)
        File("build/renders").mkdirs()
        File("build/renders/$name.png").outputStream().use { out.compress(Bitmap.CompressFormat.PNG, 100, it) }
        return out
    }

    private fun drewSomething(b: Bitmap): Boolean {
        var dark = 0
        // Anything clearly darker than the grey background is the island.
        for (y in 0 until b.height step 4) for (x in 0 until b.width step 4) if (Color.red(b.getPixel(x, y)) < 120) dark++
        return dark > 20
    }

    @Test
    fun every_scene_draws() {
        val now = System.currentTimeMillis()
        val pi = PendingIntent.getActivity(ctx, 0, Intent(), PendingIntent.FLAG_IMMUTABLE)
        val music = Live.Music("Midnight Drive", "The Example Band", "Player", true, 215_000, 64_000, SystemClock.elapsedRealtime())
        val scenes = linkedMapOf<String, () -> Unit>(
            "01-idle" to { Isle.airplane = true; Isle.dnd = true },
            "02-music" to { Isle.island.put(music); Isle.art = cover(); Isle.artColor = 0xe0446a },
            "03-call-and-music" to {
                Isle.island.put(music)
                Isle.island.put(Live.Call("note:c", "Mum", "Phone", false, now - 83_000))
            },
            "04-timer" to { Isle.startFocus(25.0) },
            "05-notification" to { Isle.island.pop(Pop.Note("k", "Messages", "Ann", "Are you coming to the match tonight? Bring the tickets"), Isle.now()) },
            "06-charging" to { Isle.island.pop(Pop.Charging(64, true), Isle.now()) },
            "07-silent" to { Isle.island.pop(Pop.Ringer(Pop.RingerMode.Silent), Isle.now()) },
            "08-music-open" to {
                Isle.island.put(music)
                Isle.art = cover()
                Isle.artColor = 0xe0446a
                Isle.lyrics = Lyrics.parse("[00:00.00]And the city lights go by")
                Isle.island.open("music")
            },
            "09-clock-card" to {
                Isle.weather = Weather(23.4, 2, true, 27.0, 18.0)
                Isle.prefs.worldClocks = listOf("Europe/London", "Asia/Tokyo")
                Isle.prefs.todos = listOf(Todo("Finish the essay", false))
                Isle.wifiOn = true
                Isle.wifiName = "Turi Wireless"
                Isle.btOn = true
                Isle.island.tap(Isle.now())
                Isle.island.tap(Isle.now())
            },
            "10-timer-open" to { Isle.startFocus(25.0); Isle.island.open("timer") },
            "11-stopwatch" to {
                Isle.stopwatch.reset()
                Isle.stopwatch.start(now - 75_000)
                Isle.stopwatch.lap(now - 40_000)
                Isle.syncTimers()
                Isle.island.open("stopwatch")
            },
            "12-meeting" to {
                Isle.events = listOf(CalendarEvent(1, "Team standup", now + 4 * 60_000, now + 30 * 60_000, false, "https://meet.google.com/abc-defg-hij"))
                Isle.changed()
                Isle.island.open("event:1")
            },
            "13-incoming-call" to {
                Isle.extras["c"] = NoteExtra("com.phone", pi, listOf("Decline" to pi, "Answer" to pi), null)
                Isle.island.put(Live.Call("note:c", "Mum", "Phone", true, 0))
                Isle.island.open("note:c")
            },
            "14-controls" to { Isle.island.longPress() },
            "15-directions" to {
                Isle.island.put(Live.Navigation("note:n", "200 m", "Turn left onto Kampala Road", "Maps"))
                Isle.island.open("note:n")
            },
            "16-download" to { Isle.island.put(Live.Progress("note:d", "photos.zip", "Files", 42)) },
            "17-low-battery" to { Isle.island.pop(Pop.LowBattery(9), Isle.now()) },
            "18-bluetooth" to { Isle.island.pop(Pop.Bluetooth("Buds Pro", true), Isle.now()) },
            "19-message-with-buttons" to {
                Isle.extras["m"] = NoteExtra("com.chat", pi, emptyList(), null)
                Isle.island.pop(Pop.Note("m", "Chat", "Anna", "Hey! Look how the island looks now", listOf("Reply", "Mark as read"), 3), Isle.now())
            },
            "20-paid" to { Isle.island.pop(Pop.Payment("Coffee Corner", "\$12.50"), Isle.now()) },
            "21-dnd" to { Isle.island.pop(Pop.DoNotDisturb(true), Isle.now()) },
            "22-glass-music" to {
                Isle.prefs.style = "glass"
                Isle.island.put(music)
                Isle.art = cover()
                Isle.artColor = 0xe0446a
            },
            "24-peek" to { Isle.battery = 61; Isle.island.tap(Isle.now()) },
            "25-calendar" to { Isle.island.showCalendar() },
            "23-call-ongoing" to { Isle.prefs.style = "black"; Isle.island.put(Live.Call("note:c", "Mum", "Phone", false, Isle.now() - 61_000)) },
            "26-lesson-on" to { Isle.prefs.timetable = lessonAround(-10, 40); Isle.changed() },
            "27-lesson-soon" to {
                Isle.prefs.timetable = lessonAround(8, 58, "Maths", "B12")
                Isle.changed()
                Isle.island.open(Isle.island.all.first().id)
            },
            "28-calendar-more" to {
                Isle.prefs.timetable = lessonAround(25, 75, "History", "")
                Isle.events = listOf(CalendarEvent(1, "Team standup", now + 3 * 3_600_000, now + 4 * 3_600_000, false))
                Isle.deadlines = listOf(CalendarEvent(2, "Biology essay due", now + 3 * 86_400_000L, now + 3 * 86_400_000L + 3_600_000, false))
                Isle.nextAlarm = now + 9 * 3_600_000
                Isle.island.showCalendar()
            },
            "29-music-sleep" to {
                Isle.island.put(music)
                Isle.art = cover()
                Isle.artColor = 0xe0446a
                Isle.setSleep(30)
                Isle.island.open("music")
            },
            "30-focus-streak" to {
                Isle.prefs.streak = Streak(Streak.dayOf(Isle.now()) - 1, 4, 6, 19, 2)
                Isle.startPomodoro()
                Isle.island.open("timer")
            },
            "32-idle-clock" to { Isle.battery = 78; Isle.charging = false },
            "33-music-lyrics" to {
                Isle.island.put(music)
                Isle.art = cover()
                Isle.artColor = 0xe0446a
                Isle.prefs.lyrics = true
                Isle.lyrics = Lyrics.parse("[00:00.00]Already rich, I don't need a thing")
                Isle.island.open("music")
            },
            "31-peek-network" to { Isle.battery = 61; Isle.prefs.stripNetwork = true; Isle.island.tap(Isle.now()) },
        )
        val shots = scenes.map { (name, setup) -> name to render(name, setup) }
        for ((name, b) in shots) assertTrue("$name drew nothing", drewSomething(b))
        // One sheet with them all.
        val w = shots[0].second.width
        val h = shots[0].second.height
        val sheet = Bitmap.createBitmap(w * 3, h * ((shots.size + 2) / 3), Bitmap.Config.ARGB_8888)
        val c = Canvas(sheet)
        c.drawColor(Color.WHITE)
        val label = Paint().apply { textSize = 36f; color = Color.DKGRAY }
        shots.forEachIndexed { i, (name, b) ->
            val x = (i % 3) * w.toFloat()
            val y = (i / 3) * h.toFloat()
            c.drawBitmap(b, x, y, null)
            c.drawText(name, x + 20, y + h - 20, label)
        }
        File("build/renders/sheet.png").outputStream().use { sheet.compress(Bitmap.CompressFormat.PNG, 100, it) }
        Isle.focus.stop()
        Isle.stopwatch.reset()
    }
}
