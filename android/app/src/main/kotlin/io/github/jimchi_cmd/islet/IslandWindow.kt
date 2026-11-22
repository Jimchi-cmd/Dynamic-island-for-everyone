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
 * The island's windows.
 *
 * Hosted by the accessibility service (its windows may sit over the status
 * bar, round the camera), the island draws in a window that never moves
 * and lets every touch through, and a second, invisible window that
 * follows the pill catches the touches meant for it. Moving or resizing a
 * window lands a frame late on many phones; with the drawing window still,
 * nothing can twitch.
 *
 * As an ordinary "display over other apps" window (no accessibility
 * switch), one small window does both, only as big as the pill (or, while
 * it changes size, both sizes together): Android blocks touches through
 * an app's overlay, so a window that stood still over the top of the
 * screen would block the apps below.
 */
class IslandWindow(private val context: Context, private val type: Int) {
    private val wm = context.getSystemService(WindowManager::class.java)
    val view = IslandView(context, this)
    private var attached = false
    private val density get() = context.resources.displayMetrics.density

    /** Drawing and touch in separate windows (only an accessibility window may let touches through). */
    private val split = type == WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY

    /** The invisible window that catches touches on the pill, in split mode. */
    private val catcher = object : android.view.View(context) {
        private val at = IntArray(2)

        @android.annotation.SuppressLint("ClickableViewAccessibility")
        override fun onTouchEvent(e: android.view.MotionEvent): Boolean {
            getLocationOnScreen(at)
            return view.touch(e, at[0], at[1])
        }
    }

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

    private fun newParams(touchable: Boolean) = WindowManager.LayoutParams(
        1,
        1,
        type,
        WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
            WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS or
            WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
            WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED or
            (if (touchable) WindowManager.LayoutParams.FLAG_WATCH_OUTSIDE_TOUCH else WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE),
        PixelFormat.TRANSLUCENT,
    ).apply {
        gravity = Gravity.TOP or Gravity.START
        title = if (touchable) "Islet touch" else "Islet"
        if (Build.VERSION.SDK_INT >= 28) {
            layoutInDisplayCutoutMode = if (Build.VERSION.SDK_INT >= 30) {
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
            } else {
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
            }
        }
        if (Build.VERSION.SDK_INT >= 30) setFitInsetsTypes(0)
    }

    /** Split mode: the still drawing window, and the touch window that follows the pill. */
    private val drawParams = newParams(touchable = false)
    private val touchParams = newParams(touchable = true)

    /** For tests: where the drawing window and the touch window are (x, y, width, height). */
    internal fun drawFrame() = listOf(drawParams.x, drawParams.y, drawParams.width, drawParams.height)
    internal fun touchFrame() = listOf(touchParams.x, touchParams.y, touchParams.width, touchParams.height)
    internal val touchView: android.view.View get() = catcher

    fun attach() {
        if (attached) return
        try {
            if (split) {
                // The full width of the screen from its top edge, tall enough for the
                // biggest card: it only ever grows, and never moves.
                drawParams.x = 0
                drawParams.y = 0
                drawParams.width = context.resources.displayMetrics.widthPixels.coerceAtLeast(1)
                drawParams.height = (420 * density).toInt()
                wm.addView(view, drawParams)
                wm.addView(catcher, touchParams)
            } else {
                wm.addView(view, params)
            }
            attached = true
        } catch (e: Exception) {
            // Permission withdrawn or the window type isn't allowed here.
            try {
                wm.removeViewImmediate(view)
            } catch (x: Exception) {
                // It wasn't added.
            }
        }
    }

    fun detach() {
        if (!attached) return
        attached = false
        for (v in if (split) listOf(view, catcher) else listOf(view)) {
            try {
                wm.removeViewImmediate(v)
            } catch (e: Exception) {
                // Already gone.
            }
        }
    }

