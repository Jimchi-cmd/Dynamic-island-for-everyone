package io.github.jimchi_cmd.islet

import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.SystemClock
import android.view.WindowManager
import islet.island.Live
import islet.island.Lyrics
import islet.island.Pop
import islet.island.Scene
import islet.island.Weather
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/** The laptop's extras on the phone, drawn: the lyrics row, the pop-ups, the looks. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w411dp-h891dp-xxhdpi")
class LooksTest {
    private val ctx = RuntimeEnvironment.getApplication()

    private fun fresh(): IslandView {
        Isle.init(ctx)
        Isle.island.all.forEach { Isle.island.remove(it.id) }
        Isle.island.close(0)
        Isle.island.dismissPop(0)
        val v = IslandWindow(ctx, WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY).view
        Isle.onChange = { v.update() }
        return v
    }

    @Test
    fun synced_lyrics_give_the_small_music_pill_a_row() {
        val v = fresh()
        Isle.prefs.lyrics = true
        Isle.island.put(Live.Music("Song", "Band", "Player", true, 200_000, 45_000, SystemClock.elapsedRealtime()))
        Isle.lyrics = null
        v.update()
        v.settle()
        val plain = v.drawnBox()!!
        Isle.lyrics = Lyrics.parse("[00:40.00]A line\n")
        v.update()
        v.settle()
        val withRow = v.drawnBox()!!
        assertTrue("taller with lyrics: $plain → $withRow", withRow.h > plain.h + 10)
        assertTrue("…from the same top", kotlin.math.abs(withRow.y - plain.y) < 0.5)
        Isle.lyrics = null
        Isle.prefs.lyrics = false
        Isle.island.remove("music")
        Isle.onChange = null
    }

    @Test
    fun pop_ups_grown_downward_stay_by_the_camera() {
        val v = fresh()
        Isle.prefs.popsDown = true
        Isle.island.pop(Pop.Charging(64, true), Isle.now())
        v.update()
        v.settle()
        val box = v.drawnBox()!!
        assertTrue("narrow, not across the status bar: ${box.w}", box.w <= 170)
        Isle.island.dismissPop(Isle.now())
        Isle.prefs.popsDown = false
        Isle.onChange = null
    }

    @Test
    fun pop_ups_both_ways() {
        // Set ISLET_SHOTS to a folder to save each pop-up sideways and grown downward.
        val dir = System.getenv("ISLET_SHOTS") ?: return
        val v = fresh()
        val dm = ctx.resources.displayMetrics
        File(dir).mkdirs()
        fun shot(name: String, heightDp: Int = 130) {
            v.update()
            v.settle()
            val bmp = Bitmap.createBitmap(dm.widthPixels, (heightDp * dm.density).toInt(), Bitmap.Config.ARGB_8888)
            bmp.eraseColor(0xff2b2b33.toInt())
            v.draw(Canvas(bmp))
            File(dir, "$name.png").outputStream().use { bmp.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
        val pops = listOf(
            "charging" to Pop.Charging(64, true),
            "silent" to Pop.Ringer(Pop.RingerMode.Silent),
            "wifi" to Pop.Wifi("Home"),
            "volume" to Pop.Level(Pop.LevelKind.Volume, 0.6f),
            "note" to Pop.Note("k", "Chat", "Sam", "Are you coming tonight?"),
        )
        for (down in listOf(false, true)) {
            Isle.prefs.popsDown = down
            for ((name, pop) in pops) {
                Isle.island.pop(pop, Isle.now())
                shot("pop-${if (down) "down" else "side"}-$name")
                while (Isle.island.scene(Isle.now()) is Scene.Popped) Isle.island.dismissPop(Isle.now())
            }
        }
        Isle.prefs.popsDown = false
        Isle.onChange = null
    }

    @Test
    fun pictures() {
        // Set ISLET_SHOTS to a folder to save how each looks.
        val dir = System.getenv("ISLET_SHOTS") ?: return
        val v = fresh()
        val dm = ctx.resources.displayMetrics
        File(dir).mkdirs()
        fun shot(name: String, heightDp: Int = 110) {
            v.update()
            v.settle()
            // Let anything that slides in (the lyrics line) arrive.
            repeat(30) {
                org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofMillis(16))
                v.draw(Canvas(Bitmap.createBitmap(10, 10, Bitmap.Config.ARGB_8888)))
            }
            val bmp = Bitmap.createBitmap(dm.widthPixels, (heightDp * dm.density).toInt(), Bitmap.Config.ARGB_8888)
            bmp.eraseColor(0xff2b2b33.toInt())
            v.draw(Canvas(bmp))
            File(dir, "$name.png").outputStream().use { bmp.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
        val p = Isle.prefs
        p.idleClock = true
        Isle.battery = 82
        // The small music pill with its lyrics row.
        p.lyrics = true
        Isle.artColor = 0xffe24070.toInt()
        Isle.island.put(Live.Music("Midnight Drive", "The Example Band", "Player", true, 215_000, 46_000, SystemClock.elapsedRealtime()))
        Isle.lyrics = Lyrics.parse("[00:40.00]Headlights on the empty road\n")
        shot("lyrics-pill")
        // Two things at once: the pill, the liquid neck, the bubble.
        Isle.lyrics = null
        p.lyrics = false
        Isle.island.put(Live.Timer("Focus", Isle.now() + 600_000, 1_500_000, null, false))
        shot("neck")
        Isle.island.all.forEach { Isle.island.remove(it.id) }
        // Pop-ups: volume, a device's battery, fully charged.
        Isle.island.pop(Pop.Level(Pop.LevelKind.Volume, 0.6f), Isle.now())
        shot("pop-volume")
        Isle.island.dismissPop(Isle.now())
        Isle.island.pop(Pop.Bluetooth("Galaxy Buds", true, 74), Isle.now())
        shot("pop-buds")
        Isle.island.dismissPop(Isle.now())
        Isle.island.pop(Pop.Charging(100, true), Isle.now())
        shot("pop-full")
        while (Isle.island.scene(Isle.now()) is Scene.Popped) Isle.island.dismissPop(Isle.now())
        // The strip with the weather.
        Isle.weather = Weather(24.0, 1, true, 27.0, 18.0)
        Isle.island.tap(Isle.now())
        shot("strip")
        Isle.island.close(Isle.now())
        Isle.island.dismissPop(0)
        // Looks: tinted, blue accent, notch, the day on the clock.
        p.style = "tinted"
        p.accent = "blue"
        p.notch = true
        p.clockFace = "day"
        shot("look-tinted-notch")
        p.style = "black"
        p.accent = "purple"
        p.notch = false
        p.clockFace = "seconds"
        shot("look-seconds")
        p.clockFace = "time"
        // An open timer card with its Control Centre button.
        Isle.island.put(Live.Timer("Focus", Isle.now() + 600_000, 1_500_000, null, false))
        Isle.island.open(Isle.island.all.first().id)
        shot("card-timer", 260)
        Isle.island.close(0)
        Isle.island.all.forEach { Isle.island.remove(it.id) }
        // The control centre, opened from the music card, as on the computer.
        Isle.weather = Weather(24.0, 2, true, 24.0, 16.0)
        Isle.wifiOn = true
        Isle.wifiName = "Turi Wi-Fi"
        Isle.btOn = false
        Isle.island.put(Live.Music("Song", "Band", "Player", true, 200_000, 0, SystemClock.elapsedRealtime()))
        Isle.island.open("music")
        Isle.island.showControls()
        shot("control-centre-laptop-style", 240)
        Isle.island.showToggles()
        shot("toggles", 320)
        Isle.island.close(0)
        Isle.island.all.forEach { Isle.island.remove(it.id) }
        Isle.weather = null
        Isle.onChange = null
    }
}
