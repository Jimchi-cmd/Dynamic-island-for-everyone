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
    fun a_setting_that_changes_the_size_moves_the_island_to_it() {
        Isle.init(ctx)
        Isle.island.all.forEach { Isle.island.remove(it.id) }
        Isle.island.close(0)
        Isle.island.dismissPop(0)
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
