package io.github.jimchi_cmd.islet

import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.Typeface
import android.graphics.drawable.Drawable
import android.net.Uri
import android.os.Build
import android.os.SystemClock
import android.provider.CalendarContract
import android.text.TextPaint
import android.text.format.DateFormat
import android.util.Size
import android.view.GestureDetector
import android.view.MotionEvent
import android.view.View
import android.view.accessibility.AccessibilityEvent
import islet.island.Box
import islet.island.ControlCentre
import islet.island.Deadlines
import islet.island.Format
import islet.island.Geometry
import islet.island.Live
import islet.island.Lists
import islet.island.Lyrics
import islet.island.Pop
import islet.island.Scene
import islet.island.Spring
import islet.island.Streak
import islet.island.Timetable
import java.util.Date
import java.util.TimeZone
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.sin

/**
 * Draws the island and handles touches on it. Every scene is drawn here
 * with plain canvas calls, so the app needs no UI library.
 */
@SuppressLint("ViewConstructor")
class IslandView(context: Context, private val window: IslandWindow) : View(context) {
    private val dp = resources.displayMetrics.density
    private var geometry: Geometry = Geometry(360.0, null)

    private var scene: Scene = Scene.Hidden
    private var shown: Scene = Scene.Hidden
    private var from: Box? = null
    private var to: Box? = null
    // Nearly critically damped: it settles without a visible bounce back.
    private val morph = Spring(1.0, damping = 0.9)
    /**
     * What was shown before the last change, and when the change began: the
     * old and the new contents cross-fade (no blink in the middle), and so
     * do the rim, the album border and the bubble.
     */
    private var prev: Scene? = null
    private var crossAt = 0L

    private fun crossP(now: Long = SystemClock.uptimeMillis()): Float {
        if (prev == null) return 1f
        val x = ((now - crossAt) / CROSS_MS.toFloat()).coerceIn(0f, 1f)
        return x * x * x * (x * (x * 6 - 15) + 10)
    }
    private var lastFrame = 0L

    /** Buttons drawn this frame, in view pixels. */
    private class Hit(val rect: RectF, val action: () -> Unit)

    /** A slider: along it (left is 0), or up a tall one (`vertical`, the bottom is 0). */
    private class Slider(val rect: RectF, val vertical: Boolean = false, val set: (Float) -> Unit) {
        fun at(x: Float, y: Float) =
            (if (vertical) (rect.bottom - y) / rect.height() else (x - rect.left) / rect.width()).coerceIn(0f, 1f)
    }

    private val hits = ArrayList<Hit>()

    /** The view's place on screen, in pixels. */
    private val at = IntArray(2)
    private val sliders = ArrayList<Slider>()
    private var dragging: Slider? = null
    private var seeking: RectF? = null
    private var seekTo: Float? = null

    private val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    private val line = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val text = TextPaint(Paint.ANTI_ALIAS_FLAG)
    private val rect = RectF()
    private val clip = Path()
    private val thumbs = HashMap<String, Bitmap?>()

    internal companion object {
        /** The rim's glow, outside in: (width in dp, colour). */
        /** How far (dp) a finger may move and still tap; how far it must move to swipe. */
        const val TAP_SLOP = 24f

        /** Taps closer together than this (ms) count once. */
        const val TAP_GAP = 220L

        /** How long the old and new contents cross-fade (ms). */
        const val CROSS_MS = 260L
        const val SWIPE = 36f

        /** Control Centre's background photo, in the app's own files. */
        const val CC_PHOTO = "control-centre.jpg"

        /** "#rrggbb" as RGB, or null. */
        fun tintOf(hex: String): Int? = hex.trim().removePrefix("#").takeIf { it.length == 6 }?.toIntOrNull(16)

        /** The accent colours (RGB), as on the computer. */
        val ACCENTS = mapOf(
            "purple" to 0x9f8cff,
            "blue" to 0x5e9eff,
            "pink" to 0xff6b8b,
            "green" to 0x4cd97b,
            "orange" to 0xffa64d,
            "white" to 0xe8e8ee,
        )

        /** The soft rim: a few strokes, widest and faintest outside, in the accent colour. */
        fun glow(accent: String): List<Pair<Float, Int>> {
            val rgb = ACCENTS[accent] ?: 0x9f8cff
            // The thin bright line: the colour mixed halfway to white.
            val light = (((rgb shr 16 and 0xff) + 255) / 2 shl 16) or (((rgb shr 8 and 0xff) + 255) / 2 shl 8) or (((rgb and 0xff) + 255) / 2)
            return listOf(7f to (0x14000000 or rgb), 4.5f to (0x2a000000 or rgb), 2.5f to (0x55000000 or rgb), 1.3f to (0xff000000.toInt() or light))
        }
    }

    private val white = Color.WHITE
    private val grey = 0xff9a9aa0.toInt()
    private val dim = 0xff2c2c2e.toInt()
    private val green = 0xff32d74b.toInt()
    private val red = 0xffff453a.toInt()
    private val orange = 0xffff9f0a.toInt()
    private val cyan = 0xff64d2ff.toInt()
    private val blue = 0xff0a84ff.toInt()
    private val purple = 0xff7d7aff.toInt()

    init {
        isClickable = true
        contentDescription = "Islet"
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_YES
    }

    private val typefaces = HashMap<String, Typeface>()

    private fun typeface(bold: Boolean): Typeface {
        val font = Isle.prefs.font
        return typefaces.getOrPut("$font$bold") { makeTypeface(font, bold) }
    }

    private fun makeTypeface(font: String, bold: Boolean): Typeface {
        // Inter, the computer's island's font (SIL Open Font License; bold is its semibold, as there).
        if (font == "inter") {
            val inter = try {
                androidx.core.content.res.ResourcesCompat.getFont(context, if (bold) R.font.inter_semibold else R.font.inter_regular)
            } catch (e: Exception) {
                null
            }
            if (inter != null) return inter
        }
        val family = when (font) {
            "serif" -> "serif"
            "mono" -> "monospace"
            "condensed" -> "sans-serif-condensed"
            "light" -> "sans-serif-light"
            else -> "sans-serif"
        }
        return Typeface.create(family, if (bold) Typeface.BOLD else Typeface.NORMAL)
    }

    // ---- Following the model ------------------------------------------------------

    /** The model changed: work out the scene, and move to it. */
    fun update() {
        geometry = window.geometry()
        readFormats()
        val p = Isle.prefs
        val next = when {
            !p.islandOn -> Scene.Hidden
            p.hideInLandscape && window.landscape -> Scene.Hidden
            else -> Isle.island.scene(Isle.now())
        }
        // Same kind of scene and the same size: only what's inside changed.
        // (A setting such as the size, a nudge or the time on the empty
        // island changes the size too: then it moves to the new one.)
        val sameBox = next == Scene.Hidden || scene == Scene.Hidden || to == null || geometry.box(next) == to
        if (sameShape(next, scene) && sameBox) {
            scene = next
            shown = next
            invalidate()
            tickLater()
            return
        }
        val current = currentBox() ?: geometry.box(Scene.Idle)
        scene = next
        from = current
        to = geometry.box(next)
        morph.snap(0.0)
        morph.target = 1.0
        if (reducedMotion()) morph.snap(1.0)
        prev = shown
        shown = next
        crossAt = SystemClock.uptimeMillis()
        lastFrame = 0
        window.place((boxFor(next) ?: current).union(current))
        announce(next)
        postInvalidateOnAnimation()
    }

    /** Jump to the end of any change (tests, and drawing for a preview). */
    fun settle() {
        morph.snap(1.0)
        prev = null
        shown = scene
        lastFrame = 0
    }

