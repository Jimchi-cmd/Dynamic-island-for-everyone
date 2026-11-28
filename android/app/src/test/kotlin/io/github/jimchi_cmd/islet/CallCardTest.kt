package io.github.jimchi_cmd.islet

import android.app.PendingIntent
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.WindowManager
import islet.island.Live
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * A ringing call's card: a red button that declines and a green one that
 * answers, even when the app's buttons have no words Islet knows (a
 * CallStyle call such as Snapchat's), and its first row clear of the status bar.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w411dp-h891dp-xxhdpi")
class CallCardTest {
    private val ctx = RuntimeEnvironment.getApplication()
    private val pi get() = PendingIntent.getActivity(ctx, 0, Intent(), PendingIntent.FLAG_IMMUTABLE)

    private fun card(extra: NoteExtra): Bitmap {
        Isle.init(ctx)
        Isle.island.all.forEach { Isle.island.remove(it.id) }
        Isle.island.close(0)
        Isle.island.dismissPop(0)
        val v = IslandWindow(ctx, WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY).view
        Isle.onChange = { v.update() }
        Isle.extras["c"] = extra
        Isle.island.put(Live.Call("note:c", "Snapchat", "Snapchat", true, 0))
        Isle.island.open("note:c")
        Isle.changed()
        v.update()
        v.settle()
        val dm = ctx.resources.displayMetrics
        val bmp = Bitmap.createBitmap(dm.widthPixels, (260 * dm.density).toInt(), Bitmap.Config.ARGB_8888)
        bmp.eraseColor(0xff2b2b33.toInt())
        v.draw(Canvas(bmp))
        Isle.onChange = null
        Isle.extras.remove("c")
        return bmp
    }

    private fun has(bmp: Bitmap, colour: Int): Boolean {
        for (y in 0 until bmp.height step 3) for (x in 0 until bmp.width step 3) if (bmp.getPixel(x, y) == colour) return true
        return false
    }

    private fun save(bmp: Bitmap, name: String) {
        val dir = System.getenv("ISLET_SHOTS") ?: return
        File(dir).mkdirs()
        File(dir, "$name.png").outputStream().use { bmp.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    @Test
    fun a_ringing_call_with_unlabelled_buttons_gets_decline_and_answer() {
        val bmp = card(NoteExtra("com.snapchat.android", pi, listOf("" to pi, "" to pi), null))
        save(bmp, "call-unlabelled")
        assertTrue("a red decline button", has(bmp, 0xffff453a.toInt()))
        assertTrue("a green answer button", has(bmp, 0xff32d74b.toInt()))
    }

    @Test
    fun a_callstyle_call_uses_its_own_answer_and_decline() {
        val bmp = card(NoteExtra("com.snapchat.android", pi, emptyList(), null, answer = pi, decline = pi))
        save(bmp, "call-callstyle")
        assertTrue("a red decline button", has(bmp, 0xffff453a.toInt()))
        assertTrue("a green answer button", has(bmp, 0xff32d74b.toInt()))
    }
}
