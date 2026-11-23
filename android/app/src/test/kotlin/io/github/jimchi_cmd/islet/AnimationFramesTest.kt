package io.github.jimchi_cmd.islet

import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.SystemClock
import android.view.WindowManager
import islet.island.Live
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowSystemClock
import java.io.File
import java.time.Duration

/** The island opening and closing, frame by frame, saved when ISLET_SHOTS is set (for review). */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w411dp-h891dp-xxhdpi")
class AnimationFramesTest {
    private val ctx = RuntimeEnvironment.getApplication()

    @Test
    fun frames() {
        val dir = System.getenv("ISLET_SHOTS") ?: return
        Isle.init(ctx)
        Isle.island.all.forEach { Isle.island.remove(it.id) }
        Isle.island.close(0)
        Isle.island.dismissPop(0)
        val v = IslandWindow(ctx, WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY).view
        Isle.onChange = { v.update() }
        v.update()
        v.settle()
        val dm = ctx.resources.displayMetrics
        val w = dm.widthPixels
        val h = (300 * dm.density).toInt()
        val frame = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
        fun strip(name: String) {
            val out = Bitmap.createBitmap(w * 5, h * 2, Bitmap.Config.ARGB_8888)
            val c = Canvas(out)
            for (i in 0 until 10) {
                ShadowSystemClock.advanceBy(Duration.ofMillis(if (i == 0) 0 else 40))
                frame.eraseColor(0xff343e6e.toInt())
                v.draw(Canvas(frame))
                c.drawBitmap(frame, (i % 5 * w).toFloat(), (i / 5 * h).toFloat(), null)
            }
            File(dir).mkdirs()
            File(dir, "$name.png").outputStream().use { out.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
        Isle.island.showControls()
        Isle.changed()
        strip("phone-open-cc")
        v.settle()
        Isle.island.close(Isle.now())
        Isle.changed()
        strip("phone-close-cc")
        v.settle()
        Isle.island.put(Live.Music("Midnight Drive", "The Example Band", "Player", true, 215_000, 46_000, SystemClock.elapsedRealtime()))
        Isle.changed()
        v.settle()
        Isle.island.open("music")
        Isle.changed()
        strip("phone-open-music")
        Isle.onChange = null
    }
}
