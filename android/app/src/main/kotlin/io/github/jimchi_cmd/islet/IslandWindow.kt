package io.github.jimchi_cmd.islet

import android.content.Context
import android.content.res.Configuration
import android.graphics.PixelFormat
import android.graphics.Rect
import android.os.Build
import android.view.Gravity
import android.view.WindowManager
import islet.island.Box
import islet.island.Camera
import islet.island.Geometry
import kotlin.math.ceil
import kotlin.math.floor

/**
 * The island's window: a small transparent window over everything, only
 * as big as the pill (or, while it changes size, as big as both sizes
 * together), so taps everywhere else reach the app below.
 *
 * Hosted by the accessibility service where it is on (its windows may sit
 * over the status bar, round the camera) or else by the notification
 * listener as an ordinary "display over other apps" window.
 */
class IslandWindow(private val context: Context, private val type: Int) {
    private val wm = context.getSystemService(WindowManager::class.java)
    val view = IslandView(context, this)
    private var attached = false
    private val density get() = context.resources.displayMetrics.density

    private val params = WindowManager.LayoutParams(
        1,
        1,
        type,
        WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
            WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS or
            WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
            WindowManager.LayoutParams.FLAG_WATCH_OUTSIDE_TOUCH or
            WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED,
        PixelFormat.TRANSLUCENT,
    ).apply {
        gravity = Gravity.TOP or Gravity.START
        title = "Islet"
        if (Build.VERSION.SDK_INT >= 28) {
            layoutInDisplayCutoutMode = if (Build.VERSION.SDK_INT >= 30) {
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
            } else {
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
            }
        }
        if (Build.VERSION.SDK_INT >= 30) setFitInsetsTypes(0)
    }

    /** The window's top left on screen, in pixels. */
    val originX get() = params.x
    val originY get() = params.y

    fun attach() {
        if (attached) return
        try {
            wm.addView(view, params)
            attached = true
        } catch (e: Exception) {
            // Permission withdrawn or the window type isn't allowed here.
        }
    }

    fun detach() {
        if (!attached) return
        attached = false
        try {
            wm.removeViewImmediate(view)
        } catch (e: Exception) {
            // Already gone.
        }
    }

    /** Fit the window round `box` (dp); null hides it. */
    fun place(box: Box?) {
        if (!attached) return
        val d = density
        val (x, y, w, h) = if (box == null) {
            listOf(0, 0, 1, 1)
        } else {
            // Room round the pill for its glowing rim.
            val m = (8 * d).toInt()
            val l = floor(box.x * d).toInt() - m
            val t = floor(box.y * d).toInt() - m
            listOf(l, t, ceil(box.right * d).toInt() + m - l, ceil(box.bottom * d).toInt() + m - t)
        }
        val touchable = box != null
        val flags = if (touchable) {
            params.flags and WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE.inv()
        } else {
            params.flags or WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE
        }
        if (params.x == x && params.y == y && params.width == w && params.height == h && params.flags == flags) return
        params.x = x
        params.y = y
        params.width = w
        params.height = h
        params.flags = flags
        try {
            wm.updateViewLayout(view, params)
        } catch (e: Exception) {
            // Detached meanwhile.
        }
    }

    val landscape get() = context.resources.configuration.orientation == Configuration.ORIENTATION_LANDSCAPE

    /** The screen's size and the front camera's place, in dp. */
    fun geometry(): Geometry {
        val d = density
        val prefs = Isle.prefs
        var width = context.resources.displayMetrics.widthPixels.toDouble()
        var cutout: Rect? = null
        if (Build.VERSION.SDK_INT >= 30) {
            val m = wm.currentWindowMetrics
            width = m.bounds.width().toDouble()
            cutout = m.windowInsets.displayCutout?.boundingRects?.firstOrNull { it.top <= 0 || it.top < it.height() }
        } else if (Build.VERSION.SDK_INT >= 28) {
            // Older Androids only say where the cutout is relative to a window.
            cutout = view.rootWindowInsets?.displayCutout?.boundingRects?.firstOrNull()?.let {
                Rect(it).apply { offset(params.x, params.y) }
            }
        }
        // A cutout as wide as a notch, or one at a corner, still gets a pill.
        val camera = cutout?.takeIf { it.width() > 0 && it.height() > 0 && !landscape }?.let {
            Camera(it.exactCenterX() / d.toDouble(), it.exactCenterY() / d.toDouble(), it.width() / d.toDouble(), it.height() / d.toDouble())
        }
        fun make(extraY: Double) = Geometry(
            width / d,
            camera,
            prefs.size,
            prefs.offsetX.toDouble(),
            prefs.offsetY.toDouble() + extraY,
            prefs.extraWidth.toDouble(),
        )
        // Round the camera either way. (As an ordinary overlay, some phones'
        // status bars take taps on the small pill; the accessibility switch
        // avoids that, and the app says so.)
        return make(0.0)
    }

    companion object {
        /** The island the app's screen previews and the services share. */
        var current: IslandWindow? = null
    }
}
