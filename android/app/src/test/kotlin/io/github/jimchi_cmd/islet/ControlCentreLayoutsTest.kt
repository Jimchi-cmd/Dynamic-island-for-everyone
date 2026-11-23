package io.github.jimchi_cmd.islet

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import android.view.WindowManager
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/** Control Centre's layouts, its photo and the tint colour on the phone. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w411dp-h891dp-xxhdpi")
class ControlCentreLayoutsTest {
    private val ctx = RuntimeEnvironment.getApplication()

    @After
    fun usual() {
        Isle.prefs.ccStyle = "classic"
        Isle.prefs.ccOrder = emptyList()
        Isle.prefs.ccPhoto = false
        Isle.prefs.style = "black"
        Isle.prefs.tint = ""
        Isle.onChange = null
    }

    private fun open(style: String): IslandView {
        Isle.init(ctx)
        Isle.prefs.ccStyle = style
        Isle.island.all.forEach { Isle.island.remove(it.id) }
        Isle.island.close(0)
        val v = IslandWindow(ctx, WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY).view
        Isle.onChange = { v.update() }
        Isle.island.showControls()
        v.update()
        v.settle()
        return v
    }

    private fun draw(v: IslandView): Bitmap {
        val dm = ctx.resources.displayMetrics
        val bmp = Bitmap.createBitmap(dm.widthPixels, (340 * dm.density).toInt(), Bitmap.Config.ARGB_8888)
        bmp.eraseColor(0xff2b2b33.toInt())
        v.draw(Canvas(bmp))
        return bmp
    }

    @Test
    fun every_layout_shows_every_button_without_overlaps() {
        for (style in listOf("modules", "sliders", "dock")) {
            val v = open(style)
            draw(v)
            val hits = v.hitRects()
            assertTrue("$style has ${hits.size} buttons", hits.size >= 9)
            for (i in hits.indices) for (j in i + 1 until hits.size) {
                assertFalse("$style: buttons $i and $j overlap", android.graphics.RectF.intersects(hits[i], hits[j]))
            }
            val tall = v.sliderRects().filter { it.height() > it.width() }
            if (style == "sliders") assertTrue("the volume slider is tall", tall.isNotEmpty()) else assertEquals(0, tall.size)
        }
    }

    @Test
    fun the_order_moves_the_buttons() {
        Isle.init(ctx)
        Isle.prefs.ccOrder = listOf("lock", "wifi")
        val v = open("dock")
        draw(v)
        // The dock's first button (furthest left) is the lock now.
        val first = v.hitRects().minByOrNull { it.left }!!
        assertTrue(first.width() > 0)
        assertEquals(listOf("lock", "wifi", "bluetooth"), islet.island.ControlCentre.order(Isle.prefs.ccOrder).take(3))
    }

    @Test
    fun pictures() {
        // Set ISLET_SHOTS to a folder to save each layout, with a photo and with a tint.
        val dir = System.getenv("ISLET_SHOTS") ?: return
        Isle.init(ctx)
        // A made-up photo: a dusk sky over hills.
        val photo = Bitmap.createBitmap(900, 600, Bitmap.Config.ARGB_8888)
        val pc = Canvas(photo)
        val p = Paint(Paint.ANTI_ALIAS_FLAG)
        p.shader = LinearGradient(0f, 0f, 0f, 600f, intArrayOf(0xff283c8c.toInt(), 0xffe6785a.toInt(), 0xfffac878.toInt()), floatArrayOf(0f, 0.6f, 1f), Shader.TileMode.CLAMP)
        pc.drawRect(0f, 0f, 900f, 600f, p)
        p.shader = null
        p.color = 0xff1e1932.toInt()
        val hill = android.graphics.Path().apply {
            moveTo(0f, 600f); lineTo(0f, 430f); quadTo(260f, 330f, 520f, 450f); quadTo(720f, 380f, 900f, 420f); lineTo(900f, 600f); close()
        }
        pc.drawPath(hill, p)
        File(ctx.filesDir, IslandView.CC_PHOTO).outputStream().use { photo.compress(Bitmap.CompressFormat.JPEG, 90, it) }
        Isle.wifiOn = true
        Isle.wifiName = "Home"
        Isle.btOn = true
        val shots = listOf(
            Triple("classic", false, ""),
            Triple("modules", false, ""),
            Triple("sliders", false, ""),
            Triple("dock", false, ""),
            Triple("dock", true, ""),
            Triple("modules", false, "#d6336c"),
            Triple("sliders", true, ""),
        )
        File(dir).mkdirs()
        for ((style, withPhoto, tint) in shots) {
            Isle.prefs.ccPhoto = withPhoto
            Isle.prefs.style = if (tint.isEmpty()) "black" else "tinted"
            Isle.prefs.tint = tint
            val v = open(style)
            val name = "cc-$style" + (if (withPhoto) "-photo" else "") + (if (tint.isNotEmpty()) "-tint" else "")
            File(dir, "$name.png").outputStream().use { draw(v).compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
    }
}