    private fun reducedMotion() = try {
        android.provider.Settings.Global.getFloat(context.contentResolver, android.provider.Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
    } catch (e: Exception) {
        false
    }

    private fun boxFor(s: Scene): Box? = if (s == Scene.Hidden) null else geometry.box(s).let {
        if (s is Scene.Compact && s.second != null) it.union(geometry.bubble(it)) else it
    }

    /** Same size and kind: only the numbers in it changed. */
    private fun sameShape(a: Scene, b: Scene): Boolean = when {
        a is Scene.Compact && b is Scene.Compact -> a.live.id == b.live.id && (a.second?.id == b.second?.id)
        a is Scene.Expanded && b is Scene.Expanded -> a.live?.id == b.live?.id
        a is Scene.Popped && b is Scene.Popped -> a.pop == b.pop ||
            (a.pop is Pop.Level && b.pop is Pop.Level && (a.pop as Pop.Level).kind == (b.pop as Pop.Level).kind)
        else -> a == b
    }

    /** The pill's box as drawn now, in dp (for tests). */
    internal fun drawnBox(): Box? = currentBox()

    /** For tests: where the buttons and sliders drawn last are (px). */
    internal fun hitRects(): List<RectF> = hits.map { RectF(it.rect) }

    internal fun sliderRects(): List<RectF> = sliders.map { RectF(it.rect) }

    private fun currentBox(): Box? {
        val f = from ?: return to
        val t = to ?: return f
        return f.lerp(t, morph.value)
    }

    private fun announce(s: Scene) {
        val words = when (s) {
            is Scene.Popped -> when (val p = s.pop) {
                is Pop.Note -> T.f("Notification from {}", p.app)
                is Pop.Charging -> if (p.charging) T.f("Charging, {} percent", p.level) else T.t("Not charging")
                is Pop.LowBattery -> T.f("Low battery, {} percent", p.level)
                is Pop.TimerDone -> p.label
                is Pop.Info -> p.text
                else -> null
            }
            is Scene.Compact -> if (s.live is Live.Call) T.f("Call: {}", (s.live as Live.Call).name) else null
            else -> null
        } ?: return
        contentDescription = words
        sendAccessibilityEvent(AccessibilityEvent.TYPE_VIEW_FOCUSED)
    }

    private fun tickLater() {
        removeCallbacks(tick)
        // Nothing moves while the screen is off.
        if (!Isle.screenOn) return
        val s = scene
        val fast = (s is Scene.Compact && s.live is Live.Music && (s.live as Live.Music).playing && (Isle.prefs.bars || geometry.lyricsRow)) ||
            (s is Scene.Expanded && s.live is Live.Music && (s.live as Live.Music).playing) ||
            (s is Scene.Expanded && s.live is Live.Stopwatch && (s.live as Live.Stopwatch).running)
        val ticking = fast || s is Scene.Compact || s is Scene.Expanded || s is Scene.Peek || s is Scene.Calendar
        if (ticking) {
            postDelayed(tick, if (fast) 33L else 1000L)
        } else if (s == Scene.Idle && Isle.prefs.idleClock) {
            // The empty island's time: redraw when the minute (or second) changes.
            val step = if (Isle.prefs.clockFace == "seconds") 1000L else 60_000L
            postDelayed(tick, step - Isle.now() % step + 50)
        }
    }

    private val tick = Runnable {
        invalidate()
        tickLater()
    }

    override fun onConfigurationChanged(newConfig: Configuration?) {
        super.onConfigurationChanged(newConfig)
        update()
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        Isle.onChange = { update() }
        post { update() }
    }

    override fun onDetachedFromWindow() {
        if (Isle.onChange != null) Isle.onChange = null
        removeCallbacks(tick)
        super.onDetachedFromWindow()
    }

    // ---- Drawing ------------------------------------------------------------------------

    override fun onDraw(c: Canvas) {
        hits.clear()
        sliders.clear()
        easing = false
        val now = SystemClock.uptimeMillis()
        val dt = if (lastFrame == 0L) 1.0 / 60 else (now - lastFrame) / 1000.0
        lastFrame = now
        if (!morph.settled) morph.step(dt)
        val p = crossP(now)
        if (p >= 1f) prev = null
        val moving = !morph.settled || prev != null
        if (moving) {
            postInvalidateOnAnimation()
        } else {
            window.place(boxFor(scene))
            tickLater()
        }
        val box = currentBox() ?: return
        if (scene == Scene.Hidden && morph.settled) return
        c.save()
        // Where the window really is on screen, not where it was last asked
        // to go: a move or resize lands a frame later, and drawing for the
        // new place too early made the island jump sideways for a frame.
        getLocationOnScreen(at)
        c.translate(-at[0].toFloat(), -at[1].toFloat())
        val l = (box.x * dp).toFloat()
        val t = (box.y * dp).toFloat()
        val r = (box.right * dp).toFloat()
        val b = (box.bottom * dp).toFloat()
        val radius = (box.radius * dp).toFloat()
        val prefs = Isle.prefs
        val notch = prefs.notch
        // The pill's outline, `inset` px in: a rounded box, or with "Notch"
        // square top corners joined to the top of the screen.
        fun outline(inset: Float, rad: Float): Path {
            shape.reset()
            if (notch) {
                rect.set(l + inset, minOf(t, 0f), r - inset, b - inset)
                shape.addRoundRect(rect, floatArrayOf(0f, 0f, 0f, 0f, rad, rad, rad, rad), Path.Direction.CW)
            } else {
                rect.set(l + inset, t + inset, r - inset, b - inset)
                shape.addRoundRect(rect, rad, rad, Path.Direction.CW)
            }
            return shape
        }
        // The pill: black, a dark shade of the accent colour, or frosted glass with a light rim.
        when (prefs.style) {
            "glass" -> {
                fill.color = 0xd92a2a30.toInt()
                c.drawPath(outline(0f, radius), fill)
                fill.shader = android.graphics.LinearGradient(0f, t, 0f, t + (b - t) * 0.6f, 0x33ffffff, 0x00ffffff, android.graphics.Shader.TileMode.CLAMP)
                c.drawPath(outline(0f, radius), fill)
                fill.shader = null
                line.color = 0x40ffffff
                line.strokeWidth = 1f * dp
                c.drawPath(outline(dp / 2, radius), line)
            }
            "tinted" -> {
                val rgb = tintOf(prefs.tint) ?: ACCENTS[prefs.accent] ?: 0x9f8cff
                fill.color = Color.rgb((rgb shr 16 and 0xff) * 22 / 100, (rgb shr 8 and 0xff) * 22 / 100, (rgb and 0xff) * 22 / 100)
                c.drawPath(outline(0f, radius), fill)
            }
            else -> {
                fill.color = Color.BLACK
                c.drawPath(outline(0f, radius), fill)
            }
        }
        // The edge: the soft rim, or the border in the album's colour, blended
        // between the old and the new scene.
        val old = prev
        val hidden = scene == Scene.Hidden
        fun edge(sc: Scene, weight: Float) {
            if (weight <= 0.01f) return
            val border = borderFor(sc)
            if (border == null && Isle.prefs.ring && !hidden) {
                // The glow as a few soft strokes: a blurred shadow costs far more every frame.
                val path = outline(0.75f * dp, radius)
                for ((width, colour) in glow(prefs.accent)) {
                    line.color = withAlpha(colour, weight)
                    line.strokeWidth = width * dp
                    c.drawPath(path, line)
                }
            }
            if (border != null) {
                line.color = withAlpha(border, weight)
                line.strokeWidth = 2f * dp
                c.drawPath(outline(dp, radius - dp), line)
            }
        }
        if (old != null) edge(old, 1f - p)
        edge(shown, p)
        // The second activity's bubble fades in and out instead of popping.
        fun bubble(sc: Scene?, weight: Float) {
            if (sc !is Scene.Compact || sc.second == null || weight <= 0.01f) return
            c.saveLayerAlpha(null, (weight * 255).toInt())
            drawBubble(c, sc.second!!, geometry.bubble(box))
            c.restore()
        }
        val grown = morph.value.toFloat().coerceIn(0f, 1f)
        neck(c, old, box, (1f - p) * (1f - grown))
        neck(c, shown, box, p * grown)
        bubble(old, (1f - p) * (1f - grown))
        bubble(shown, p * grown)
        // The contents, clipped to the pill, cross-fading.
        clip.reset()
        clip.addPath(outline(0f, radius))
        c.save()
        c.clipPath(clip)
        val area = RectF(l, t, r, b)
        // (The layers use `area`: drawing a scene reuses `rect` for its own shapes.)
        if (old != null && p < 1f) {
            c.saveLayerAlpha(area, ((1f - p) * 255).toInt())
            drawScene(c, old, area)
            c.restore()
            // Only the new scene's buttons take taps.
            hits.clear()
            sliders.clear()
        }
        // An offscreen layer only while fading: drawing straight is much cheaper.
        if (p < 1f) c.saveLayerAlpha(area, (p * 255).toInt()) else c.save()
        drawScene(c, shown, area)
        c.restore()
        c.restore()
        c.restore()
        if (easing) postInvalidateOnAnimation()
    }

    /** Scratch path for the pill's outline. */
    private val shape = Path()
    private val neckPath = Path()

    /**
     * The liquid neck between the pill and the second activity's bubble, as
     * on the computer: the two read as one drop that has started to split.
     */
    private fun neck(c: Canvas, sc: Scene?, pill: Box, weight: Float) {
        if (sc !is Scene.Compact || sc.second == null || weight <= 0.01f) return
        val bub = geometry.bubble(pill)
        val pr = (pill.right * dp).toFloat()
        val bl = (bub.x * dp).toFloat()
        if (bl - pr > 14 * dp) return
        val cy = ((bub.y + bub.h / 2) * dp).toFloat()
        val half = (bub.h * dp).toFloat() * 0.30f
        val mid = (pr + bl) / 2
        neckPath.reset()
        neckPath.moveTo(pr - 6 * dp, cy - half * 1.5f)
        neckPath.quadTo(mid, cy - half * 0.35f, bl + 6 * dp, cy - half * 1.5f)
        neckPath.lineTo(bl + 6 * dp, cy + half * 1.5f)
        neckPath.quadTo(mid, cy + half * 0.35f, pr - 6 * dp, cy + half * 1.5f)
        neckPath.close()
        fill.color = withAlpha(if (Isle.prefs.style == "glass") 0xd92a2a30.toInt() else Color.BLACK, weight)
        c.drawPath(neckPath, fill)
    }

    private fun withAlpha(colour: Int, weight: Float): Int =
        (colour and 0x00ffffff) or (((colour ushr 24) * weight.coerceIn(0f, 1f)).toInt() shl 24)

    private fun borderFor(s: Scene): Int? {
        if (!Isle.prefs.albumBorder) return null
        val music = when (s) {
            is Scene.Compact -> s.live as? Live.Music
            is Scene.Expanded -> s.live as? Live.Music
            else -> null
        } ?: return null
        return if (music.playing) Isle.artColor?.let { it or 0xff000000.toInt() } else null
    }

    private fun drawScene(c: Canvas, s: Scene, box: RectF) {
        when (s) {
            Scene.Hidden -> Unit
            Scene.Idle -> drawIdle(c, box)
            is Scene.Compact -> drawCompact(c, s.live, box)
            is Scene.Popped -> drawPop(c, s.pop, box)
            is Scene.Expanded -> drawExpanded(c, s.live, box)
            Scene.Controls -> drawClockCard(c, box)
            Scene.Toggles -> drawControls(c, box)
            Scene.Peek -> drawPeek(c, box)
            Scene.Calendar -> drawCalendar(c, box)
        }
    }

    // ---- Small helpers ---------------------------------------------------------------------

    private fun write(
        c: Canvas,
        s: String,
        x: Float,
        y: Float,
        sizeSp: Float,
        color: Int,
        bold: Boolean = false,
        max: Float = 10_000f,
        align: Paint.Align = Paint.Align.LEFT,
    ): Float {
        text.textSize = sizeSp * dp
        text.color = color
        text.typeface = typeface(bold)
        text.textAlign = align
        var shownText = s
        if (text.measureText(s) > max) {
            val n = text.breakText(s, true, (max - text.measureText("…")).coerceAtLeast(0f), null)
            shownText = s.take(n).trimEnd() + "…"
        }
        c.drawText(shownText, x, y, text)
        return text.measureText(shownText)
    }

    /** The largest size (sp) up to `size`, down to `smallest`, at which `s` fits in `max` pixels. */
    private fun fit(s: String, max: Float, size: Float, smallest: Float, bold: Boolean = false): Float {
        text.typeface = typeface(bold)
        var sp = size
        while (sp > smallest) {
            text.textSize = sp * dp
            if (text.measureText(s) <= max) return sp
            sp -= 2f
        }
        return smallest
    }

    private fun icon(c: Canvas, i: Icon, cx: Float, cy: Float, sizeDp: Float, color: Int) =
        Icons.draw(c, i, cx, cy, sizeDp * dp, color)

    /** A round button; `action` runs when it is tapped. */
    private fun button(c: Canvas, cx: Float, cy: Float, dDp: Float, bg: Int, i: Icon, fg: Int = white, iconScale: Float = 0.46f, action: () -> Unit) {
        fill.color = bg
        c.drawCircle(cx, cy, dDp * dp / 2, fill)
        icon(c, i, cx, cy, dDp * iconScale, fg)
        hits.add(Hit(RectF(cx - dDp * dp / 2, cy - dDp * dp / 2, cx + dDp * dp / 2, cy + dDp * dp / 2), action))
    }

    /** A rounded text button. */
    private fun chip(c: Canvas, label: String, x: Float, cy: Float, bg: Int, fg: Int = white, action: () -> Unit): Float {
        text.textSize = 13 * dp
        text.typeface = typeface(true)
        val w = text.measureText(label) + 24 * dp
        val h = 30 * dp
        fill.color = bg
        rect.set(x, cy - h / 2, x + w, cy + h / 2)
        c.drawRoundRect(rect, h / 2, h / 2, fill)
        write(c, label, x + w / 2, cy + 4.5f * dp, 13f, fg, bold = true, align = Paint.Align.CENTER)
        hits.add(Hit(RectF(rect), action))
        return w
    }

    private fun drawable(c: Canvas, d: Drawable?, cx: Float, cy: Float, sizeDp: Float, round: Boolean = false): Boolean {
        d ?: return false
        val half = (sizeDp * dp / 2).toInt()
        d.setBounds((cx - half).toInt(), (cy - half).toInt(), (cx + half).toInt(), (cy + half).toInt())
        if (round) {
            c.save()
            val p = Path().apply { addCircle(cx, cy, half.toFloat(), Path.Direction.CW) }
            c.clipPath(p)
            d.draw(c)
            c.restore()
        } else {
            d.draw(c)
        }
        return true
    }

    private fun bitmap(c: Canvas, b: Bitmap?, l: Float, t: Float, sizeDp: Float, cornerDp: Float): Boolean {
        b ?: return false
        val s = sizeDp * dp
        c.save()
        val p = Path().apply { addRoundRect(RectF(l, t, l + s, t + s), cornerDp * dp, cornerDp * dp, Path.Direction.CW) }
        c.clipPath(p)
        // The middle square of a wide picture (a video's thumbnail), not a squashed one.
        val side = minOf(b.width, b.height)
        val src = Rect((b.width - side) / 2, (b.height - side) / 2, (b.width + side) / 2, (b.height + side) / 2)
        c.drawBitmap(b, src, RectF(l, t, l + s, t + s), fill.apply { color = white })
        c.restore()
        return true
    }

    /**
     * Moving bars beside a song: drawn motion, or the sound's own levels
     * with "Bars follow the sound" on.
     */
    private fun bars(c: Canvas, right: Float, cy: Float, hDp: Float, color: Int, playing: Boolean) {
        val n = 4
        val w = 3f * dp
        val gap = 2.5f * dp
        val t = SystemClock.uptimeMillis() / 1000.0
        val live = if (playing && Isle.prefs.bars) LiveBars.levels() else null
        fill.color = color
        for (i in 0 until n) {
            val phase = t * (5.0 + i * 1.7) + i * 1.3
            val f = when {
                live != null -> 0.25 + 0.75 * live[i]
                playing && Isle.prefs.bars -> 0.3 + 0.7 * (0.5 + 0.5 * sin(phase)) * (0.6 + 0.4 * sin(phase * 0.37))
                else -> 0.3
            }
            val h = (hDp * dp * f).toFloat()
            val x = right - (n - i) * (w + gap)
            rect.set(x, cy - h / 2, x + w, cy + h / 2)
            c.drawRoundRect(rect, w / 2, w / 2, fill)
        }
    }

    /**
     * A download's progress, eased: apps report it in steps (often once a
     * second or slower), and a ring that jumped from step to step looked
     * stuck, then lagging. It now glides to each new value.
     */
    private class Ease(var shown: Float, var at: Long, var target: Float)

    private val eases = HashMap<String, Ease>()

    /** Something on screen is still easing (or spinning): draw again next frame. */
    private var easing = false

    /** For tests: how full a download's ring is drawn now (0..1). */
    internal fun ringShown(id: String): Float? = eases[id]?.shown

    private fun eased(id: String, percent: Int): Float {
        if (percent < 0) {
            easing = true
            return -1f
        }
        val target = percent / 100f
        val now = SystemClock.uptimeMillis()
        if (eases.size > 8 && id !in eases) eases.clear()
        val e = eases.getOrPut(id) { Ease(target, now, target) }
        if (e.target != target) {
            e.target = target
            e.at = now
        }
        // Going back (a new download in the same place) is shown at once.
        if (target < e.shown) e.shown = target
        val dt = (now - e.at).coerceAtLeast(0L)
        e.at = now
        e.shown += (target - e.shown) * (1f - kotlin.math.exp(-dt / 150f))
        if (kotlin.math.abs(target - e.shown) < 0.004f) e.shown = target else easing = true
        return e.shown
    }

    private fun ring(c: Canvas, cx: Float, cy: Float, rDp: Float, fraction: Float, color: Int) {
        line.strokeWidth = 2.5f * dp
        line.strokeCap = Paint.Cap.ROUND
        line.color = dim
        c.drawCircle(cx, cy, rDp * dp, line)
        line.color = color
        rect.set(cx - rDp * dp, cy - rDp * dp, cx + rDp * dp, cy + rDp * dp)
        if (fraction < 0) {
            val start = ((SystemClock.uptimeMillis() / 3L) % 360).toFloat()
            c.drawArc(rect, start, 90f, false, line)
        } else {
            c.drawArc(rect, -90f, 360f * fraction.coerceIn(0f, 1f), false, line)
        }
    }

    private fun bar(c: Canvas, l: Float, r: Float, cy: Float, fraction: Float, color: Int, hDp: Float = 4f) {
        val h = hDp * dp
        fill.color = dim
        rect.set(l, cy - h / 2, r, cy + h / 2)
        c.drawRoundRect(rect, h / 2, h / 2, fill)
        fill.color = color
        rect.set(l, cy - h / 2, l + (r - l) * fraction.coerceIn(0f, 1f), cy + h / 2)
        c.drawRoundRect(rect, h / 2, h / 2, fill)
    }

    /** "4:32" (no AM/PM: it has to fit beside the camera). */
    /** Time formats, read once per change (making them every frame is slow). */
    private var h24 = true
    private var shortFormat = java.text.SimpleDateFormat("H:mm", java.util.Locale.getDefault())
    private var timeFormat: java.text.DateFormat = java.text.SimpleDateFormat("H:mm", java.util.Locale.getDefault())

    private fun readFormats() {
        h24 = DateFormat.is24HourFormat(context)
        shortFormat = java.text.SimpleDateFormat(if (h24) "H:mm" else "h:mm", java.util.Locale.getDefault())
        timeFormat = DateFormat.getTimeFormat(context)
    }

    /** The empty island's clock, in the chosen face. */
    private fun idleTime(ms: Long): String = when (Isle.prefs.clockFace) {
        "day" -> T.date("EEE", ms) + " " + shortTime(ms)
        "seconds" -> shortTime(ms) + DateFormat.format(":ss", ms)
        else -> shortTime(ms)
    }

    private fun shortTime(ms: Long): String {
        shortFormat.timeZone = TimeZone.getDefault()
        return shortFormat.format(Date(ms))
    }

    private fun timeText(ms: Long, zone: TimeZone? = null): String {
        timeFormat.timeZone = zone ?: TimeZone.getDefault()
        return timeFormat.format(Date(ms))
    }

    /** Readings from the system (binder calls), at most once a second rather than every frame. */
    private val readings = HashMap<String, Pair<Long, Any>>()

    @Suppress("UNCHECKED_CAST")
    private fun <T : Any> reading(key: String, read: () -> T): T {
        val now = SystemClock.uptimeMillis()
        readings[key]?.let { (at, v) -> if (now - at < 1000) return v as T }
        val v = read()
        readings[key] = now to v
        return v
    }

    // ---- Idle --------------------------------------------------------------------------

    private fun drawIdle(c: Canvas, box: RectF) {
        val cy = box.centerY()
        val p = Isle.prefs
        if (p.idleClock) {
            // As on the computer: the time beside the camera, the battery on the other side.
            val h = box.height()
            write(c, idleTime(Isle.now()), box.left + h * 0.5f, cy + 5 * dp, 14f, white, bold = true)
            val dots = p.privacyDots && (Isle.cameraOn || Isle.micOn)
            val modes = p.modeIcons && (Isle.airplane || Isle.dnd)
            val right = box.right - h * 0.5f
            when {
                dots -> privacyDots(c, right, cy)
                modes -> {
                    var x = right - 5 * dp
                    if (Isle.dnd) { icon(c, Icon.Moon, x, cy, 12f, purple); x -= 15 * dp }
                    if (Isle.airplane) icon(c, Icon.Plane, x, cy, 12f, orange)
                }
                Isle.battery >= 0 -> {
                    val level = Isle.battery
                    val colour = when {
                        Isle.charging || level > 20 -> green
                        level > 10 -> orange
                        else -> red
                    }
                    // The percentage inside the battery, as on the phone's own status bar.
                    battery(c, right - 14 * dp, cy, level, colour, number = true)
                    if (Isle.charging) icon(c, Icon.Bolt, right - 35 * dp, cy, 11f, green)
                }
            }
            return
        }
        if (p.modeIcons) {
            var x = box.left + box.height() * 0.55f
            if (Isle.airplane) {
                icon(c, Icon.Plane, x, cy, 12f, orange)
                x += 15 * dp
            }
            if (Isle.dnd) icon(c, Icon.Moon, x, cy, 12f, purple)
        }
        if (p.privacyDots) privacyDots(c, box.right - box.height() * 0.55f, cy)
    }

    private fun privacyDots(c: Canvas, right: Float, cy: Float) {
        var x = right
        if (Isle.micOn) {
            fill.color = orange
            c.drawCircle(x, cy, 3.5f * dp, fill)
            x -= 10 * dp
        }
        if (Isle.cameraOn) {
            fill.color = green
            c.drawCircle(x, cy, 3.5f * dp, fill)
        }
    }

    // ---- Compact ------------------------------------------------------------------------

    private fun drawCompact(c: Canvas, live: Live, box: RectF) {
        // The pill's own row (a lyrics row may hang below it).
        val h = minOf(box.height(), (geometry.idleHeight * dp).toFloat())
        val cy = box.top + h / 2
        val lx = box.left + h * 0.62f
        val rx = box.right - h * 0.5f
        val wing = (geometry.wing * dp).toFloat() - 8 * dp
        val now = Isle.now()
        when (live) {
            is Live.Music -> {
                if (!bitmap(c, Isle.art, lx - 11 * dp, cy - 11 * dp, 22f, 6f) && !drawable(c, Isle.playerIcon, lx, cy, 20f)) icon(c, Icon.Music, lx, cy, 16f, white)
                bars(c, rx + 8 * dp, cy, 16f, Isle.artColor?.let { it or 0xff000000.toInt() } ?: white, live.playing)
                if (box.height() > h + 8 * dp) lyricsRow(c, box, box.top + h)
            }
            is Live.Call -> {
                val pulse = if (live.since == 0L) (0.6f + 0.4f * sin(SystemClock.uptimeMillis() / 180.0).toFloat()) else 1f
                icon(c, Icon.Phone, lx, cy, 15f, (green and 0x00ffffff) or ((pulse * 255).toInt() shl 24))
                val left = if (live.since > 0) Format.elapsed(now - live.since) else live.name
                write(c, left, lx + 12 * dp, cy + 5 * dp, 14f, green, bold = true, max = wing - 12 * dp)
                bars(c, rx + 8 * dp, cy, 16f, green, true)
            }
            is Live.Navigation -> {
                val extra = Isle.extras[live.id.removePrefix("note:")]
                if (!drawable(c, extra?.icon, lx, cy, 20f)) icon(c, Icon.Arrow, lx, cy, 17f, green)
                write(c, live.title, rx + 8 * dp, cy + 5 * dp, 14f, green, bold = true, max = wing, align = Paint.Align.RIGHT)
            }
            is Live.Timer -> {
                icon(c, Icon.Timer, lx, cy, 16f, orange)
                write(c, Format.clock(live.left(now)), rx + 8 * dp, cy + 5 * dp, 14f, orange, bold = true, align = Paint.Align.RIGHT)
            }
            is Live.Countdown -> {
                icon(c, Icon.Timer, lx, cy, 16f, orange)
                write(c, Format.clock(live.endsAt - now), rx + 8 * dp, cy + 5 * dp, 14f, orange, bold = true, align = Paint.Align.RIGHT)
            }
            is Live.Stopwatch -> {
                icon(c, Icon.Stopwatch, lx, cy, 16f, orange)
                write(c, Format.elapsed(live.elapsed(now)), rx + 8 * dp, cy + 5 * dp, 14f, orange, bold = true, align = Paint.Align.RIGHT)
            }
            is Live.Event -> {
                icon(c, Icon.Calendar, lx, cy, 15f, red)
                write(c, Format.until(live.startsAt - now).removePrefix("in "), rx + 8 * dp, cy + 5 * dp, 13f, white, bold = true, max = wing, align = Paint.Align.RIGHT)
            }
            is Live.Progress -> {
                val extra = Isle.extras[live.id.removePrefix("note:")]
                if (!drawable(c, extra?.let { Isle.appIcon(it.pkg) }, lx, cy, 18f)) icon(c, Icon.Download, lx, cy, 16f, blue)
                ring(c, rx, cy, 8f, eased(live.id, live.percent), blue)
            }
            is Live.Lesson -> {
                icon(c, Icon.Book, lx, cy, 16f, cyan)
                val text = if (live.started(now)) Format.clock(live.endsAt - now) else Format.until(live.startsAt - now).removePrefix("in ")
                write(c, text, rx + 8 * dp, cy + 5 * dp, 13f, if (live.started(now)) cyan else white, bold = true, max = wing, align = Paint.Align.RIGHT)
            }
        }
    }

    /** The lyrics row's line, the one before it (sliding away) and when it changed. */
    private var rowLine: String? = null
    private var rowPrev: String? = null
    private var rowAt = 0L

    /**
     * The line being sung, under the small music pill, as the computer's
     * island shows it: a new line slides up into place as the old one
     * slides away.
     */
    private fun lyricsRow(c: Canvas, box: RectF, top: Float) {
        val line = Isle.lyrics?.lineAt(Isle.mediaPositionNow() + Lyrics.LEAD, Isle.prefs.lyricsOffset) ?: "♪"
        val now = SystemClock.uptimeMillis()
        if (line != rowLine) {
            rowPrev = rowLine
            rowLine = line
            rowAt = now
        }
        val p = ((now - rowAt) / 320f).coerceIn(0f, 1f).let { it * it * (3 - 2 * it) }
        val rowH = box.bottom - top
        val base = top + rowH / 2 + 2 * dp
        val accent = accentOr(white)
        val max = box.width() - 32 * dp
        c.save()
        c.clipRect(box.left, top, box.right, box.bottom)
        rowPrev?.takeIf { p < 1f }?.let {
            write(c, it, box.centerX(), base - p * rowH * 0.8f, 13f, withAlpha(grey, 1f - p), bold = true, max = max, align = Paint.Align.CENTER)
        }
        write(c, line, box.centerX(), base + (1f - p) * rowH * 0.8f, 13f, withAlpha(accent, p), bold = true, max = max, align = Paint.Align.CENTER)
        c.restore()
        if (p < 1f) easing = true
    }

    private fun drawBubble(c: Canvas, live: Live, b: Box) {
        val l = (b.x * dp).toFloat()
        val t = (b.y * dp).toFloat()
        val s = (b.w * dp).toFloat()
        fill.color = Color.BLACK
        c.drawCircle(l + s / 2, t + s / 2, s / 2, fill)
        val cx = l + s / 2
        val cy = t + s / 2
        when (live) {
            is Live.Music -> if (!bitmap(c, Isle.art, cx - 9 * dp, cy - 9 * dp, 18f, 9f) && !drawable(c, Isle.playerIcon, cx, cy, 18f)) icon(c, Icon.Music, cx, cy, 14f, white)
            is Live.Call -> icon(c, Icon.Phone, cx, cy, 14f, green)
            is Live.Navigation -> icon(c, Icon.Arrow, cx, cy, 14f, green)
            is Live.Timer, is Live.Countdown -> icon(c, Icon.Timer, cx, cy, 14f, orange)
            is Live.Stopwatch -> icon(c, Icon.Stopwatch, cx, cy, 14f, orange)
            is Live.Event -> icon(c, Icon.Calendar, cx, cy, 14f, red)
            is Live.Progress -> ring(c, cx, cy, 7f, eased(live.id, live.percent), blue)
            is Live.Lesson -> icon(c, Icon.Book, cx, cy, 14f, cyan)
        }
        hits.add(Hit(RectF(l, t, l + s, t + s)) { Isle.island.open(live.id); Isle.changed() })
    }

    // ---- Pop-ups ---------------------------------------------------------------------------

    private fun drawPop(c: Canvas, p: Pop, box: RectF) {
        val cy = box.centerY()
        val h = box.height()
        val pad = 16 * dp
        when (p) {
            is Pop.Note -> {
                // On the lock screen, only which app: the message waits for an unlock.
                val locked = context.getSystemService(android.app.KeyguardManager::class.java)?.isKeyguardLocked == true
                val title = if (locked) p.app else p.title.ifEmpty { p.app }
                val body = if (locked) T.t("New notification") else p.text.ifEmpty { p.app }
                val extra = Isle.extras[p.key]
                // With buttons, the text sits higher to leave room for them.
                val ty = if (p.actions.isEmpty() || locked) cy else box.top + 40 * dp
                val ix = box.left + pad + 20 * dp
                avatar(c, extra, title, ix, ty, locked)
                val x = ix + 32 * dp
                val max = box.right - pad - x
                val badge = if (p.count > 1) 30 * dp else 0f
                val w = write(c, title, x, ty - 4 * dp, 15f, white, bold = true, max = max - badge - 44 * dp)
                write(c, " · now", x + w, ty - 4 * dp, 12f, grey)
                write(c, body, x, ty + 16 * dp, 14f, 0xffd1d1d6.toInt(), max = max)
                if (p.count > 1) {
                    fill.color = 0xff3a3a3c.toInt()
                    rect.set(box.right - pad - 28 * dp, ty - 17 * dp, box.right - pad, ty + 1 * dp)
                    c.drawRoundRect(rect, 9 * dp, 9 * dp, fill)
                    write(c, p.count.toString(), rect.centerX(), ty - 3.5f * dp, 12f, white, bold = true, align = Paint.Align.CENTER)
                }
                if (!locked && p.actions.isNotEmpty()) {
                    var ax = x
                    val ay = box.bottom - 22 * dp
                    for ((i, label) in p.actions.withIndex()) {
                        val aw = write(c, label, ax, ay + 5 * dp, 14f, blue, bold = true)
                        hits.add(Hit(RectF(ax - 8 * dp, ay - 14 * dp, ax + aw + 8 * dp, ay + 14 * dp)) { Isle.act(p.key, i) })
                        ax += aw + 22 * dp
                    }
                }
            }
            is Pop.Screenshot -> {
                val thumb = thumbnail(p.uri)
                if (!bitmap(c, thumb, box.left + pad, cy - 25 * dp, 50f, 8f)) icon(c, Icon.Screenshot, box.left + pad + 25 * dp, cy, 26f, white)
                val x = box.left + pad + 64 * dp
                write(c, T.t("Screenshot saved"), x, cy - 3 * dp, 15f, white, bold = true)
                write(c, T.t("Tap to open it"), x, cy + 16 * dp, 13f, grey)
            }
            is Pop.TimerDone -> {
                icon(c, Icon.Timer, box.left + pad + 18 * dp, cy, 26f, orange)
                write(c, T.t("Time's up"), box.left + pad + 46 * dp, cy - 3 * dp, 15f, orange, bold = true)
                write(c, p.label, box.left + pad + 46 * dp, cy + 16 * dp, 13f, grey, max = box.width() - 160 * dp)
                chip(c, "OK", box.right - pad - 56 * dp, cy, dim) { Isle.island.dismissPop(Isle.now()); Isle.changed() }
            }
            is Pop.Bluetooth -> {
                icon(c, Icon.Bluetooth, box.left + pad + 18 * dp, cy, 24f, blue)
                val room = if (p.battery >= 0) 150 * dp else 90 * dp
                write(c, p.name, box.left + pad + 46 * dp, cy - 3 * dp, 15f, white, bold = true, max = box.width() - room)
                write(c, if (p.connected) T.t("Connected") else T.t("Disconnected"), box.left + pad + 46 * dp, cy + 16 * dp, 13f, grey)
                // The device's own battery, as the computer shows for earbuds and headphones.
                if (p.battery >= 0) {
                    val colour = if (p.battery > 20) green else if (p.battery > 10) orange else red
                    battery(c, box.right - pad - 16 * dp, cy, p.battery, colour, number = true)
                }
            }
            is Pop.Level -> if (geometry.popsDown) {
                // Grown downward: under the camera, the icon, a bar and the amount in one row.
                val ty = box.top + (geometry.idleHeight * dp).toFloat() + 12 * dp
                val i = if (p.kind == Pop.LevelKind.Volume) Icon.Sound else Icon.Brightness
                icon(c, i, box.left + 20 * dp, ty, 15f, white)
                write(c, "${(p.fraction * 100).toInt().coerceIn(0, 100)}%", box.right - 14 * dp, ty + 5 * dp, 13f, white, bold = true, align = Paint.Align.RIGHT)
                bar(c, box.left + 34 * dp, box.right - 52 * dp, ty, p.fraction, white, 5f)
            } else {
                // As on the computer: the icon and the amount by the camera, a bar under it.
                val row = (geometry.idleHeight * dp).toFloat()
                val ty = box.top + row / 2
                val i = if (p.kind == Pop.LevelKind.Volume) Icon.Sound else Icon.Brightness
                icon(c, i, box.left + row * 0.6f, ty, 16f, white)
                write(c, "${(p.fraction * 100).toInt().coerceIn(0, 100)}%", box.right - row * 0.55f, ty + 5 * dp, 14f, white, bold = true, align = Paint.Align.RIGHT)
                bar(c, box.left + row * 0.5f, box.right - row * 0.5f, box.top + row + 7 * dp, p.fraction, white, 5f)
            }
            else -> {
                val lx = box.left + h * 0.6f
                val rx = box.right - h * 0.55f
                val (i, colour, left, right) = when (p) {
                    is Pop.Charging -> Quad(Icon.Bolt, if (p.charging) green else grey, if (!p.charging) T.t("Not charging") else if (p.level >= 100) T.t("Fully charged") else T.t("Charging"), "${p.level}%")
                    is Pop.LowBattery -> Quad(Icon.Bolt, red, T.t("Low battery"), "${p.level}%")
                    is Pop.Ringer -> when (p.mode) {
                        Pop.RingerMode.Silent -> Quad(Icon.BellOff, red, "", T.t("Silent"))
                        Pop.RingerMode.Vibrate -> Quad(Icon.Vibrate, orange, "", T.t("Vibrate"))
                        Pop.RingerMode.Ring -> Quad(Icon.Bell, white, "", T.t("Ring"))
                    }
                    is Pop.Airplane -> Quad(Icon.Plane, orange, T.t("Airplane mode"), if (p.on) T.t("On") else T.t("Off"))
                    is Pop.DoNotDisturb -> Quad(Icon.Moon, purple, T.t("Do Not Disturb"), if (p.on) T.t("On") else T.t("Off"))
                    is Pop.Wifi -> Quad(Icon.Wifi, blue, p.name.ifEmpty { T.t("Wi-Fi") }, T.t("Connected"))
                    is Pop.Info -> Quad(null, white, p.text, "")
                    is Pop.Payment -> Quad(Icon.Check, green, T.t("Paid"), p.amount)
                    else -> Quad(null, white, "", "")
                }
                if (geometry.popsDown) {
                    droppedPop(c, p, box, i, colour, left, right)
                } else if (i == null) {
                    write(c, left, box.centerX(), cy + 5 * dp, 14f, white, bold = true, max = box.width() - 24 * dp, align = Paint.Align.CENTER)
                } else {
                    icon(c, i, lx, cy, 16f, colour)
                    write(c, left, lx + 14 * dp, cy + 5 * dp, 14f, colour, bold = true, max = box.width() / 2)
                    if (p is Pop.Charging || p is Pop.LowBattery) {
                        battery(c, rx - 10 * dp, cy, (p as? Pop.Charging)?.level ?: (p as Pop.LowBattery).level, colour)
                        write(c, right, rx - 26 * dp, cy + 5 * dp, 13f, white, bold = true, align = Paint.Align.RIGHT)
                    } else {
                        val tone = if (p is Pop.Wifi) grey else colour
                        write(c, right, rx + 6 * dp, cy + 5 * dp, 15f, tone, bold = true, align = Paint.Align.RIGHT)
                    }
                }
            }
        }
    }

    private data class Quad(val icon: Icon?, val color: Int, val left: String, val right: String)

    /**
     * A small pop-up grown downward: the camera's pill on top, and under it
     * the icon and what happened, then the amount (with the battery, for
     * charging). With only one of them, one line in the middle.
     */
    private fun droppedPop(c: Canvas, p: Pop, box: RectF, i: Icon?, colour: Int, left: String, right: String) {
        val top = box.top + (geometry.idleHeight * dp).toFloat()
        val room = box.width() - 20 * dp
        val first = left.ifEmpty { right }
        val second = if (left.isEmpty()) "" else right
        val y1 = if (second.isEmpty()) (top + box.bottom) / 2 else top + 13 * dp
        // The icon and the first line, centred together.
        val iconW = if (i != null) 22 * dp else 0f
        val w1 = minOf(measure(first, 14f, true), room - iconW)
        val x1 = box.centerX() - (iconW + w1) / 2
        if (i != null) icon(c, i, x1 + 8 * dp, y1, 15f, colour)
        write(c, first, x1 + iconW, y1 + 5 * dp, 14f, if (i == null) white else colour, bold = true, max = room - iconW)
        if (second.isEmpty()) return
        val y2 = top + 33 * dp
        val battery = (p as? Pop.Charging)?.level ?: (p as? Pop.LowBattery)?.level
        if (battery != null) {
            val tw = measure(second, 13f, true)
            val x2 = box.centerX() - (tw + 34 * dp) / 2
            write(c, second, x2, y2 + 4.5f * dp, 13f, white, bold = true)
            battery(c, x2 + tw + 20 * dp, y2, battery, colour)
        } else {
            val tone = if (p is Pop.Wifi) grey else colour
            write(c, second, box.centerX(), y2 + 4.5f * dp, 13f, tone, bold = true, max = room, align = Paint.Align.CENTER)
        }
    }

    /** The sender's picture if the notification has one, else their initial on a colour, with the app's icon in the corner. */
    private fun avatar(c: Canvas, extra: NoteExtra?, name: String, cx: Float, cy: Float, locked: Boolean) {
        val r = 20 * dp
        val photo = if (locked) null else extra?.icon
        if (!drawable(c, photo, cx, cy, 40f, round = true)) {
            val app = extra?.let { Isle.appIcon(it.pkg) }
            if (locked || name.isBlank()) {
                if (!drawable(c, app, cx, cy, 40f)) icon(c, Icon.Bell, cx, cy, 24f, white)
                return
            }
            val hues = intArrayOf(0xffc77dff.toInt(), 0xff5e9eff.toInt(), 0xffff8a65.toInt(), 0xff4cd97b.toInt(), 0xffffc04d.toInt(), 0xffff6b8b.toInt())
            fill.color = hues[Math.floorMod(name.hashCode(), hues.size)]
            c.drawCircle(cx, cy, r, fill)
            write(c, name.trim().take(1).uppercase(), cx, cy + 6.5f * dp, 18f, white, bold = true, align = Paint.Align.CENTER)
        }
        // The app's own icon, small, at the bottom right.
        val app = extra?.let { Isle.appIcon(it.pkg) } ?: return
        fill.color = Color.BLACK
        c.drawCircle(cx + 14 * dp, cy + 14 * dp, 9 * dp, fill)
        drawable(c, app, cx + 14 * dp, cy + 14 * dp, 15f, round = true)
    }

    /** A battery `level` % full; with `number`, the percentage written inside it. */
    private fun battery(c: Canvas, cx: Float, cy: Float, level: Int, color: Int, number: Boolean = false) {
        val lv = level.coerceIn(0, 100)
        if (number) {
            // As on the phone's own status bar: a solid light battery, the
            // charge in colour, the number in black on top (readable at any level).
            val w = 26 * dp
            val h = 14 * dp
            fill.color = 0xff9a9aa2.toInt()
            rect.set(cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2)
            c.drawRoundRect(rect, 4 * dp, 4 * dp, fill)
            c.drawRoundRect(cx + w / 2 + 0.8f * dp, cy - 2.5f * dp, cx + w / 2 + 2.6f * dp, cy + 2.5f * dp, 1 * dp, 1 * dp, fill)
            c.save()
            c.clipRect(cx - w / 2, cy - h / 2, cx - w / 2 + w * lv / 100f, cy + h / 2)
            fill.color = color
            c.drawRoundRect(rect, 4 * dp, 4 * dp, fill)
            c.restore()
            write(c, lv.toString(), cx, cy + 3.6f * dp, 10f, Color.BLACK, bold = true, align = Paint.Align.CENTER)
            return
        }
        val w = 24 * dp
        val h = 12 * dp
        line.strokeWidth = 1.4f * dp
        line.color = grey
        rect.set(cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2)
        c.drawRoundRect(rect, 3.5f * dp, 3.5f * dp, line)
        fill.color = grey
        c.drawRect(cx + w / 2 + 1 * dp, cy - 2.5f * dp, cx + w / 2 + 2.5f * dp, cy + 2.5f * dp, fill)
        fill.color = color
        rect.set(cx - w / 2 + 2 * dp, cy - h / 2 + 2 * dp, cx - w / 2 + 2 * dp + (w - 4 * dp) * lv / 100f, cy + h / 2 - 2 * dp)
        c.drawRoundRect(rect, 2 * dp, 2 * dp, fill)
    }

    private fun thumbnail(uri: String): Bitmap? {
        if (thumbs.containsKey(uri)) return thumbs[uri]
        thumbs[uri] = null
        Isle.background {
            val b = try {
                if (Build.VERSION.SDK_INT >= 29) {
                    context.contentResolver.loadThumbnail(Uri.parse(uri), Size(160, 160), null)
                } else {
                    context.contentResolver.openInputStream(Uri.parse(uri))?.use {
                        android.graphics.BitmapFactory.decodeStream(it, null, android.graphics.BitmapFactory.Options().apply { inSampleSize = 8 })
                    }
                }
            } catch (e: Exception) {
                null
            }
            post {
                if (thumbs.size > 6) thumbs.clear()
                thumbs[uri] = b
                invalidate()
            }
        }
        return null
    }

    // ---- Expanded ---------------------------------------------------------------------------

    private fun drawExpanded(c: Canvas, live: Live?, whole: RectF) {
        // As on the computer: every open card has a way to the control
        // centre at its top (the music card draws its own; the clock card
        // has the controls button).
        val box = if (live != null && live !is Live.Music) {
            chip(c, T.t("‹  Control Centre"), whole.left + 18 * dp, whole.top + 26 * dp, dim) {
                Isle.island.showControls()
                Isle.changed()
            }
            RectF(whole.left, whole.top + Geometry.CARD_HEADER.toFloat() * dp, whole.right, whole.bottom)
        } else {
            whole
        }
        val pad = 20 * dp
        val l = box.left + pad
        val r = box.right - pad
        val t = box.top + pad
        val now = Isle.now()
        when (live) {
            null -> drawClockCard(c, box)
            is Live.Music -> drawMusic(c, live, box)
            is Live.Call -> {
                val cy = box.top + box.height() / 2
                fill.color = 0xff3a3a3c.toInt()
                c.drawCircle(l + 24 * dp, cy, 24 * dp, fill)
                write(c, live.name.take(1).uppercase(), l + 24 * dp, cy + 7 * dp, 20f, white, bold = true, align = Paint.Align.CENTER)
                val status = when {
                    live.incoming -> T.t("Incoming call")
                    live.since > 0 -> Format.elapsed(now - live.since)
                    else -> T.t("Calling…")
                }
                write(c, live.name, l + 60 * dp, cy - 4 * dp, 18f, white, bold = true, max = box.width() * 0.4f)
                write(c, status, l + 60 * dp, cy + 16 * dp, 13f, if (live.since > 0) green else grey, max = box.width() * 0.4f)
                val actions = Isle.extras[live.id.removePrefix("note:")]?.actions.orEmpty().takeLast(3)
                var x = r - 24 * dp
                for ((label, intent) in actions.reversed()) {
                    val lower = label.lowercase()
                    val (bg, i) = when {
                        listOf("answer", "accept", "pick").any { lower.contains(it) } -> green to Icon.Phone
                        listOf("decline", "reject", "hang", "end", "dismiss").any { lower.contains(it) } -> red to Icon.HangUp
                        lower.contains("mute") -> dim to Icon.Mic
                        lower.contains("speaker") -> dim to Icon.Sound
                        lower.contains("video") -> dim to Icon.Video
                        else -> dim to Icon.Phone
                    }
                    button(c, x, cy, 48f, bg, i) { Isle.send(intent) }
                    x -= 58 * dp
                }
                if (actions.isEmpty()) chip(c, T.t("Open"), r - 70 * dp, cy, dim) { Isle.open(live.id) }
            }
            is Live.Navigation -> {
                val extra = Isle.extras[live.id.removePrefix("note:")]
                val cy = box.centerY()
                fill.color = 0xff123d1c.toInt()
                c.drawCircle(l + 24 * dp, cy - 10 * dp, 24 * dp, fill)
                if (!drawable(c, extra?.icon, l + 24 * dp, cy - 10 * dp, 30f)) icon(c, Icon.Arrow, l + 24 * dp, cy - 10 * dp, 26f, green)
                write(c, live.title, l + 62 * dp, cy - 14 * dp, 22f, white, bold = true, max = r - l - 62 * dp)
                write(c, live.text, l + 62 * dp, cy + 6 * dp, 13f, grey, max = r - l - 62 * dp)
                hits.add(Hit(RectF(box)) { Isle.open(live.id) })
                chip(c, T.f("Open {}", live.app), l + 62 * dp, cy + 30 * dp, green, Color.BLACK) { Isle.open(live.id) }
            }
            is Live.Timer -> {
                val streak = if (live.label == "Focus" || live.onBreak) Isle.prefs.streak.label(Streak.dayOf(now)) else null
                write(c, listOfNotNull(if (live.onBreak) T.t("Break") else T.t(live.label), streak).joinToString(" · "), l, t + 14 * dp, 13f, grey, max = r - l - 160 * dp)
                write(c, Format.clock(live.left(now)), l, t + 62 * dp, 44f, orange, bold = true)
                // How much is left, as a thin bar under the time.
                bar(c, l, l + 150 * dp, t + 80 * dp, live.left(now).toFloat() / live.total.coerceAtLeast(1), orange, 3f)
                val cy = t + 44 * dp
                button(c, r - 24 * dp, cy, 48f, 0xff5a3a00.toInt(), if (live.pausedLeft != null) Icon.Play else Icon.Pause, orange) {
                    if (Isle.focus.paused) Isle.focus.resume(Isle.now()) else Isle.focus.pause(Isle.now())
                    Isle.syncTimers()
                }
                button(c, r - 82 * dp, cy, 48f, dim, Icon.Plus) {
                    Isle.focus.add(1, Isle.now())
                    Isle.syncTimers()
                }
                button(c, r - 140 * dp, cy, 48f, dim, Icon.Close) {
                    Isle.focus.stop()
                    Isle.island.close(Isle.now())
                    Isle.syncTimers()
                }
            }
            is Live.Countdown -> {
                write(c, "${live.app} · ${live.label}", l, t + 14 * dp, 13f, grey, max = r - l)
                write(c, Format.clock(live.endsAt - now), l, t + 62 * dp, 44f, orange, bold = true)
                chip(c, T.t("Open"), r - 70 * dp, box.bottom - pad - 20 * dp, dim) { Isle.open(live.id) }
            }
            is Live.Stopwatch -> {
                write(c, T.t("Stopwatch"), l, t + 14 * dp, 13f, grey)
                write(c, Format.stopwatch(live.elapsed(now)), l, t + 62 * dp, 42f, white, bold = true, max = r - l - 130 * dp)
                if (live.laps.isNotEmpty()) {
                    val parts = ArrayList<String>()
                    val first = maxOf(0, live.laps.size - 3)
                    for (n in live.laps.size - 1 downTo first) parts.add(T.f("Lap {} {}", n + 1, Format.stopwatch(live.laps[n])))
                    write(c, parts.joinToString("   "), l, t + 92 * dp, 13f, grey, max = r - l)
                }
                val cy = t + 44 * dp
                if (live.running) {
                    button(c, r - 82 * dp, cy, 48f, dim, Icon.Flag) { Isle.stopwatch.lap(Isle.now()); Isle.syncTimers() }
                    button(c, r - 24 * dp, cy, 48f, 0xff5a1d1a.toInt(), Icon.Pause, red) { Isle.stopwatch.pause(Isle.now()); Isle.syncTimers() }
                } else {
                    button(c, r - 82 * dp, cy, 48f, dim, Icon.Reset) {
                        Isle.stopwatch.reset()
                        Isle.island.close(Isle.now())
                        Isle.syncTimers()
                    }
                    button(c, r - 24 * dp, cy, 48f, 0xff123d1c.toInt(), Icon.Play, green) { Isle.stopwatch.start(Isle.now()); Isle.syncTimers() }
                }
            }
            is Live.Event -> {
                icon(c, Icon.Calendar, l + 14 * dp, t + 16 * dp, 24f, red)
                write(c, live.title, l + 36 * dp, t + 22 * dp, 17f, white, bold = true, max = r - l - 36 * dp)
                val starts = Format.until(live.startsAt - now)
                write(c, "${timeText(live.startsAt)} · ${if (starts == islet.island.Words.t("now")) starts else T.f("starts {}", starts)}", l + 36 * dp, t + 44 * dp, 13f, grey)
                val cy = box.bottom - pad - 18 * dp
                var x = l
                live.join?.let { url -> x += chip(c, T.t("Join"), x, cy, green, Color.BLACK) { Isle.openUrl(url) } + 10 * dp }
                chip(c, T.t("Open calendar"), x, cy, dim) {
                    val uri = CalendarContract.CONTENT_URI.buildUpon().appendPath("time").appendPath(live.startsAt.toString()).build()
                    Isle.start(Intent(Intent.ACTION_VIEW, uri))
                }
            }
            is Live.Lesson -> {
                icon(c, Icon.Book, l + 14 * dp, t + 16 * dp, 24f, cyan)
                write(c, live.name, l + 36 * dp, t + 22 * dp, 17f, white, bold = true, max = r - l - 36 * dp)
                val started = live.started(now)
                val sub = listOf(
                    "${timeText(live.startsAt)}–${timeText(live.endsAt)}",
                    live.room,
                ).filter { it.isNotEmpty() }.joinToString(" · ")
                write(c, sub, l + 36 * dp, t + 44 * dp, 13f, grey, max = r - l - 36 * dp)
                val label = if (started) T.f("{} left", Format.clock(live.endsAt - now)) else T.f("Starts {}", Format.until(live.startsAt - now))
                write(c, label, l, box.bottom - pad - 22 * dp, 20f, cyan, bold = true)
                if (started) {
                    val total = (live.endsAt - live.startsAt).coerceAtLeast(1)
                    bar(c, l, r, box.bottom - pad - 4 * dp, (now - live.startsAt).toFloat() / total, cyan, 4f)
                }
            }
            is Live.Progress -> {
                val extra = Isle.extras[live.id.removePrefix("note:")]
                if (!drawable(c, extra?.let { Isle.appIcon(it.pkg) }, l + 18 * dp, t + 18 * dp, 36f)) icon(c, Icon.Download, l + 18 * dp, t + 18 * dp, 28f, blue)
                write(c, live.title, l + 48 * dp, t + 14 * dp, 16f, white, bold = true, max = r - l - 110 * dp)
                write(c, live.app, l + 48 * dp, t + 34 * dp, 13f, grey, max = r - l - 110 * dp)
                if (live.percent >= 0) write(c, "${live.percent}%", r, t + 24 * dp, 20f, blue, bold = true, align = Paint.Align.RIGHT)
                bar(c, l, r, box.bottom - pad - 14 * dp, eased(live.id, live.percent).coerceAtLeast(0f), blue, 6f)
                hits.add(Hit(RectF(box)) { Isle.open(live.id) })
            }
        }
    }

    /**
     * The music card, big as on a phone's own player: a large cover, the
     * song, the lyrics line and the next one, the progress bar, the volume,
     * and big buttons.
     */
    /**
     * The open music card, as designed in Figma ("Island / Expanded / Music
     * (phone)"): Control Centre and the player at the top, the song, the
     * progress bar, the buttons with the sleep timer and the lyrics switch
     * either side, the volume, and, with lyrics on, two lines of them.
     */
    private fun drawMusic(c: Canvas, m: Live.Music, box: RectF) {
        val l = box.left + 22 * dp
        val r = box.right - 22 * dp
        val accent = Isle.artColor?.let { it or 0xff000000.toInt() } ?: white
        val soft = 0x1affffff
        // To the control centre (its back button comes back here), and the player.
        val hy = box.top + 31 * dp
        chip(c, T.t("‹  Control Centre"), l, hy, dim) {
            Isle.island.showControls()
            Isle.changed()
        }
        if (m.app.isNotEmpty()) playerTag(c, m.app, r, hy, accentOr(orange))
        // The cover (tap it to open the player), the title and artist, the bars.
        val t = box.top + 61 * dp
        val cover = 84f
        if (!bitmap(c, Isle.art, l, t, cover, 18f)) {
            fill.color = 0xff3a3a3c.toInt()
            rect.set(l, t, l + cover * dp, t + cover * dp)
            c.drawRoundRect(rect, 18 * dp, 18 * dp, fill)
            val mid = cover / 2 * dp
            if (!drawable(c, Isle.playerIcon, l + mid, t + mid, 48f)) icon(c, Icon.Music, l + mid, t + mid, 34f, white)
        }
        hits.add(Hit(RectF(l, t, l + cover * dp, t + cover * dp)) { Isle.openPlayer() })
        val tx = l + (cover + 16) * dp
        val textMax = r - tx - 34 * dp
        write(c, m.title, tx, t + 38 * dp, 21f, white, bold = true, max = textMax)
        write(c, m.artist.ifEmpty { m.app }, tx, t + 62 * dp, 15f, grey, max = textMax)
        bars(c, r, t + 42 * dp, 24f, accent, m.playing)
        // Progress in the song's colour, the times under it; drag to move in the song.
        val position = seekTo?.let { (it * m.duration).toLong() } ?: Isle.mediaPositionNow()
        val by = t + 102 * dp
        if (m.duration > 0) {
            bar(c, l, r, by, position.toFloat() / m.duration, accent, if (seekTo != null) 8f else 6f)
            write(c, Format.elapsed(position), l, by + 21 * dp, 12f, grey)
            write(c, "-" + Format.elapsed((m.duration - position).coerceAtLeast(0L)), r, by + 21 * dp, 12f, grey, align = Paint.Align.RIGHT)
            seeking = RectF(l - 8 * dp, by - 16 * dp, r + 8 * dp, by + 12 * dp)
            sliders.add(Slider(seeking!!) { seekTo = it })
        }
        // The buttons: the sleep timer (off → 15 → 30 → 45 → 60 minutes),
        // previous, play/pause on a white disc, next, and lyrics on and off.
        val cy = by + 73 * dp
        val cx = box.centerX()
        val sleep = Isle.sleepMinutesLeft()
        val purple = 0xffbf9bff.toInt()
        button(c, l + 22 * dp, cy, 44f, if (sleep != null) 0xff2c2a5a.toInt() else soft, Icon.Moon, if (sleep != null) purple else 0xbfffffff.toInt()) { Isle.stepSleep() }
        if (sleep != null) write(c, "${sleep}m", l + 22 * dp, cy + 36 * dp, 11f, purple, bold = true, align = Paint.Align.CENTER)
        button(c, cx - 80 * dp, cy, 56f, Color.TRANSPARENT, Icon.Previous, iconScale = 0.6f) { Isle.previous() }
        button(c, cx, cy, 64f, white, if (m.playing) Icon.Pause else Icon.Play, Color.BLACK) { Isle.playPause() }
        button(c, cx + 80 * dp, cy, 56f, Color.TRANSPARENT, Icon.Next, iconScale = 0.6f) { Isle.next() }
        val on = Isle.prefs.lyrics
        val lyricColour = accentOr(orange)
        button(c, r - 22 * dp, cy, 44f, if (on) (lyricColour and 0x00ffffff) or 0x2e000000 else soft, Icon.Quote, if (on) lyricColour else grey) { Isle.toggleLyrics() }
        // The volume: drag along it.
        val vy = cy + 56 * dp
        val vl = l + 26 * dp
        val vr = r - 28 * dp
        icon(c, Icon.Sound, l + 7 * dp, vy, 14f, grey)
        icon(c, Icon.Sound, r - 9 * dp, vy, 18f, grey)
        bar(c, vl, vr, vy, reading("volume") { Controls.volume }, 0xe6ffffff.toInt(), 6f)
        sliders.add(Slider(RectF(vl - 8 * dp, vy - 16 * dp, vr + 8 * dp, vy + 16 * dp)) { readings.remove("volume"); Controls.volume = it })
        // With lyrics on: the line being sung, and the next one, under it all.
        if (!on) return
        val ly = vy + 40 * dp
        val lyrics = Isle.lyrics
        val now = position + Lyrics.LEAD
        val line = lyrics?.lineAt(now, Isle.prefs.lyricsOffset)
            ?: if (Isle.lyricsMissing) T.t("No synced lyrics for this song") else null
        val next = lyrics?.nextAfter(now, Isle.prefs.lyricsOffset)
        if (line != null) write(c, line, cx, ly, 17f, if (lyrics != null) white else grey, bold = lyrics != null, max = r - l, align = Paint.Align.CENTER)
        if (next != null) write(c, next, cx, ly + 24 * dp, 15f, 0x73ffffff, max = r - l, align = Paint.Align.CENTER)
    }

    /** The player's name in a tinted capsule at the card's top right (tap it to open the player). */
    private fun playerTag(c: Canvas, name: String, right: Float, cy: Float, colour: Int) {
        text.textSize = 13 * dp
        text.typeface = typeface(true)
        val label = name.take(24)
        val w = minOf(text.measureText(label), 130 * dp) + 35 * dp
        val h = 28 * dp
        fill.color = (colour and 0x00ffffff) or 0x29000000
        rect.set(right - w, cy - h / 2, right, cy + h / 2)
        c.drawRoundRect(rect, h / 2, h / 2, fill)
        fill.color = colour
        c.drawCircle(right - w + 13.5f * dp, cy, 3.5f * dp, fill)
        write(c, label, right - w + 23 * dp, cy + 4.5f * dp, 13f, colour, bold = true, max = 130 * dp)
        hits.add(Hit(RectF(rect)) { Isle.openPlayer() })
    }

    private fun accentOr(fallback: Int): Int = Isle.artColor?.let { it or 0xff000000.toInt() } ?: fallback

    /**
     * The clock card, which is also the control centre, laid out as the
     * computer's: the time and date, the weather in the corner, Wi-Fi and
     * Bluetooth with the to-do, speed and toggles buttons beside them, and
     * search, calendar, screenshot and lock under them. Opened from an
     * activity's card, a "Now Playing ›" button at the top goes back to it.
     */
    private fun drawClockCard(c: Canvas, box: RectF) {
        val photo = ccPhoto()
        if (photo != null) {
            // The photo, cropped to fill the card and darkened under the buttons.
            val k = maxOf(box.width() / photo.width, box.height() / photo.height)
            val w = photo.width * k
            val h = photo.height * k
            rect.set(box.centerX() - w / 2, box.centerY() - h / 2, box.centerX() + w / 2, box.centerY() + h / 2)
            c.drawBitmap(photo, null, rect, bitmapPaint)
            fill.color = 0x6b000000
            c.drawRect(box, fill)
        }
        val style = Isle.prefs.ccStyle
        if (style != "classic" && style in ControlCentre.STYLES) {
            drawStyledCard(c, box, style, photo != null)
            return
        }
        val left = box.left + 26 * dp
        val right = box.right - 20 * dp
        val top = box.top
        val now = Isle.now()
        val faint = 0x24ffffff
        // The way back to the card it was opened from.
        backLabel()?.let { backButton(c, it, left, top + 8 * dp) }
        // The time, big, with AM/PM smaller beside it; the date under it.
        val weather = Isle.weather
        val time = shortTime(now)
        val room = (right - left) * if (weather != null) 0.58f else 0.8f
        val size = fit(time, room, 52f, 34f, bold = true)
        val tw = write(c, time, left, top + 74 * dp, size, white, bold = true)
        if (!h24) write(c, T.date("a", now), left + tw + 6 * dp, top + 74 * dp, 20f, white, bold = true)
        write(c, T.date("EEEEdMMMM", now), left + 2 * dp, top + 102 * dp, 15f, 0x99ffffff.toInt(), max = room + 30 * dp)
        // The weather in the corner, with Open-Meteo's credit.
        weather?.let { w ->
            val p = Isle.prefs
            write(c, Format.temperature(w.celsius, p.fahrenheit), right, top + 58 * dp, 30f, white, bold = true, align = Paint.Align.RIGHT)
            icon(c, Icons.sky(w.sky, w.day), right - 90 * dp, top + 46 * dp, 28f, white)
            if (w.highC != null && w.lowC != null) {
                write(c, T.f("H {}  L {}", Format.temperature(w.highC!!, p.fahrenheit), Format.temperature(w.lowC!!, p.fahrenheit)), right, top + 80 * dp, 12.5f, 0x99ffffff.toInt(), align = Paint.Align.RIGHT)
            }
            write(c, "Open-Meteo", right, top + 102 * dp, 10f, 0x66ffffff, align = Paint.Align.RIGHT)
        }
        // Row one: Wi-Fi and Bluetooth (blue while on, with the names), then three round buttons.
        val bh = 32 * dp
        val gap = 10 * dp
        val y1 = top + 122 * dp
        val tools = 3
        val chipW = (right - (left - 6 * dp) - tools * (bh + gap) - gap) / 2
        fun pill(x: Float, on: Boolean, i: Icon, label: String, action: () -> Unit) {
            fill.color = if (on) 0xe60a84ff.toInt() else faint
            rect.set(x, y1, x + chipW, y1 + bh)
            c.drawRoundRect(rect, bh / 2, bh / 2, fill)
            val fits = measure(label, 13f, true) <= chipW - 44 * dp
            if (!on || !fits && label.length <= 9) {
                icon(c, i, x + chipW / 2, y1 + bh / 2, 18f, white)
            } else {
                icon(c, i, x + 19 * dp, y1 + bh / 2, 18f, white)
                write(c, label, x + 34 * dp, y1 + bh / 2 + 4.5f * dp, 13f, white, bold = true, max = chipW - 44 * dp)
            }
            hits.add(Hit(RectF(x, y1 - 4 * dp, x + chipW, y1 + bh + 4 * dp), action))
        }
        val x0 = left - 6 * dp
        pill(x0, Isle.wifiOn, Icon.Wifi, Isle.wifiName ?: T.t("Wi-Fi")) { Controls.wifi() }
        pill(x0 + chipW + gap, Isle.btOn, Icon.Bluetooth, Isle.btName ?: T.t("Bluetooth")) { Controls.bluetooth() }
        fun round(i: Int, y: Float, ic: Icon, action: () -> Unit) {
            val cx = right - bh / 2 - (tools - 1 - i) * (bh + gap)
            fill.color = faint
            c.drawCircle(cx, y + bh / 2, bh / 2, fill)
            icon(c, ic, cx, y + bh / 2, 17f, white)
            hits.add(Hit(RectF(cx - bh / 2 - 4 * dp, y - 4 * dp, cx + bh / 2 + 4 * dp, y + bh + 4 * dp), action))
        }
        round(0, y1, Icon.Clipboard) { openApp(T.t("Todo")) }
        round(1, y1, Icon.Gauge) { openApp(T.t("Speed")) }
        round(2, y1, Icon.Toggles) {
            Isle.island.showToggles()
            Isle.changed()
        }
        // Row two: search on the left; calendar, screenshot and lock on the right.
        val y2 = top + 164 * dp
        val sw = measure(T.t("Search"), 13f, true) + 50 * dp
        fill.color = faint
        rect.set(x0, y2, x0 + sw, y2 + bh)
        c.drawRoundRect(rect, bh / 2, bh / 2, fill)
        icon(c, Icon.Search, x0 + 19 * dp, y2 + bh / 2, 16f, white)
        write(c, T.t("Search"), x0 + 34 * dp, y2 + bh / 2 + 4.5f * dp, 13f, white, bold = true)
        hits.add(Hit(RectF(x0, y2 - 4 * dp, x0 + sw, y2 + bh + 4 * dp)) { openLauncher() })
        round(0, y2, Icon.Calendar) {
            Isle.island.showCalendar()
            Isle.changed()
        }
        round(1, y2, Icon.Screenshot) { Controls.screenshot() }
        round(2, y2, Icon.Lock) { Controls.lock() }
    }

    private val bitmapPaint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    private var photo: android.graphics.Bitmap? = null
    private var photoStamp = -1L

    /** Control Centre's photo, when one is set (read again only when the file changes). */
    private fun ccPhoto(): android.graphics.Bitmap? {
        if (!Isle.prefs.ccPhoto) return null
        val f = java.io.File(context.filesDir, CC_PHOTO)
        val stamp = if (f.exists()) f.lastModified() else 0L
        if (stamp != photoStamp) {
            photoStamp = stamp
            photo = if (stamp == 0L) null else runCatching { android.graphics.BitmapFactory.decodeFile(f.path) }.getOrNull()
        }
        return photo
    }

    /** The way back to the card Control Centre was opened from ("Now Playing ›"), or null. */
    private fun backLabel(): String? {
        val from = Isle.island.openedFrom
        return when {
            from != null -> when (from) {
                is Live.Music -> T.t("Now Playing")
                is Live.Call -> T.t("Call")
                is Live.Navigation -> T.t("Directions")
                is Live.Timer, is Live.Countdown -> T.t("Timer")
                is Live.Stopwatch -> T.t("Stopwatch")
                is Live.Event -> T.t("Meeting")
                is Live.Progress -> T.t("Download")
                is Live.Lesson -> T.t("Lesson")
            }
            Isle.island.openedFromCalendar -> T.t("Calendar")
            else -> null
        }
    }

    private fun backButton(c: Canvas, back: String, left: Float, y: Float) {
        val w = measure(back, 12.5f, true) + 34 * dp
        fill.color = 0x24ffffff
        rect.set(left - 2 * dp, y, left - 2 * dp + w, y + 22 * dp)
        c.drawRoundRect(rect, 11 * dp, 11 * dp, fill)
        write(c, back, left + 9 * dp, y + 15.5f * dp, 12.5f, 0xe6ffffff.toInt(), bold = true)
        write(c, "›", rect.right - 13 * dp, y + 16.5f * dp, 16f, 0xe6ffffff.toInt(), bold = true, align = Paint.Align.CENTER)
        hits.add(Hit(RectF(rect.left, rect.top - 6 * dp, rect.right, rect.bottom + 6 * dp)) {
            Isle.island.back()
            Isle.changed()
        })
    }

    /** A Control Centre button: its icon, its name under it in the dock, and what a tap does. */
    private fun ccIcon(id: String) = when (id) {
        "wifi" -> Icon.Wifi
        "bluetooth" -> Icon.Bluetooth
        "search" -> Icon.Search
        "todo" -> Icon.Clipboard
        "speed" -> Icon.Gauge
        "controls" -> Icon.Toggles
        "calendar" -> Icon.Calendar
        "screenshot" -> Icon.Screenshot
        else -> Icon.Lock
    }

    private fun ccLabel(id: String) = when (id) {
        "wifi" -> T.t("Wi-Fi")
        "bluetooth" -> T.t("Bluetooth")
        "search" -> T.t("Search")
        "todo" -> T.t("To-do")
        "speed" -> T.t("Speed")
        "controls" -> T.t("Controls‖short button label")
        "calendar" -> T.t("Calendar")
        "screenshot" -> T.t("Capture‖short label for the screenshot button")
        else -> T.t("Lock")
    }

    private fun ccOn(id: String) = (id == "wifi" && Isle.wifiOn) || (id == "bluetooth" && Isle.btOn)

    private fun ccTap(id: String) {
        when (id) {
            "wifi" -> Controls.wifi()
            "bluetooth" -> Controls.bluetooth()
            "search" -> openLauncher()
            "todo" -> openApp(T.t("Todo"))
            "speed" -> openApp(T.t("Speed"))
            "controls" -> {
                Isle.island.showToggles()
                Isle.changed()
            }
            "calendar" -> {
                Isle.island.showCalendar()
                Isle.changed()
            }
            "screenshot" -> Controls.screenshot()
            else -> Controls.lock()
        }
    }

    private fun roundRect(c: Canvas, l: Float, t: Float, r: Float, b: Float, radius: Float, color: Int) {
        fill.color = color
        rect.set(l, t, r, b)
        c.drawRoundRect(rect, radius, radius, fill)
    }

    /** A round button with an icon (blue while on). */
    private fun disc(c: Canvas, id: String, cx: Float, cy: Float, d: Float, iconDp: Float) {
        fill.color = if (ccOn(id)) 0xeb0a84ff.toInt() else 0x29ffffff
        c.drawCircle(cx, cy, d / 2, fill)
        icon(c, ccIcon(id), cx, cy, iconDp, white)
    }

    /**
     * Control Centre in one of the newer layouts (Modules, Sliders, Dock),
     * with the buttons in the order set in the app.
     */
    private fun drawStyledCard(c: Canvas, box: RectF, style: String, photo: Boolean) {
        val left = box.left + 22 * dp
        val right = box.right - 22 * dp
        val back = backLabel()
        if (back != null) backButton(c, back, left + 2 * dp, box.top + 8 * dp)
        val top = box.top + (ControlCentre.TOP + if (back != null) ControlCentre.BACK else 0.0).toFloat() * dp
        val order = ControlCentre.order(Isle.prefs.ccOrder)
        val now = Isle.now()
        val dimText = 0x99ffffff.toInt()
        val weather = Isle.weather
        if (style == "dock") {
            if (!photo) {
                // A soft wash of the accent colour into near-black.
                val rgb = tintOf(Isle.prefs.tint) ?: ACCENTS[Isle.prefs.accent] ?: 0x9f8cff
                val deep = Color.rgb((rgb shr 16 and 0xff) * 30 / 100 + 10, (rgb shr 8 and 0xff) * 30 / 100 + 10, (rgb and 0xff) * 30 / 100 + 10)
                fill.shader = android.graphics.LinearGradient(box.left, box.top, box.right, box.bottom, deep, 0xff0a0a14.toInt(), android.graphics.Shader.TileMode.CLAMP)
                c.drawRect(box, fill)
                fill.shader = null
            }
            icon(c, Icon.Wifi, left + 8 * dp, top + 7 * dp, 15f, 0xb3ffffff.toInt())
            val net = if (!Isle.wifiOn) T.t("Wi-Fi off") else Isle.wifiName ?: T.t("Wi-Fi")
            write(c, net, left + 21 * dp, top + 12 * dp, 13f, 0xb3ffffff.toInt(), max = (right - left) / 2)
            weather?.let { w ->
                val temp = Format.temperature(w.celsius, Isle.prefs.fahrenheit)
                val tw = write(c, temp, right, top + 12 * dp, 14f, white, bold = true, align = Paint.Align.RIGHT)
                icon(c, Icons.sky(w.sky, w.day), right - tw - 13 * dp, top + 7 * dp, 18f, white)
                write(c, "Open-Meteo", right, top + 26 * dp, 9.5f, 0x66ffffff, align = Paint.Align.RIGHT)
            }
            val time = shortTime(now)
            val size = fit(time, (right - left) * 0.7f, 62f, 40f, bold = true)
            val tw = measure(time, size, true)
            val sw = if (h24) 0f else measure(T.date("a", now), 20f, true) + 6 * dp
            val x = box.centerX() - (tw + sw) / 2
            write(c, time, x, top + 76 * dp, size, white, bold = true)
            if (!h24) write(c, T.date("a", now), x + tw + 6 * dp, top + 76 * dp, 20f, 0xccffffff.toInt(), bold = true)
            write(c, T.date("EEEEdMMMM", now), box.centerX(), top + 100 * dp, 14f, dimText, align = Paint.Align.CENTER, max = right - left)
            // The dock: every button in one row, with its name under it.
            val dl = left - 6 * dp
            val dr = right + 6 * dp
            val dt = top + ControlCentre.DOCK_HEAD.toFloat() * dp
            roundRect(c, dl, dt, dr, dt + ControlCentre.DOCK.toFloat() * dp, 28 * dp, 0x17ffffff)
            val cw = (dr - dl) / maxOf(order.size, 6)
            val x0 = dl + ((dr - dl) - cw * order.size) / 2
            // One size for every name: the size the longest one fits at.
            val fs = order.minOf { fit(ccLabel(it), cw - 3 * dp, 10.5f, 8f) }
            order.forEachIndexed { i, id ->
                val cx = x0 + cw * (i + 0.5f)
                val cy = dt + 30 * dp
                val d = minOf(40 * dp, cw - 7 * dp)
                disc(c, id, cx, cy, d, minOf(19f, d / dp * 0.5f))
                val label = ccLabel(id)
                write(c, label, cx, dt + 66 * dp, fs, 0xc7ffffff.toInt(), align = Paint.Align.CENTER, max = cw - 3 * dp)
                hits.add(Hit(RectF(x0 + cw * i, dt + 4 * dp, x0 + cw * (i + 1), dt + ControlCentre.DOCK.toFloat() * dp - 4 * dp)) { ccTap(id) })
            }
            return
        }
        // Modules and Sliders: the time, the date and the weather on top.
        val time = shortTime(now)
        val size = fit(time, (right - left) * 0.55f, 40f, 30f, bold = true)
        val tw = write(c, time, left, top + 40 * dp, size, white, bold = true)
        if (!h24) write(c, T.date("a", now), left + tw + 5 * dp, top + 40 * dp, 17f, white, bold = true)
        write(c, T.date("EEEEdMMMM", now), left + 2 * dp, top + 62 * dp, 14f, dimText, max = (right - left) * if (weather != null) 0.66f else 1f)
        weather?.let { w ->
            val p = Isle.prefs
            val temp = Format.temperature(w.celsius, p.fahrenheit)
            val ww = write(c, temp, right, top + 31 * dp, 24f, white, bold = true, align = Paint.Align.RIGHT)
            icon(c, Icons.sky(w.sky, w.day), right - ww - 18 * dp, top + 22 * dp, 24f, white)
            if (w.highC != null && w.lowC != null) {
                write(c, T.f("H {}  L {}", Format.temperature(w.highC!!, p.fahrenheit), Format.temperature(w.lowC!!, p.fahrenheit)), right, top + 48 * dp, 12f, dimText, align = Paint.Align.RIGHT)
            }
            write(c, "Open-Meteo", right, top + 62 * dp, 9.5f, 0x66ffffff, align = Paint.Align.RIGHT)
        }
        val b = top + ControlCentre.HEAD.toFloat() * dp
        val bh = ControlCentre.BODY.toFloat() * dp
        val tile = 0x1affffff
        if (style == "modules") {
            // A block of four round buttons; search beside it; the rest under search.
            val others = order.filter { it != "search" }
            val d = 46 * dp
            val bw = 12 * dp + 2 * d + 10 * dp + 12 * dp
            roundRect(c, left, b, left + bw, b + bh, 26 * dp, tile)
            others.take(4).forEachIndexed { i, id ->
                val cx = left + 12 * dp + d / 2 + (i % 2) * (d + 10 * dp)
                val cy = b + 12 * dp + d / 2 + (i / 2) * (d + 10 * dp)
                disc(c, id, cx, cy, d, 21f)
                hits.add(Hit(RectF(cx - d / 2 - 4 * dp, cy - d / 2 - 4 * dp, cx + d / 2 + 4 * dp, cy + d / 2 + 4 * dp)) { ccTap(id) })
            }
            val rx = left + bw + 10 * dp
            val rest = others.drop(4).take(4)
            val rowH = (bh - 10 * dp) / 2
            var ry = b
            if ("search" in order) {
                roundRect(c, rx, b, right, b + rowH, 22 * dp, tile)
                icon(c, Icon.Search, rx + 24 * dp, b + rowH / 2, 20f, white)
                write(c, T.t("Search"), rx + 44 * dp, b + rowH / 2 + 5.5f * dp, 16f, white, bold = true, max = right - rx - 52 * dp)
                hits.add(Hit(RectF(rx, b, right, b + rowH)) { ccTap("search") })
                ry += rowH + 10 * dp
            }
            if (rest.isNotEmpty()) {
                val n = rest.size
                val w = ((right - rx) - (n - 1) * 8 * dp) / n
                rest.forEachIndexed { i, id ->
                    val x = rx + i * (w + 8 * dp)
                    roundRect(c, x, ry, x + w, ry + rowH, 18 * dp, tile)
                    icon(c, ccIcon(id), x + w / 2, ry + rowH / 2, 20f, white)
                    hits.add(Hit(RectF(x, ry, x + w, ry + rowH)) { ccTap(id) })
                }
            }
            return
        }
        // Sliders: volume and brightness, tall, beside a grid of the buttons.
        val sw = 54 * dp
        fun tall(x: Float, ic: Icon, value: Float, allowed: Boolean, set: (Float) -> Unit, ask: () -> Unit) {
            roundRect(c, x, b, x + sw, b + bh, 20 * dp, 0x21ffffff)
            val f = if (allowed) value.coerceIn(0f, 1f) * bh else 0f
            if (f > 1f) roundRect(c, x, b + bh - f, x + sw, b + bh, minOf(20 * dp, f / 2), 0xebffffff.toInt())
            val onFill = f > 36 * dp
            icon(c, ic, x + sw / 2, b + bh - 22 * dp, 22f, if (onFill) 0xbf000000.toInt() else white)
            if (allowed) {
                sliders.add(Slider(RectF(x, b, x + sw, b + bh), vertical = true, set = set))
            } else {
                hits.add(Hit(RectF(x, b, x + sw, b + bh), ask))
            }
        }
        tall(left, Icon.Sound, reading("volume") { Controls.volume }, true, { readings.remove("volume"); Controls.volume = it }) {}
        tall(left + sw + 10 * dp, Icon.Brightness, reading("brightness") { Controls.brightness }, reading("canWrite") { Controls.canSetBrightness }, { readings.remove("brightness"); Controls.brightness = it }) { Controls.askForBrightness() }
        val gx = left + 2 * (sw + 10 * dp)
        val gw = right - gx
        val rh = (bh - 16 * dp) / 3
        val u = (gw - 24 * dp) / 4
        fun units(id: String, wide: Boolean) = if (wide && (id == "wifi" || id == "bluetooth" || id == "search")) 2 else 1
        fun pack(wide: Boolean): List<Triple<String, Int, Int>> {
            var row = 0
            var col = 0
            val out = ArrayList<Triple<String, Int, Int>>()
            for (id in order) {
                val n = units(id, wide)
                if (col + n > 4) {
                    row++
                    col = 0
                }
                if (row >= 3) break
                out.add(Triple(id, row, col))
                col += n
            }
            return out
        }
        val wide = pack(true).size == order.size
        for ((id, row, col) in pack(wide)) {
            val n = units(id, wide)
            val x = gx + col * (u + 8 * dp)
            val y = b + row * (rh + 8 * dp)
            val w = n * u + (n - 1) * 8 * dp
            roundRect(c, x, y, x + w, y + rh, 16 * dp, tile)
            if (n == 2) {
                disc(c, id, x + 6 * dp + 13 * dp, y + rh / 2, 26 * dp, 15f)
                val state = when (id) {
                    "wifi" -> if (!Isle.wifiOn) T.t("Off") else Isle.wifiName ?: T.t("On")
                    "bluetooth" -> if (!Isle.btOn) T.t("Off") else Isle.btName ?: T.t("On")
                    else -> null
                }
                val tx = x + 38 * dp
                if (state != null) {
                    write(c, ccLabel(id), tx, y + rh / 2 - 1 * dp, 12.5f, white, bold = true, max = w - 42 * dp)
                    write(c, state, tx, y + rh / 2 + 12 * dp, 11f, dimText, max = w - 42 * dp)
                } else {
                    write(c, ccLabel(id), tx, y + rh / 2 + 4.5f * dp, 13f, white, bold = true, max = w - 42 * dp)
                }
            } else {
                icon(c, ccIcon(id), x + w / 2, y + rh / 2, 19f, white)
            }
            hits.add(Hit(RectF(x, y, x + w, y + rh)) { ccTap(id) })
        }
    }

    /** How wide `s` is drawn at `size` sp (px). */
    private fun measure(s: String, size: Float, bold: Boolean): Float {
        text.textSize = size * dp
        text.typeface = typeface(bold)
        return text.measureText(s)
    }

    private fun openLauncher() {
        Isle.island.close(Isle.now())
        Isle.changed()
        Isle.start(Intent(context, LauncherActivity::class.java))
    }

    private fun openApp(page: String) {
        Isle.island.close(Isle.now())
        Isle.changed()
        Isle.start(Intent(context, MainActivity::class.java).putExtra("page", page))
    }

    /** The strip for a tap on the empty island: battery left of the camera; the time and memory use right of it. */
    private fun drawPeek(c: Canvas, box: RectF) {
        val cy = box.centerY()
        val h = box.height()
        val level = Isle.battery.coerceAtLeast(0)
        val colour = when {
            Isle.charging || level > 20 -> green
            level > 10 -> orange
            else -> red
        }
        val bx = box.left + h * 0.55f + 12 * dp
        battery(c, bx, cy, level, colour)
        if (Isle.charging) icon(c, Icon.Bolt, bx, cy, 10f, Color.BLACK)
        val pw = write(c, "$level%", bx + 18 * dp, cy + 5.5f * dp, 15f, colour, bold = true)
        // The weather beside it, as the computer's island shows when pointed at.
        Isle.weather?.let { w ->
            val wx = bx + 18 * dp + pw + 12 * dp
            icon(c, Icons.sky(w.sky, w.day), wx + 8 * dp, cy, 15f, if (w.day) 0xffffd60a.toInt() else white)
            write(c, Format.temperature(w.celsius, Isle.prefs.fahrenheit), wx + 19 * dp, cy + 5.5f * dp, 15f, white, bold = true)
        }
        val right = box.right - h * 0.5f
        if (Isle.prefs.stripNetwork) {
            // Download speed instead of memory use.
            Isle.sampleNet()
            val down = Isle.netRate?.first?.let { Format.rate(it) } ?: "–"
            val nw = write(c, down, right, cy + 5.5f * dp, 14f, white, bold = true, align = Paint.Align.RIGHT)
            write(c, "↓", right - nw - 4 * dp, cy + 5.5f * dp, 14f, blue, bold = true, align = Paint.Align.RIGHT)
            write(c, timeText(Isle.now()), right - nw - 22 * dp, cy + 5.5f * dp, 15f, white, bold = true, align = Paint.Align.RIGHT)
            return
        }
        val mem = reading("memory") { memoryUse() }
        val mw = write(c, "$mem%", right, cy + 5.5f * dp, 15f, white, bold = true, align = Paint.Align.RIGHT)
        icon(c, Icon.Gauge, right - mw - 10 * dp, cy, 15f, white)
        write(c, timeText(Isle.now()), right - mw - 24 * dp, cy + 5.5f * dp, 15f, white, bold = true, align = Paint.Align.RIGHT)
    }

    /** How full the phone's memory is, 0–100 (Android keeps other apps' CPU use private). */
    private fun memoryUse(): Int {
        val am = context.getSystemService(android.app.ActivityManager::class.java) ?: return 0
        val info = android.app.ActivityManager.MemoryInfo()
        am.getMemoryInfo(info)
        return if (info.totalMem <= 0) 0 else ((info.totalMem - info.availMem) * 100 / info.totalMem).toInt()
    }

    /** The month, today ringed, and timer buttons below. */
    private fun drawCalendar(c: Canvas, box: RectF) {
        val pad = 20 * dp
        val l = box.left + pad
        val r = box.right - pad
        val t = box.top + pad
        val cal = java.util.Calendar.getInstance()
        val today = cal.get(java.util.Calendar.DAY_OF_MONTH)
        write(c, T.date("MMMMyyyy", cal.timeInMillis), l, t + 16 * dp, 17f, white, bold = true)
        cal.set(java.util.Calendar.DAY_OF_MONTH, 1)
        val firstDay = cal.firstDayOfWeek
        val offset = Math.floorMod(cal.get(java.util.Calendar.DAY_OF_WEEK) - firstDay, 7)
        val days = cal.getActualMaximum(java.util.Calendar.DAY_OF_MONTH)
        val colW = (r - l) / 7
        val names = java.text.DateFormatSymbols.getInstance().shortWeekdays
        for (i in 0 until 7) {
            val d = Math.floorMod(firstDay - 1 + i, 7) + 1
            write(c, names[d].take(2), l + colW * (i + 0.5f), t + 42 * dp, 11f, grey, align = Paint.Align.CENTER)
        }
        val rowH = 24 * dp
        for (day in 1..days) {
            val cell = offset + day - 1
            val x = l + colW * (cell % 7 + 0.5f)
            val y = t + 64 * dp + rowH * (cell / 7)
            if (day == today) {
                fill.color = 0xff0a84ff.toInt()
                c.drawCircle(x, y - 4.5f * dp, 11 * dp, fill)
            }
            write(c, day.toString(), x, y, 13f, white, bold = day == today, align = Paint.Align.CENTER)
        }
        val now = Isle.now()
        val by = box.bottom - pad - 14 * dp
        // Up to three lines above the buttons: the class, the next event, a deadline, the alarm.
        val rows = ArrayList<Triple<Icon, Int, String>>()
        val p = Isle.prefs
        if (p.lessons) {
            p.timetable.line(Timetable.weekday(now), Timetable.minuteOfDay(now), h24)?.let { rows.add(Triple(Icon.Book, cyan, it)) }
        }
        // Today's agenda (what's left of it); else the next event, another day.
        val midnight = java.util.Calendar.getInstance().apply {
            timeInMillis = now
            set(java.util.Calendar.HOUR_OF_DAY, 0)
            set(java.util.Calendar.MINUTE, 0)
            set(java.util.Calendar.SECOND, 0)
            set(java.util.Calendar.MILLISECOND, 0)
            add(java.util.Calendar.DAY_OF_MONTH, 1)
        }.timeInMillis
        val agenda = islet.island.Meetings.today(Isle.events, now, midnight)
        if (agenda.isNotEmpty()) {
            agenda.take(2).forEach { rows.add(Triple(Icon.Calendar, red, "${timeText(it.begin)}  ${it.title}")) }
        } else {
            islet.island.Meetings.next(Isle.events, now)?.let { rows.add(Triple(Icon.Calendar, red, "${it.title} · ${T.date("EEE", it.begin)} ${timeText(it.begin)}")) }
        }
        if (p.deadlines) {
            Deadlines.next(Isle.deadlines, now)?.let { rows.add(Triple(Icon.Flag, orange, "${it.title} · ${Deadlines.countdown(it, now)}")) }
        }
        Isle.nextAlarm?.let { rows.add(Triple(Icon.Bell, white, "${T.t("Alarm")} · ${T.date("EEE", it)} ${timeText(it)}")) }
        rows.take(3).forEachIndexed { i, (ic, colour, text) ->
            val y = by - 34 * dp - (2 - i) * 22 * dp + (3 - minOf(3, rows.size)) * 22 * dp
            icon(c, ic, l + 7 * dp, y - 4.5f * dp, 13f, colour)
            write(c, text, l + 22 * dp, y, 13f, grey, max = r - l - 22 * dp)
        }
        var x = l
        for (minutes in listOf(1, 5, 10, 25)) {
            x += chip(c, "${minutes}m", x, by, dim) { Isle.startFocus(minutes.toDouble()); Isle.island.open("timer"); Isle.changed() } + 8 * dp
        }
        // Focus fits between the timer chips and the two round buttons on most phones.
        text.textSize = 13 * dp
        text.typeface = typeface(true)
        if (x + text.measureText(T.t("Focus")) + 24 * dp + 6 * dp <= r - 67 * dp) chip(c, T.t("Focus"), x, by, 0xff5a3a00.toInt(), orange) { Isle.startPomodoro(); Isle.island.open("timer"); Isle.changed() }
        button(c, r - 52 * dp, by, 30f, dim, Icon.Bell) { openApp(T.t("Alarms")) }
        button(c, r - 15 * dp, by, 30f, dim, Icon.Stopwatch) {
            Isle.stopwatch.start(Isle.now())
            Isle.syncTimers()
            Isle.island.open("stopwatch")
            Isle.changed()
        }
    }

    // ---- Control centre -----------------------------------------------------------------------

    private fun drawControls(c: Canvas, box: RectF) {
        val pad = 16 * dp
        val l = box.left + pad
        val r = box.right - pad
        // A back button to the card it came from, and the title.
        val hy = box.top + pad + 14 * dp
        chip(c, T.t("‹  Back"), l, hy, dim) {
            Isle.island.back()
            Isle.changed()
        }
        write(c, T.t("Controls"), r, hy + 5 * dp, 15f, white, bold = true, align = Paint.Align.RIGHT)
        val t = box.top + pad + 40 * dp
        val tile = 48 * dp
        val gap = ((r - l) - 4 * tile) / 3
        fun tileAt(i: Int, on: Boolean, colour: Int, ic: Icon, action: () -> Unit) {
            val col = i % 4
            val row = i / 4
            val x = l + col * (tile + gap)
            val y = t + row * (tile + 10 * dp)
            fill.color = if (on) colour else dim
            rect.set(x, y, x + tile, y + tile)
            c.drawRoundRect(rect, 16 * dp, 16 * dp, fill)
            icon(c, ic, x + tile / 2, y + tile / 2, 22f, if (on) Color.BLACK else white)
            hits.add(Hit(RectF(rect)) {
                action()
                postDelayed({ invalidate() }, 300)
            })
        }
        tileAt(0, Isle.torchOn, white, Icon.Flashlight) { Controls.flashlight() }
        tileAt(1, Isle.dnd, purple, Icon.Moon) { Controls.doNotDisturb() }
        val vibrate = reading("ringer") { Controls.ringerMode } != android.media.AudioManager.RINGER_MODE_NORMAL
        tileAt(2, vibrate, orange, if (vibrate) Icon.Vibrate else Icon.Bell) { Controls.ringer() }
        tileAt(3, Isle.airplane, orange, Icon.Plane) { Controls.airplane() }
        tileAt(4, false, blue, Icon.Wifi) { Controls.wifi() }
        tileAt(5, false, blue, Icon.Bluetooth) { Controls.bluetooth() }
        tileAt(6, false, white, Icon.Screenshot) { Controls.screenshot() }
        tileAt(7, false, white, Icon.Lock) { Controls.lock() }
        var y = t + 2 * tile + 10 * dp + 22 * dp
        fun slider(ic: Icon, value: Float, allowed: Boolean, set: (Float) -> Unit, ask: () -> Unit) {
            val h = 28 * dp
            fill.color = dim
            rect.set(l, y - h / 2, r, y + h / 2)
            c.drawRoundRect(rect, h / 2, h / 2, fill)
            if (allowed) {
                fill.color = white
                rect.set(l, y - h / 2, l + h + (r - l - h) * value.coerceIn(0f, 1f), y + h / 2)
                c.drawRoundRect(rect, h / 2, h / 2, fill)
                icon(c, ic, l + h / 2, y, 16f, Color.BLACK)
                sliders.add(Slider(RectF(l, y - h / 2, r, y + h / 2), set = set))
            } else {
                icon(c, ic, l + h / 2, y, 16f, white)
                write(c, T.t("Tap to allow"), l + h + 8 * dp, y + 5 * dp, 13f, grey)
                hits.add(Hit(RectF(l, y - h / 2, r, y + h / 2), ask))
            }
            y += h + 10 * dp
        }
        // While a slider is dragged it shows the finger's place; otherwise the system's, read once a second.
        slider(Icon.Sound, reading("volume") { Controls.volume }, true, { readings.remove("volume"); Controls.volume = it }) {}
        slider(Icon.Brightness, reading("brightness") { Controls.brightness }, reading("canWrite") { Controls.canSetBrightness }, { readings.remove("brightness"); Controls.brightness = it }) { Controls.askForBrightness() }
        val apps = Isle.prefs.shortcuts
        var x = l + 14 * dp
        for (pkg in apps) {
            val d = Isle.appIcon(pkg) ?: continue
            drawable(c, d, x, y + 4 * dp, 28f)
            hits.add(Hit(RectF(x - 16 * dp, y - 12 * dp, x + 16 * dp, y + 20 * dp)) {
                Isle.island.close(Isle.now())
                Isle.changed()
                Isle.launch(pkg)
            })
            x += 44 * dp
        }
        button(c, r - 14 * dp, y + 4 * dp, 28f, dim, Icon.Gear) {
            Isle.island.close(Isle.now())
            Isle.changed()
            Isle.start(Intent(context, MainActivity::class.java))
        }
    }

    // ---- Touch ---------------------------------------------------------------------------------

    private val gestures = GestureDetector(
        context,
        object : GestureDetector.SimpleOnGestureListener() {
            override fun onDown(e: MotionEvent) = true

            override fun onLongPress(e: MotionEvent) {
                if (dragging != null) return
                longPressed = true
                performHapticFeedback(android.view.HapticFeedbackConstants.LONG_PRESS)
                Isle.island.longPress()
                Isle.changed()
            }

            override fun onFling(e1: MotionEvent?, e2: MotionEvent, vx: Float, vy: Float): Boolean {
                if (dragging != null || longPressed) return false
                // Only a real swipe counts: a thumb tap that slides a little is still a tap.
                val dx = e2.rawX - downX
                val dy = e2.rawY - downY
                if (kotlin.math.hypot(dx, dy) < SWIPE * dp) return false
                swiped = true
                if (abs(dy) > abs(dx) && dy < 0) {
                    Isle.island.close(Isle.now())
                } else if (abs(dx) > abs(dy) && !Isle.island.isOpen) {
                    Isle.island.cycle()
                } else if (abs(dy) > abs(dx) && dy > 0 && !Isle.island.isOpen) {
                    Isle.island.tap(Isle.now())
                }
                Isle.changed()
                return true
            }
        },
    )

    /** A tap at (x, y) on screen: a button if there's one there (with some room round it), else the island itself. */
    /** When the last tap was handled: a second one straight after is the same tap (a double tap would undo it). */
    private var lastTap = -TAP_GAP

    private fun tapAt(x: Float, y: Float) {
        val now = SystemClock.uptimeMillis()
        if (now - lastTap < TAP_GAP) return
        lastTap = now
        performHapticFeedback(android.view.HapticFeedbackConstants.CONTEXT_CLICK)
        val near = 8 * dp
        hits.lastOrNull { RectF(it.rect).apply { inset(-near, -near) }.contains(x, y) }?.let {
            it.action()
            Isle.changed()
            return
        }
        tapBackground()
        Isle.changed()
    }

    private fun tapBackground() {
        val now = Isle.now()
        when (val s = scene) {
            is Scene.Popped -> {
                when (val p = s.pop) {
                    is Pop.Note -> Isle.open(p.key)
                    is Pop.Screenshot -> Isle.start(Intent(Intent.ACTION_VIEW, Uri.parse(p.uri)).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION))
                    else -> Unit
                }
                Isle.island.dismissPop(now)
            }
            // An open card stays open when tapped (only its buttons act):
            // swipe up, tap outside or press back to close it.
            is Scene.Expanded, Scene.Controls, Scene.Toggles, Scene.Calendar -> Unit
            else -> Isle.island.tap(now)
        }
        Isle.changed()
    }

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(e: MotionEvent): Boolean {
        getLocationOnScreen(at)
        return touch(e, at[0], at[1])
    }

