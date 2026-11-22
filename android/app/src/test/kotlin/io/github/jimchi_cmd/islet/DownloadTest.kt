package io.github.jimchi_cmd.islet

import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.SystemClock
import android.view.WindowManager
import islet.island.Live
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowSystemClock
import java.io.File
import java.time.Duration

/** A download's ring glides to each new percentage; the battery shows its number. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w411dp-h891dp-xxhdpi")
class DownloadTest {
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
    fun the_ring_glides_to_a_new_percentage_instead_of_jumping() {
        val v = fresh()
        val bmp = Bitmap.createBitmap(10, 10, Bitmap.Config.ARGB_8888)
        Isle.island.put(Live.Progress("dl", "file.zip", "Files", 10))
        Isle.changed()
        v.settle()
        v.draw(Canvas(bmp))
        assertEquals(0.10f, v.ringShown("dl")!!, 0.001f)
        // The app reports a big step: the ring doesn't jump to it...
        Isle.island.put(Live.Progress("dl", "file.zip", "Files", 60))
        Isle.changed()
        val seen = ArrayList<Float>()
        repeat(60) {
            ShadowSystemClock.advanceBy(Duration.ofMillis(16))
            v.draw(Canvas(bmp))
            seen.add(v.ringShown("dl")!!)
        }
        assertTrue("it moved part way first: ${seen.take(3)}", seen[1] > 0.10f && seen[1] < 0.5f)
        assertTrue("always forward: $seen", seen.zipWithNext().all { (a, b) -> b >= a })
        // ...and arrives within a second.
        assertEquals(0.60f, seen.last(), 0.001f)
        Isle.island.remove("dl")
        Isle.onChange = null
    }

    @Test
    fun pictures_for_a_look() {
        // Set ISLET_SHOTS to a folder to save how the battery and a download look.
        val dir = System.getenv("ISLET_SHOTS") ?: return
        val v = fresh()
        val dm = ctx.resources.displayMetrics
        val bmp = Bitmap.createBitmap(dm.widthPixels, (90 * dm.density).toInt(), Bitmap.Config.ARGB_8888)
        fun shot(name: String) {
            v.update()
            v.settle()
            bmp.eraseColor(0xff2b2b33.toInt())
            v.draw(Canvas(bmp))
            File(dir).mkdirs()
            File(dir, "$name.png").outputStream().use { bmp.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
        Isle.prefs.idleClock = true
        for (level in listOf(100, 64, 15)) {
            Isle.battery = level
            shot("battery-$level")
        }
        Isle.island.put(Live.Progress("dl", "file.zip", "Files", 42))
        shot("download")
        Isle.island.remove("dl")
        // The big music card, with lyrics.
        val tall = Bitmap.createBitmap(dm.widthPixels, (380 * dm.density).toInt(), Bitmap.Config.ARGB_8888)
        Isle.island.put(Live.Music("Midnight Drive", "The Example Band", "Player", true, 215_000, 46_000, SystemClock.elapsedRealtime()))
        Isle.lyrics = islet.island.Lyrics.parse("[00:40.00]Headlights on the empty road\n[00:50.00]Every sign is glowing gold\n")
        Isle.artColor = 0xffe24070.toInt()
        Isle.island.open("music")
        v.update()
        v.settle()
        tall.eraseColor(0xff2b2b33.toInt())
        v.draw(Canvas(tall))
        File(dir, "music-card.png").outputStream().use { tall.compress(Bitmap.CompressFormat.PNG, 100, it) }
        // The control centre, opened from the music card.
        Isle.island.showControls()
        v.update()
        v.settle()
        tall.eraseColor(0xff2b2b33.toInt())
        v.draw(Canvas(tall))
        File(dir, "control-centre.png").outputStream().use { tall.compress(Bitmap.CompressFormat.PNG, 100, it) }
        Isle.island.close(0)
        Isle.island.remove("music")
        Isle.lyrics = null
        Isle.onChange = null
    }
}