    /** Fit the window round `box` (dp); null hides it. */
    fun place(box: Box?) {
        if (!attached) return
        if (split) {
            placeSplit(box)
            return
        }
        val d = density
        val (x, y, w, h) = if (box == null) {
            // Hidden: a dot at the same corner, so showing again doesn't move it either.
            val (ax, ay) = anchor()
            listOf(ax, ay, 1, 1)
        } else {
            // The window's top left corner stays put, at the top left of the
            // widest card: as the island grows and shrinks only the window's
            // width and height change. Moving a window lands on screen a frame
            // apart from what's drawn in it, and the island jumped sideways
            // for that frame; a window that only changes size never moves
            // what's already drawn.
            // Room round the pill for its glowing rim, and a little more
            // below it so a thumb that lands just under the small pill still
            // reaches it (the screen's edge is right above).
            val m = (10 * d).toInt()
            val below = (18 * d).toInt()
            val (ax, ay) = anchor()
            val l = minOf(ax, floor(box.x * d).toInt() - m)
            val t = minOf(ay, floor(box.y * d).toInt() - m)
            listOf(l, t, ceil(box.right * d).toInt() + m - l, ceil(box.bottom * d).toInt() + below - t)
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

    /** The overlay window's fixed corner (px): the top left of the widest card, less the rim. */
    private fun anchor(): Pair<Int, Int> {
        val d = density
        val m = (10 * d).toInt()
        val card = geometry().box(islet.island.Scene.Controls)
        return Pair(floor(card.x * d).toInt() - m, floor(card.y * d).toInt() - m)
    }

    /** For tests: where the single overlay window is (x, y, width, height). */
    internal fun frame() = listOf(params.x, params.y, params.width, params.height)

    private fun placeSplit(box: Box?) {
        val d = density
        // The drawing window: grow (never move, never shrink) if a card is taller than it.
        if (box != null) {
            val need = ceil((box.bottom + 30) * d).toInt()
            val width = context.resources.displayMetrics.widthPixels
            if (need > drawParams.height || width != drawParams.width) {
                drawParams.height = maxOf(drawParams.height, need)
                drawParams.width = width
                try {
                    wm.updateViewLayout(view, drawParams)
                } catch (e: Exception) {
                    // Detached meanwhile.
                }
            }
        }
        // The touch window: the pill and some room round it, more below
        // (a thumb often lands just under the small pill; the edge is above).
        val p = touchParams
        val (x, y, w, h) = if (box == null) {
            listOf(0, 0, 1, 1)
        } else {
            val m = (12 * d).toInt()
            val below = (30 * d).toInt()
            val l = floor(box.x * d).toInt() - m
            val t = maxOf(0, floor(box.y * d).toInt() - m)
            listOf(l, t, ceil(box.right * d).toInt() + m - l, ceil(box.bottom * d).toInt() + below - t)
        }
        val flags = if (box != null) {
            p.flags and WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE.inv()
        } else {
            p.flags or WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE
        }
        if (p.x == x && p.y == y && p.width == w && p.height == h && p.flags == flags) return
        p.x = x
        p.y = y
        p.width = w
        p.height = h
        p.flags = flags
        try {
            wm.updateViewLayout(catcher, p)
        } catch (e: Exception) {
            // Detached meanwhile.
        }
    }

    /** The last camera found, and the screen width it was found at. */
    private var lastCamera: Pair<Double, Camera>? = null

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
        }
        // The camera hole, from the screen itself: it never changes. (The
        // insets of the moment can leave it out, for instance while the
        // quick settings are open, and the island jumped to the middle of
        // the screen and back.)
        if (Build.VERSION.SDK_INT >= 29) {
            cutout = try {
                @Suppress("DEPRECATION")
                wm.defaultDisplay?.cutout?.boundingRects?.firstOrNull { it.top <= 0 || it.top < it.height() }
            } catch (e: Exception) {
                null
            }
        }
        if (cutout == null && Build.VERSION.SDK_INT >= 30) {
            cutout = wm.currentWindowMetrics.windowInsets.displayCutout?.boundingRects?.firstOrNull { it.top <= 0 || it.top < it.height() }
        } else if (cutout == null && Build.VERSION.SDK_INT >= 28) {
            // Older Androids only say where the cutout is relative to a window.
            cutout = view.rootWindowInsets?.displayCutout?.boundingRects?.firstOrNull()?.let {
                Rect(it).apply { offset(params.x, params.y) }
            }
        }
        // A cutout as wide as a notch, or one at a corner, still gets a pill.
        var camera = cutout?.takeIf { it.width() > 0 && it.height() > 0 && !landscape }?.let {
            Camera(it.exactCenterX() / d.toDouble(), it.exactCenterY() / d.toDouble(), it.width() / d.toDouble(), it.height() / d.toDouble())
        }
        // Once found, the camera stays where it was for this screen width: a
        // moment without it mustn't move the island.
        if (camera != null) {
            lastCamera = width to camera
        } else if (!landscape) {
            lastCamera?.takeIf { it.first == width }?.let { camera = it.second }
        }
        fun make(extraY: Double) = Geometry(
            width / d,
            camera,
            prefs.size,
            prefs.offsetX.toDouble(),
            prefs.offsetY.toDouble() + extraY,
            prefs.extraWidth.toDouble(),
            prefs.idleClock,
            prefs.lyrics && Isle.lyrics != null,
            when (prefs.clockFace) {
                "day" -> 26.0
                "seconds" -> 14.0
                else -> 0.0
            },
            prefs.popsDown,
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
