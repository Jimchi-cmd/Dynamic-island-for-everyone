package io.github.jimchi_cmd.islet

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.os.SystemClock
import android.view.WindowManager
import islet.island.Live
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowSystemClock
import java.time.Duration

/**
 * The island's motion, frame by frame, as drawn: it grows and shrinks
 * smoothly, stays centred, and its contents never blink out mid-change.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w411dp-h891dp-xxhdpi")
class MotionTest {
    private val ctx = RuntimeEnvironment.getApplication()

    /** One drawn frame: the island's left and right edges (px) and how much of it is lit. */
    private data class Shot(val left: Int, val right: Int, val lit: Int)

    private fun shoot(v: IslandView, bmp: Bitmap): Shot {
        bmp.eraseColor(Color.rgb(200, 200, 200))
        v.draw(Canvas(bmp))
        var l = bmp.width
        var r = -1
        var lit = 0
        val row = IntArray(bmp.width)
        for (y in 0 until bmp.height step 3) {
            bmp.getPixels(row, 0, bmp.width, 0, y, bmp.width, 1)
            for (x in row.indices) {
                val c = row[x]
                val dark = Color.red(c) < 60 && Color.green(c) < 60 && Color.blue(c) < 60
                if (dark) {
                    if (x < l) l = x
                    if (x > r) r = x
                }
                // How bright the contents are (text, cover, bars) on the island's black:
                // a cross-fade keeps the total up; a blink drops it to nothing.
                if (y < bmp.height / 2 && Color.red(c) < 200) lit += maxOf(0, (Color.red(c) + Color.green(c) + Color.blue(c)) / 3 - 40)
            }
        }
        return Shot(l, r, lit)
    }

    @Test
    fun closing_the_music_card_shrinks_smoothly_centred_without_a_blink() {
        Isle.init(ctx)
        Isle.island.all.forEach { Isle.island.remove(it.id) }
        Isle.island.close(0)
        Isle.island.dismissPop(0)
        Isle.prefs.ring = false
        Isle.island.put(Live.Music("Song", "Band", "Player", true, 200_000, 0, SystemClock.elapsedRealtime()))
        val v = IslandWindow(ctx, WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY).view
        Isle.onChange = { v.update() }
        Isle.island.open("music")
        Isle.changed()
        v.settle()
        val dm = ctx.resources.displayMetrics
        val bmp = Bitmap.createBitmap(dm.widthPixels, (300 * dm.density).toInt(), Bitmap.Config.ARGB_8888)
        val open = shoot(v, bmp)
        Isle.island.close(Isle.now())
        Isle.changed()
        val shots = ArrayList<Shot>()
        repeat(45) {
            ShadowSystemClock.advanceBy(Duration.ofMillis(16))
            shots.add(shoot(v, bmp))
        }
        val end = shots.last()
        assertTrue("it shrank: ${open.right - open.left} → ${end.right - end.left}", end.right - end.left < (open.right - open.left) * 0.8)
        val centre = (open.left + open.right) / 2f
        for ((i, s) in shots.withIndex()) {
            assertTrue("frame $i: off centre (${(s.left + s.right) / 2f} vs $centre)", kotlin.math.abs((s.left + s.right) / 2f - centre) < 3 * dm.density)
            if (i > 0) {
                val grew = (s.right - s.left) - (shots[i - 1].right - shots[i - 1].left)
                assertTrue("frame $i: it grew again by $grew px (a bounce)", grew <= 2 * dm.density)
            }
        }
        // Something is always showing inside it: no blink to an empty island.
        val dimmest = shots.minOf { it.lit }
        assertTrue("contents blinked out (lit $dimmest, at the end ${end.lit})", dimmest >= minOf(open.lit, end.lit) * 0.35)
        Isle.onChange = null
        Isle.prefs.ring = true
    }
}
