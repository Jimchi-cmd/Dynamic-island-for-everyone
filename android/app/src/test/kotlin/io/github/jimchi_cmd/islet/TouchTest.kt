package io.github.jimchi_cmd.islet

import android.os.SystemClock
import android.view.MotionEvent
import android.view.WindowManager
import islet.island.Live
import islet.island.Scene
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** Taps on the small pill open the big card, even when the thumb slides a little. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w411dp-h891dp-xxhdpi")
class TouchTest {
    private val ctx = RuntimeEnvironment.getApplication()
    private val dp = ctx.resources.displayMetrics.density

    private fun view(): IslandView {
        Isle.init(ctx)
        Isle.island.all.forEach { Isle.island.remove(it.id) }
        Isle.island.close(0)
        Isle.island.dismissPop(0)
        Isle.island.put(Live.Music("Song", "Band", "Player", true, 200_000, 0, SystemClock.elapsedRealtime()))
        val v = IslandWindow(ctx, WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY).view
        // As in the app: the view follows every change.
        Isle.onChange = { v.update() }
        v.update()
        v.settle()
        return v
    }

    private fun touch(v: IslandView, x0: Float, y0: Float, x1: Float, y1: Float, ms: Long) {
        val t = SystemClock.uptimeMillis()
        v.dispatchTouchEvent(MotionEvent.obtain(t, t, MotionEvent.ACTION_DOWN, x0, y0, 0))
        v.dispatchTouchEvent(MotionEvent.obtain(t, t + ms / 2, MotionEvent.ACTION_MOVE, (x0 + x1) / 2, (y0 + y1) / 2, 0))
        v.dispatchTouchEvent(MotionEvent.obtain(t, t + ms, MotionEvent.ACTION_UP, x1, y1, 0))
    }

    @Test
    fun a_quick_tap_that_slides_a_little_still_opens_the_card() {
        val v = view()
        assertTrue(Isle.island.scene(Isle.now()) is Scene.Compact)
        // A quick thumb tap that slides 14 dp sideways: Android's own tap would miss it.
        touch(v, 100 * dp, 30 * dp, 114 * dp, 32 * dp, 60)
        val s = Isle.island.scene(Isle.now())
        assertTrue("opened: $s", s is Scene.Expanded)
    }

