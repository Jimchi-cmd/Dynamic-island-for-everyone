package io.github.jimchi_cmd.islet

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.view.View
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/** Draws the app's pages, dark and light, into build/renders. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ScreenRenderTest {
    private fun shoot(name: String) {
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        val pages = listOf("Home", "Setup", "Look", "Shows", "Timers", "Computer")
        val shots = pages.map { page ->
            if (page != "Home") activity.show(page)
            org.robolectric.shadows.ShadowLooper.idleMainLooper()
            val root: View = activity.window.decorView
            val b = Bitmap.createBitmap(root.width, root.height, Bitmap.Config.ARGB_8888)
            root.draw(Canvas(b))
            b
        }
        val w = shots[0].width
        val h = shots[0].height
        val sheet = Bitmap.createBitmap(w * shots.size, h, Bitmap.Config.ARGB_8888)
        val c = Canvas(sheet)
        c.drawColor(Color.MAGENTA)
        shots.forEachIndexed { i, b -> c.drawBitmap(b, (i * w).toFloat(), 0f, null) }
        File("build/renders").mkdirs()
        File("build/renders/screens-$name.png").outputStream().use { sheet.compress(Bitmap.CompressFormat.PNG, 100, it) }
        assertTrue(w > 0 && h > 0)
    }

    @Test
    @Config(sdk = [34], qualifiers = "w411dp-h891dp-night-xhdpi")
    fun dark() = shoot("dark")

    @Test
    @Config(sdk = [34], qualifiers = "w411dp-h891dp-notnight-xhdpi")
    fun light() = shoot("light")
}