    /**
     * A touch, from this view or from the window that catches touches for
     * it (see IslandWindow): (ox, oy) is where the touched view is on screen.
     */
    fun touch(e: MotionEvent, ox: Int, oy: Int): Boolean {
        if (e.actionMasked == MotionEvent.ACTION_OUTSIDE) {
            if (Isle.island.isOpen) {
                Isle.island.close(Isle.now())
                Isle.changed()
            }
            return false
        }
        val x = e.x + ox
        val y = e.y + oy
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                dragging = sliders.firstOrNull { it.rect.contains(x, y) }
                downX = e.rawX
                downY = e.rawY
                downAt = e.eventTime
                longPressed = false
                swiped = false
            }
            MotionEvent.ACTION_MOVE -> dragging?.let {
                it.set(it.at(x, y))
                invalidate()
                return true
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> dragging?.let {
                it.set(it.at(x, y))
                dragging = null
                seekTo?.let { f ->
                    val m = Isle.island.get("music") as? Live.Music
                    if (m != null && m.duration > 0) Isle.seek((f * m.duration).toLong())
                    seekTo = null
                }
                invalidate()
                return true
            }
        }
        gestures.onTouchEvent(e)
        // Our own tap: more forgiving than Android's (a thumb moves a little,
        // and a quick flick that barely moves is a tap, not a swipe).
        if (e.actionMasked == MotionEvent.ACTION_UP && !longPressed && !swiped &&
            kotlin.math.hypot(e.rawX - downX, e.rawY - downY) < TAP_SLOP * dp && e.eventTime - downAt < 700
        ) {
            tapAt(x, y)
        }
        return true
    }

    private var downX = 0f
    private var downY = 0f
    private var downAt = 0L
    private var longPressed = false
    private var swiped = false

    override fun performClick(): Boolean {
        super.performClick()
        tapBackground()
        return true
    }
}