    @Test
    fun quick_repeated_taps_open_the_card_and_leave_it_open() {
        val v = view()
        // Three quick taps (a thumb that taps again because nothing seemed to happen).
        for (k in 0 until 3) {
            touch(v, 100 * dp, 30 * dp, 102 * dp, 31 * dp, 40)
            org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofMillis(90))
        }
        assertTrue("open after quick taps: ${Isle.island.scene(Isle.now())}", Isle.island.scene(Isle.now()) is Scene.Expanded)
        // A later tap on the open card keeps it open; a swipe up closes it.
        org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofMillis(600))
        touch(v, 100 * dp, 60 * dp, 101 * dp, 60 * dp, 40)
        assertTrue(Isle.island.isOpen)
        org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofMillis(600))
        touch(v, 100 * dp, 120 * dp, 100 * dp, 40 * dp, 90)
        assertEquals(false, Isle.island.isOpen)
    }

    @Test
    fun a_setting_that_changes_the_size_moves_the_island_to_it() {
        Isle.init(ctx)
        Isle.island.all.forEach { Isle.island.remove(it.id) }
        Isle.island.close(0)
        Isle.island.dismissPop(0)
        // The sideways layout: the clock beside the camera widens the pill.
        Isle.prefs.popsDown = false
        Isle.prefs.idleClock = true
        val w = IslandWindow(ctx, WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY)
        val v = w.view
        v.update()
        v.settle()
        val wide = v.drawnBox()!!.w
        Isle.prefs.idleClock = false
        v.update()
        v.settle()
        val narrow = v.drawnBox()!!.w
        assertTrue("the pill narrows to the new size ($wide → $narrow)", narrow < wide - 40)
        assertEquals(w.geometry().box(Scene.Idle), v.drawnBox())
        Isle.prefs.idleClock = true
        Isle.prefs.popsDown = true
    }

    @Test
    fun over_the_status_bar_the_drawing_window_never_moves_and_taps_still_land() {
        Isle.init(ctx)
        Isle.island.all.forEach { Isle.island.remove(it.id) }
        Isle.island.close(0)
        Isle.island.dismissPop(0)
        Isle.island.put(Live.Music("Song", "Band", "Player", true, 200_000, 0, SystemClock.elapsedRealtime()))
        val w = IslandWindow(ctx, WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY)
        w.attach()
        val v = w.view
        Isle.onChange = { v.update() }
        v.update()
        v.settle()
        v.draw(android.graphics.Canvas(android.graphics.Bitmap.createBitmap(10, 10, android.graphics.Bitmap.Config.ARGB_8888)))
        val drawn = w.drawFrame()
        val small = w.touchFrame()
        // Open the card through the touch window, as a finger would.
        val t = SystemClock.uptimeMillis()
        val tv = w.touchView
        val cx = small[2] / 2f
        val cy = small[3] / 3f
        tv.dispatchTouchEvent(MotionEvent.obtain(t, t, MotionEvent.ACTION_DOWN, cx, cy, 0))
        tv.dispatchTouchEvent(MotionEvent.obtain(t, t + 50, MotionEvent.ACTION_UP, cx, cy, 0))
        assertTrue("opened by the touch window", Isle.island.isOpen)
        v.settle()
        v.draw(android.graphics.Canvas(android.graphics.Bitmap.createBitmap(10, 10, android.graphics.Bitmap.Config.ARGB_8888)))
        val big = w.touchFrame()
        assertTrue("the touch window grew with the card: $small → $big", big[3] > small[3] && big[2] > small[2])
        assertEquals("the drawing window didn't move", drawn.take(2), w.drawFrame().take(2))
        Isle.island.close(Isle.now())
        Isle.changed()
        v.settle()
        v.draw(android.graphics.Canvas(android.graphics.Bitmap.createBitmap(10, 10, android.graphics.Bitmap.Config.ARGB_8888)))
        assertEquals("…nor when it shrank back", drawn.take(2), w.drawFrame().take(2))
        w.detach()
        Isle.onChange = null
    }

    @Test
    fun as_an_overlay_the_window_corner_never_moves_as_the_island_grows_and_shrinks() {
        Isle.init(ctx)
        Isle.island.all.forEach { Isle.island.remove(it.id) }
        Isle.island.close(0)
        Isle.island.dismissPop(0)
        Isle.island.put(Live.Music("Song", "Band", "Player", true, 200_000, 0, SystemClock.elapsedRealtime()))
        val w = IslandWindow(ctx, WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY)
        w.attach()
        val v = w.view
        Isle.onChange = { v.update() }
        val bmp = android.graphics.Bitmap.createBitmap(10, 10, android.graphics.Bitmap.Config.ARGB_8888)
        fun frameAfter(step: () -> Unit): List<Int> {
            step()
            // Through the change frame by frame, as the screen draws it.
            repeat(40) {
                org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofMillis(16))
                v.draw(android.graphics.Canvas(bmp))
            }
            return w.frame()
        }
        val small = frameAfter { v.update() }
        val corner = small.take(2)
        val open = frameAfter { Isle.island.open("music"); Isle.changed() }
        assertTrue("the window grew for the card: $small → $open", open[2] > small[2] && open[3] > small[3])
        assertEquals("…without moving its corner", corner, open.take(2))
        val shut = frameAfter { Isle.island.close(Isle.now()); Isle.changed() }
        assertEquals("…nor when it shrank back", corner, shut.take(2))
        assertTrue("it shrank back: $open → $shut", shut[3] < open[3])
        val popped = frameAfter { Isle.pop(islet.island.Pop.Info("Bluetooth on")) }
        assertEquals("…nor for a pop-up", corner, popped.take(2))
        w.detach()
        Isle.onChange = null
    }

    @Test
    fun a_real_swipe_up_closes_and_is_not_a_tap() {
        val v = view()
        Isle.island.tap(Isle.now())
        assertTrue(Isle.island.isOpen)
        touch(v, 100 * dp, 120 * dp, 100 * dp, 40 * dp, 90)
        assertEquals(false, Isle.island.isOpen)
    }
}
