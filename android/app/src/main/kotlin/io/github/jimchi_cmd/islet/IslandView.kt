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
import islet.island.Deadlines
import islet.island.Format
import islet.island.Geometry
import islet.island.Live
import islet.island.Lists
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
    private val morph = Spring(1.0)
    private val fade = Spring(1.0, stiffness = 1800.0, damping = 1.0)
    private var lastFrame = 0L

    /** Buttons drawn this frame, in view pixels. */
    private class Hit(val rect: RectF, val action: () -> Unit)

    private class Slider(val rect: RectF, val set: (Float) -> Unit)

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

    private companion object {
        /** The rim's glow, outside in: (width in dp, colour). */
        /** How far (dp) a finger may move and still tap; how far it must move to swipe. */
        const val TAP_SLOP = 24f
        const val SWIPE = 36f

        val GLOW = listOf(7f to 0x149f8cff, 4.5f to 0x2a9f8cff, 2.5f to 0x559f8cff, 1.3f to 0xffd6ccff.toInt())
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
            if (fade.target == 1.0 && fade.settled) shown = next
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
        fade.snap(1.0)
        fade.target = 0.0
        lastFrame = 0
        window.place((boxFor(next) ?: current).union(current))
        announce(next)
        postInvalidateOnAnimation()
    }

    /** Jump to the end of any change (tests, and drawing for a preview). */
    fun settle() {
        morph.snap(1.0)
        fade.snap(1.0)
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
        a is Scene.Popped && b is Scene.Popped -> a.pop == b.pop
        else -> a == b
    }

    /** The pill's box as drawn now, in dp (for tests). */
    internal fun drawnBox(): Box? = currentBox()

    private fun currentBox(): Box? {
        val f = from ?: return to
        val t = to ?: return f
        return f.lerp(t, morph.value)
    }

    private fun announce(s: Scene) {
        val words = when (s) {
            is Scene.Popped -> when (val p = s.pop) {
                is Pop.Note -> "Notification from ${p.app}"
                is Pop.Charging -> if (p.charging) "Charging, ${p.level} percent" else "Not charging"
                is Pop.LowBattery -> "Low battery, ${p.level} percent"
                is Pop.TimerDone -> p.label
                is Pop.Info -> p.text
                else -> null
            }
            is Scene.Compact -> if (s.live is Live.Call) "Call: ${(s.live as Live.Call).name}" else null
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
        val fast = (s is Scene.Compact && s.live is Live.Music && (s.live as Live.Music).playing && Isle.prefs.bars) ||
            (s is Scene.Expanded && s.live is Live.Music && (s.live as Live.Music).playing) ||
            (s is Scene.Expanded && s.live is Live.Stopwatch && (s.live as Live.Stopwatch).running)
        val ticking = fast || s is Scene.Compact || s is Scene.Expanded || s is Scene.Peek || s is Scene.Calendar
        if (ticking) {
            postDelayed(tick, if (fast) 33L else 1000L)
        } else if (s == Scene.Idle && Isle.prefs.idleClock) {
            // The empty island's time: redraw when the minute changes.
            postDelayed(tick, 60_000L - Isle.now() % 60_000L + 50)
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
        val now = SystemClock.uptimeMillis()
        val dt = if (lastFrame == 0L) 1.0 / 60 else (now - lastFrame) / 1000.0
        lastFrame = now
        val moving = !morph.settled || !fade.settled
        if (!morph.settled) morph.step(dt)
        if (!fade.settled) {
            fade.step(dt)
            // Half way out, swap to the new content and fade it in.
            if (fade.target == 0.0 && fade.value < 0.15) {
                shown = scene
                fade.target = 1.0
            }
        }
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
        // The pill: black, or frosted glass with a light rim.
        rect.set(l, t, r, b)
        if (Isle.prefs.style == "glass") {
            fill.color = 0xd92a2a30.toInt()
            c.drawRoundRect(rect, radius, radius, fill)
            fill.shader = android.graphics.LinearGradient(0f, t, 0f, t + (b - t) * 0.6f, 0x33ffffff, 0x00ffffff, android.graphics.Shader.TileMode.CLAMP)
            c.drawRoundRect(rect, radius, radius, fill)
            fill.shader = null
            line.color = 0x40ffffff
            line.strokeWidth = 1f * dp
            rect.set(l + dp / 2, t + dp / 2, r - dp / 2, b - dp / 2)
            c.drawRoundRect(rect, radius, radius, line)
        } else {
            fill.color = Color.BLACK
            c.drawRoundRect(rect, radius, radius, fill)
        }
        if (Isle.prefs.ring && borderFor(shown) == null && scene != Scene.Hidden) {
            // The glow as a few soft strokes: a blurred shadow costs far more every frame.
            rect.set(l + 0.75f * dp, t + 0.75f * dp, r - 0.75f * dp, b - 0.75f * dp)
            for ((width, colour) in GLOW) {
                line.color = colour
                line.strokeWidth = width * dp
                c.drawRoundRect(rect, radius, radius, line)
            }
        }
        borderFor(shown)?.let { colour ->
            line.color = colour
            line.strokeWidth = 2f * dp
            rect.set(l + dp, t + dp, r - dp, b - dp)
            c.drawRoundRect(rect, radius - dp, radius - dp, line)
        }
        val s = shown
        if (s is Scene.Compact && s.second != null && morph.settled) drawBubble(c, s.second!!, geometry.bubble(box))
        // The content, clipped to the pill.
        clip.reset()
        rect.set(l, t, r, b)
        clip.addRoundRect(rect, radius, radius, Path.Direction.CW)
        c.save()
        c.clipPath(clip)
        val alpha = (fade.value.coerceIn(0.0, 1.0) * 255).toInt()
        // An offscreen layer only while fading: drawing straight is much cheaper.
        if (alpha < 255) c.saveLayerAlpha(rect, alpha) else c.save()
        if (morph.value > 0.2 || morph.settled) drawScene(c, s, RectF(l, t, r, b))
        c.restore()
        c.restore()
        c.restore()
    }

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
            Scene.Controls -> drawControls(c, box)
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

    private fun icon(c: Canvas, i: Icon, cx: Float, cy: Float, sizeDp: Float, color: Int) =
        Icons.draw(c, i, cx, cy, sizeDp * dp, color)

    /** A round button; `action` runs when it is tapped. */
    private fun button(c: Canvas, cx: Float, cy: Float, dDp: Float, bg: Int, i: Icon, fg: Int = white, action: () -> Unit) {
        fill.color = bg
        c.drawCircle(cx, cy, dDp * dp / 2, fill)
        icon(c, i, cx, cy, dDp * 0.46f, fg)
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
        c.drawBitmap(b, null, RectF(l, t, l + s, t + s), fill.apply { color = white })
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
            write(c, shortTime(Isle.now()), box.left + h * 0.5f, cy + 5 * dp, 14f, white, bold = true)
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
                    battery(c, right - 13 * dp, cy, level, colour)
                    if (Isle.charging) icon(c, Icon.Bolt, right - 13 * dp, cy, 10f, Color.BLACK)
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
        val cy = box.centerY()
        val h = box.height()
        val lx = box.left + h * 0.62f
        val rx = box.right - h * 0.5f
        val wing = (geometry.wing * dp).toFloat() - 8 * dp
        val now = Isle.now()
        when (live) {
            is Live.Music -> {
                if (!bitmap(c, Isle.art, lx - 11 * dp, cy - 11 * dp, 22f, 6f)) icon(c, Icon.Music, lx, cy, 16f, white)
                bars(c, rx + 8 * dp, cy, 16f, Isle.artColor?.let { it or 0xff000000.toInt() } ?: white, live.playing)
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
                ring(c, rx, cy, 8f, if (live.percent < 0) -1f else live.percent / 100f, blue)
            }
            is Live.Lesson -> {
                icon(c, Icon.Book, lx, cy, 16f, cyan)
                val text = if (live.started(now)) Format.clock(live.endsAt - now) else Format.until(live.startsAt - now).removePrefix("in ")
                write(c, text, rx + 8 * dp, cy + 5 * dp, 13f, if (live.started(now)) cyan else white, bold = true, max = wing, align = Paint.Align.RIGHT)
            }
        }
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
            is Live.Music -> if (!bitmap(c, Isle.art, cx - 9 * dp, cy - 9 * dp, 18f, 9f)) icon(c, Icon.Music, cx, cy, 14f, white)
            is Live.Call -> icon(c, Icon.Phone, cx, cy, 14f, green)
            is Live.Navigation -> icon(c, Icon.Arrow, cx, cy, 14f, green)
            is Live.Timer, is Live.Countdown -> icon(c, Icon.Timer, cx, cy, 14f, orange)
            is Live.Stopwatch -> icon(c, Icon.Stopwatch, cx, cy, 14f, orange)
            is Live.Event -> icon(c, Icon.Calendar, cx, cy, 14f, red)
            is Live.Progress -> ring(c, cx, cy, 7f, if (live.percent < 0) -1f else live.percent / 100f, blue)
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
                val body = if (locked) "New notification" else p.text.ifEmpty { p.app }
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
                write(c, "Screenshot saved", x, cy - 3 * dp, 15f, white, bold = true)
                write(c, "Tap to open it", x, cy + 16 * dp, 13f, grey)
            }
            is Pop.TimerDone -> {
                icon(c, Icon.Timer, box.left + pad + 18 * dp, cy, 26f, orange)
                write(c, "Time's up", box.left + pad + 46 * dp, cy - 3 * dp, 15f, orange, bold = true)
                write(c, p.label, box.left + pad + 46 * dp, cy + 16 * dp, 13f, grey, max = box.width() - 160 * dp)
                chip(c, "OK", box.right - pad - 56 * dp, cy, dim) { Isle.island.dismissPop(Isle.now()); Isle.changed() }
            }
            is Pop.Bluetooth -> {
                icon(c, Icon.Bluetooth, box.left + pad + 18 * dp, cy, 24f, blue)
                write(c, p.name, box.left + pad + 46 * dp, cy - 3 * dp, 15f, white, bold = true, max = box.width() - 90 * dp)
                write(c, if (p.connected) "Connected" else "Disconnected", box.left + pad + 46 * dp, cy + 16 * dp, 13f, grey)
            }
            else -> {
                val lx = box.left + h * 0.6f
                val rx = box.right - h * 0.55f
                val (i, colour, left, right) = when (p) {
                    is Pop.Charging -> Quad(Icon.Bolt, if (p.charging) green else grey, if (p.charging) "Charging" else "Not charging", "${p.level}%")
                    is Pop.LowBattery -> Quad(Icon.Bolt, red, "Low battery", "${p.level}%")
                    is Pop.Ringer -> when (p.mode) {
                        Pop.RingerMode.Silent -> Quad(Icon.BellOff, red, "", "Silent")
                        Pop.RingerMode.Vibrate -> Quad(Icon.Vibrate, orange, "", "Vibrate")
                        Pop.RingerMode.Ring -> Quad(Icon.Bell, white, "", "Ring")
                    }
                    is Pop.Airplane -> Quad(Icon.Plane, orange, "Airplane mode", if (p.on) "On" else "Off")
                    is Pop.DoNotDisturb -> Quad(Icon.Moon, purple, "Do Not Disturb", if (p.on) "On" else "Off")
                    is Pop.Wifi -> Quad(Icon.Wifi, blue, p.name.ifEmpty { "Wi-Fi" }, "Connected")
                    is Pop.Info -> Quad(null, white, p.text, "")
                    is Pop.Payment -> Quad(Icon.Check, green, "Paid", p.amount)
                    else -> Quad(null, white, "", "")
                }
                if (i == null) {
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

    private fun battery(c: Canvas, cx: Float, cy: Float, level: Int, color: Int) {
        val w = 24 * dp
        val h = 12 * dp
        line.strokeWidth = 1.4f * dp
        line.color = grey
        rect.set(cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2)
        c.drawRoundRect(rect, 3.5f * dp, 3.5f * dp, line)
        fill.color = grey
        c.drawRect(cx + w / 2 + 1 * dp, cy - 2.5f * dp, cx + w / 2 + 2.5f * dp, cy + 2.5f * dp, fill)
        fill.color = color
        rect.set(cx - w / 2 + 2 * dp, cy - h / 2 + 2 * dp, cx - w / 2 + 2 * dp + (w - 4 * dp) * level / 100f, cy + h / 2 - 2 * dp)
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

    private fun drawExpanded(c: Canvas, live: Live?, box: RectF) {
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
                    live.incoming -> "Incoming call"
                    live.since > 0 -> Format.elapsed(now - live.since)
                    else -> "Calling…"
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
                if (actions.isEmpty()) chip(c, "Open", r - 70 * dp, cy, dim) { Isle.open(live.id) }
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
                chip(c, "Open ${live.app}", l + 62 * dp, cy + 30 * dp, green, Color.BLACK) { Isle.open(live.id) }
            }
            is Live.Timer -> {
                val streak = if (live.label == "Focus" || live.onBreak) Isle.prefs.streak.label(Streak.dayOf(now)) else null
                write(c, listOfNotNull(if (live.onBreak) "Break" else live.label, streak).joinToString(" · "), l, t + 14 * dp, 13f, grey, max = r - l - 160 * dp)
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
                chip(c, "Open", r - 70 * dp, box.bottom - pad - 20 * dp, dim) { Isle.open(live.id) }
            }
            is Live.Stopwatch -> {
                write(c, "Stopwatch", l, t + 14 * dp, 13f, grey)
                write(c, Format.stopwatch(live.elapsed(now)), l, t + 62 * dp, 42f, white, bold = true, max = r - l - 130 * dp)
                if (live.laps.isNotEmpty()) {
                    val parts = ArrayList<String>()
                    val first = maxOf(0, live.laps.size - 3)
                    for (n in live.laps.size - 1 downTo first) parts.add("Lap " + (n + 1) + "  " + Format.stopwatch(live.laps[n]))
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
                write(c, "${timeText(live.startsAt)} · ${if (starts == "now") "now" else "starts $starts"}", l + 36 * dp, t + 44 * dp, 13f, grey)
                val cy = box.bottom - pad - 18 * dp
                var x = l
                live.join?.let { url -> x += chip(c, "Join", x, cy, green, Color.BLACK) { Isle.openUrl(url) } + 10 * dp }
                chip(c, "Open calendar", x, cy, dim) {
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
                val label = if (started) "${Format.clock(live.endsAt - now)} left" else "Starts ${Format.until(live.startsAt - now)}"
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
                bar(c, l, r, box.bottom - pad - 14 * dp, if (live.percent < 0) 0f else live.percent / 100f, blue, 6f)
                hits.add(Hit(RectF(box)) { Isle.open(live.id) })
            }
        }
    }

    private fun drawMusic(c: Canvas, m: Live.Music, box: RectF) {
        val pad = 20 * dp
        val l = box.left + pad
        val r = box.right - pad
        val t = box.top + pad
        val accent = Isle.artColor?.let { it or 0xff000000.toInt() } ?: white
        if (!bitmap(c, Isle.art, l, t, 56f, 12f)) {
            fill.color = 0xff3a3a3c.toInt()
            rect.set(l, t, l + 56 * dp, t + 56 * dp)
            c.drawRoundRect(rect, 12 * dp, 12 * dp, fill)
            icon(c, Icon.Music, l + 28 * dp, t + 28 * dp, 26f, white)
        }
        hits.add(Hit(RectF(l, t, l + 56 * dp, t + 56 * dp)) { Isle.openPlayer() })
        val tx = l + 70 * dp
        write(c, m.title, tx, t + 22 * dp, 16f, white, bold = true, max = r - tx - 34 * dp)
        write(c, m.artist.ifEmpty { m.app }, tx, t + 44 * dp, 14f, grey, max = r - tx - 34 * dp)
        bars(c, r, t + 16 * dp, 18f, accent, m.playing)
        val position = seekTo?.let { (it * m.duration).toLong() } ?: Isle.mediaPositionNow()
        var y = t + 56 * dp
        val lyric = Isle.lyrics?.lineAt(position, Isle.prefs.lyricsOffset)
            ?: if (Isle.prefs.lyrics && Isle.lyricsMissing) "No synced lyrics for this song" else null
        lyric?.let {
            write(c, it, box.centerX(), y + 20 * dp, 13f, if (Isle.lyrics != null) accent else grey, bold = Isle.lyrics != null, max = r - l, align = Paint.Align.CENTER)
            y += 8 * dp
        }
        if (m.duration > 0) {
            val by = y + 30 * dp
            val bl = l + 40 * dp
            val br = r - 40 * dp
            bar(c, bl, br, by, position.toFloat() / m.duration, white)
            write(c, Format.elapsed(position), l, by + 4 * dp, 11f, grey)
            write(c, "-" + Format.elapsed(m.duration - position), r, by + 4 * dp, 11f, grey, align = Paint.Align.RIGHT)
            seeking = RectF(bl - 8 * dp, by - 14 * dp, br + 8 * dp, by + 14 * dp)
            sliders.add(Slider(seeking!!) { seekTo = it })
        }
        val cy = box.bottom - pad - 16 * dp
        val cx = box.centerX()
        button(c, cx - 64 * dp, cy, 44f, Color.TRANSPARENT, Icon.Previous) { Isle.previous() }
        button(c, cx, cy, 48f, Color.TRANSPARENT, if (m.playing) Icon.Pause else Icon.Play) { Isle.playPause() }
        button(c, cx + 64 * dp, cy, 44f, Color.TRANSPARENT, Icon.Next) { Isle.next() }
        // The sleep timer: off → 15 → 30 → 45 → 60 minutes.
        val sleep = Isle.sleepMinutesLeft()
        button(c, l + 16 * dp, cy, 36f, if (sleep != null) 0xff2c2a5a.toInt() else Color.TRANSPARENT, Icon.Moon, if (sleep != null) 0xffbf9bff.toInt() else grey) { Isle.stepSleep() }
        if (sleep != null) write(c, "${sleep}m", l + 38 * dp, cy + 4.5f * dp, 12f, 0xffbf9bff.toInt(), bold = true)
        // Lyrics on and off (from LRCLIB, only once switched on).
        val on = Isle.prefs.lyrics
        button(c, r - 16 * dp, cy, 36f, if (on) 0xff3a2a10.toInt() else Color.TRANSPARENT, Icon.Quote, if (on) accentOr(orange) else grey) { Isle.toggleLyrics() }
    }

    private fun accentOr(fallback: Int): Int = Isle.artColor?.let { it or 0xff000000.toInt() } ?: fallback

    /** The clock card, as on the computer: the time, the date, Wi-Fi and Bluetooth, and round buttons. */
    private fun drawClockCard(c: Canvas, box: RectF) {
        val pad = 22 * dp
        val l = box.left + pad
        val r = box.right - pad
        val t = box.top + pad
        val now = Isle.now()
        write(c, timeText(now), l - 2 * dp, t + 50 * dp, 54f, white, bold = true, max = (r - l) * 0.7f)
        write(c, DateFormat.format("EEEE, d MMMM", now).toString(), l, t + 76 * dp, 15f, grey)
        Isle.weather?.let { w ->
            val p = Isle.prefs
            write(c, Format.temperature(w.celsius, p.fahrenheit), r, t + 30 * dp, 22f, white, bold = true, align = Paint.Align.RIGHT)
            icon(c, Icons.sky(w.sky, w.day), r - 62 * dp, t + 22 * dp, 22f, if (w.day) 0xffffd60a.toInt() else white)
            write(c, w.words, r, t + 50 * dp, 12f, grey, max = (r - l) * 0.3f, align = Paint.Align.RIGHT)
        }
        // Wi-Fi and Bluetooth: blue while on, with the network's and device's names.
        val cy = t + 112 * dp
        val chipW = (r - l - 10 * dp) / 2
        fun pill(x: Float, on: Boolean, i: Icon, label: String, action: () -> Unit) {
            fill.color = if (on) 0xff0a84ff.toInt() else dim
            rect.set(x, cy - 20 * dp, x + chipW, cy + 20 * dp)
            c.drawRoundRect(rect, 20 * dp, 20 * dp, fill)
            icon(c, i, x + 22 * dp, cy, 18f, white)
            write(c, label, x + 38 * dp, cy + 5.5f * dp, 15f, white, bold = true, max = chipW - 50 * dp)
            hits.add(Hit(RectF(rect), action))
        }
        pill(l, Isle.wifiOn, Icon.Wifi, Isle.wifiName ?: if (Isle.wifiOn) "Wi-Fi" else "Wi-Fi off") { Controls.wifi() }
        pill(l + chipW + 10 * dp, Isle.btOn, Icon.Bluetooth, Isle.btName ?: if (Isle.btOn) "Bluetooth" else "Bluetooth off") { Controls.bluetooth() }
        // Round buttons, spread across the card.
        val by = cy + 52 * dp
        val buttons = listOf<Pair<Icon, () -> Unit>>(
            Icon.Search to { openLauncher() },
            Icon.Clipboard to { openApp("Todo") },
            Icon.Gauge to { openApp("Speed") },
            Icon.Toggles to { Isle.island.showControls() },
            Icon.Calendar to { Isle.island.showCalendar() },
            Icon.Screenshot to { Controls.screenshot() },
            Icon.Lock to { Controls.lock() },
        )
        val step = (r - l - 40 * dp) / (buttons.size - 1)
        buttons.forEachIndexed { i, (ic, action) ->
            button(c, l + 20 * dp + i * step, by, 40f, dim, ic, action = action)
        }
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
        write(c, "$level%", bx + 18 * dp, cy + 5.5f * dp, 15f, colour, bold = true)
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
        write(c, DateFormat.format("MMMM yyyy", cal).toString(), l, t + 16 * dp, 17f, white, bold = true)
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
        islet.island.Meetings.next(Isle.events, now)?.let { rows.add(Triple(Icon.Calendar, red, "${it.title} · ${timeText(it.begin)}")) }
        if (p.deadlines) {
            Deadlines.next(Isle.deadlines, now)?.let { rows.add(Triple(Icon.Flag, orange, "${it.title} · ${Deadlines.countdown(it, now)}")) }
        }
        Isle.nextAlarm?.let { rows.add(Triple(Icon.Bell, white, "Alarm · ${DateFormat.format("EEE", it)} ${timeText(it)}")) }
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
        if (x + text.measureText("Focus") + 24 * dp + 6 * dp <= r - 67 * dp) chip(c, "Focus", x, by, 0xff5a3a00.toInt(), orange) { Isle.startPomodoro(); Isle.island.open("timer"); Isle.changed() }
        button(c, r - 52 * dp, by, 30f, dim, Icon.Bell) { openApp("Alarms") }
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
        val t = box.top + pad
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
                sliders.add(Slider(RectF(l, y - h / 2, r, y + h / 2), set))
            } else {
                icon(c, ic, l + h / 2, y, 16f, white)
                write(c, "Tap to allow", l + h + 8 * dp, y + 5 * dp, 13f, grey)
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
    private fun tapAt(x: Float, y: Float) {
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
            else -> Isle.island.tap(now)
        }
        Isle.changed()
    }

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(e: MotionEvent): Boolean {
        if (e.actionMasked == MotionEvent.ACTION_OUTSIDE) {
            if (Isle.island.isOpen) {
                Isle.island.close(Isle.now())
                Isle.changed()
            }
            return false
        }
        getLocationOnScreen(at)
        val x = e.x + at[0]
        val y = e.y + at[1]
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
                it.set(((x - it.rect.left) / it.rect.width()).coerceIn(0f, 1f))
                invalidate()
                return true
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> dragging?.let {
                it.set(((x - it.rect.left) / it.rect.width()).coerceIn(0f, 1f))
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
