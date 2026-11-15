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
import android.text.TextUtils
import android.text.format.DateFormat
import android.util.Size
import android.view.GestureDetector
import android.view.MotionEvent
import android.view.View
import android.view.accessibility.AccessibilityEvent
import islet.island.Box
import islet.island.Format
import islet.island.Geometry
import islet.island.Live
import islet.island.Lists
import islet.island.Pop
import islet.island.Scene
import islet.island.Spring
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
    private val fade = Spring(1.0, stiffness = 500.0, damping = 1.0)
    private var lastFrame = 0L

    /** Buttons drawn this frame, in view pixels. */
    private class Hit(val rect: RectF, val action: () -> Unit)

    private class Slider(val rect: RectF, val set: (Float) -> Unit)

    private val hits = ArrayList<Hit>()
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

    private val white = Color.WHITE
    private val grey = 0xff9a9aa0.toInt()
    private val dim = 0xff2c2c2e.toInt()
    private val green = 0xff32d74b.toInt()
    private val red = 0xffff453a.toInt()
    private val orange = 0xffff9f0a.toInt()
    private val blue = 0xff0a84ff.toInt()
    private val purple = 0xff7d7aff.toInt()

    init {
        isClickable = true
        contentDescription = "Islet"
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_YES
    }

    private fun typeface(bold: Boolean): Typeface {
        val family = when (Isle.prefs.font) {
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
        val p = Isle.prefs
        val next = when {
            !p.islandOn -> Scene.Hidden
            p.hideInLandscape && window.landscape -> Scene.Hidden
            else -> Isle.island.scene(Isle.now())
        }
        if (sameShape(next, scene)) {
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

    private fun currentBox(): Box? {
        val f = from ?: return to
        val t = to ?: return f
        return f.lerp(t, morph.value)
    }

    private fun announce(s: Scene) {
        val words = when (s) {
            is Scene.Popped -> when (val p = s.pop) {
                is Pop.Note -> "${p.app}: ${p.title}. ${p.text}"
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
        val s = scene
        val fast = (s is Scene.Compact && s.live is Live.Music && (s.live as Live.Music).playing && Isle.prefs.bars) ||
            (s is Scene.Expanded && s.live is Live.Music && (s.live as Live.Music).playing) ||
            (s is Scene.Expanded && s.live is Live.Stopwatch && (s.live as Live.Stopwatch).running)
        val ticking = fast || s is Scene.Compact || s is Scene.Expanded
        if (ticking) postDelayed(tick, if (fast) 33L else 1000L)
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
        c.translate(-window.originX.toFloat(), -window.originY.toFloat())
        val l = (box.x * dp).toFloat()
        val t = (box.y * dp).toFloat()
        val r = (box.right * dp).toFloat()
        val b = (box.bottom * dp).toFloat()
        val radius = (box.radius * dp).toFloat()
        // The pill.
        fill.color = Color.BLACK
        rect.set(l, t, r, b)
        c.drawRoundRect(rect, radius, radius, fill)
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
        c.saveLayerAlpha(rect, alpha)
        if (morph.value > 0.35 || morph.settled) drawScene(c, s, RectF(l, t, r, b))
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
        val shownText = TextUtils.ellipsize(s, text, max, TextUtils.TruncateAt.END).toString()
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

    /** Moving bars beside a song. The motion is drawn, not measured from the sound. */
    private fun bars(c: Canvas, right: Float, cy: Float, hDp: Float, color: Int, playing: Boolean) {
        val n = 4
        val w = 3f * dp
        val gap = 2.5f * dp
        val t = SystemClock.uptimeMillis() / 1000.0
        fill.color = color
        for (i in 0 until n) {
            val phase = t * (5.0 + i * 1.7) + i * 1.3
            val f = if (playing && Isle.prefs.bars) 0.3 + 0.7 * (0.5 + 0.5 * sin(phase)) * (0.6 + 0.4 * sin(phase * 0.37)) else 0.3
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

    private fun timeText(ms: Long, zone: TimeZone? = null): String {
        val f = DateFormat.getTimeFormat(context)
        if (zone != null) f.timeZone = zone
        return f.format(Date(ms))
    }

    // ---- Idle --------------------------------------------------------------------------

    private fun drawIdle(c: Canvas, box: RectF) {
        val cy = box.centerY()
        val p = Isle.prefs
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
                val right = if (live.since > 0) Format.elapsed(now - live.since) else live.name
                write(c, right, rx + 8 * dp, cy + 5 * dp, 14f, green, bold = true, max = wing, align = Paint.Align.RIGHT)
            }
            is Live.Navigation -> {
                val extra = Isle.extras[live.id.removePrefix("note:")]
                if (!drawable(c, extra?.icon, lx, cy, 20f)) icon(c, Icon.Arrow, lx, cy, 17f, green)
                write(c, live.title, rx + 8 * dp, cy + 5 * dp, 14f, white, bold = true, max = wing, align = Paint.Align.RIGHT)
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
                val ix = box.left + pad + 20 * dp
                val extra = Isle.extras[p.key]
                if (!drawable(c, extra?.let { Isle.appIcon(it.pkg) }, ix, cy, 40f)) icon(c, Icon.Bell, ix, cy, 24f, white)
                val x = ix + 30 * dp
                val max = box.right - pad - x
                write(c, p.title.ifEmpty { p.app }, x, cy - 4 * dp, 15f, white, bold = true, max = max - 40 * dp)
                write(c, p.text.ifEmpty { p.app }, x, cy + 16 * dp, 14f, grey, max = max)
                write(c, "now", box.right - pad, cy - 4 * dp, 12f, grey, align = Paint.Align.RIGHT)
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
                        Pop.RingerMode.Silent -> Quad(Icon.BellOff, red, "Silent", "")
                        Pop.RingerMode.Vibrate -> Quad(Icon.Vibrate, orange, "Vibrate", "")
                        Pop.RingerMode.Ring -> Quad(Icon.Bell, white, "Ring", "")
                    }
                    is Pop.Airplane -> Quad(Icon.Plane, orange, "Airplane mode", if (p.on) "On" else "Off")
                    is Pop.DoNotDisturb -> Quad(Icon.Moon, purple, "Do Not Disturb", if (p.on) "On" else "Off")
                    is Pop.Wifi -> Quad(Icon.Wifi, blue, p.name.ifEmpty { "Wi-Fi" }, "Connected")
                    is Pop.Info -> Quad(null, white, p.text, "")
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
                        write(c, right, rx + 6 * dp, cy + 5 * dp, 14f, grey, bold = true, align = Paint.Align.RIGHT)
                    }
                }
            }
        }
    }

    private data class Quad(val icon: Icon?, val color: Int, val left: String, val right: String)

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
                    live.incoming -> "${live.app} · incoming"
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
                if (!drawable(c, extra?.icon, l + 26 * dp, cy, 52f)) icon(c, Icon.Arrow, l + 26 * dp, cy, 44f, green)
                write(c, live.title, l + 66 * dp, cy - 6 * dp, 22f, white, bold = true, max = r - l - 66 * dp)
                write(c, live.text, l + 66 * dp, cy + 16 * dp, 14f, grey, max = r - l - 66 * dp)
                write(c, live.app, l + 66 * dp, cy + 34 * dp, 11f, grey)
                hits.add(Hit(RectF(box)) { Isle.open(live.id) })
            }
            is Live.Timer -> {
                write(c, if (live.onBreak) "Break" else live.label, l, t + 14 * dp, 13f, grey)
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
        Isle.lyrics?.lineAt(position, Isle.prefs.lyricsOffset)?.let {
            write(c, it, box.centerX(), y + 20 * dp, 13f, accent, bold = true, max = r - l, align = Paint.Align.CENTER)
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
    }

    private fun drawClockCard(c: Canvas, box: RectF) {
        val pad = 20 * dp
        val l = box.left + pad
        val r = box.right - pad
        val t = box.top + pad
        val now = Isle.now()
        write(c, timeText(now), l, t + 38 * dp, 40f, white, bold = true, max = (r - l) / 2)
        write(c, DateFormat.format("EEEE d MMMM", now).toString(), l, t + 58 * dp, 13f, grey)
        Isle.weather?.let { w ->
            val p = Isle.prefs
            icon(c, Icons.sky(w.sky, w.day), r - 76 * dp, t + 22 * dp, 26f, if (w.day) 0xffffd60a.toInt() else white)
            write(c, Format.temperature(w.celsius, p.fahrenheit), r, t + 32 * dp, 26f, white, bold = true, align = Paint.Align.RIGHT)
            val hiLo = if (w.highC != null && w.lowC != null) {
                " · H ${Format.temperature(w.highC!!, p.fahrenheit)} L ${Format.temperature(w.lowC!!, p.fahrenheit)}"
            } else {
                ""
            }
            write(c, w.words + hiLo, r, t + 54 * dp, 12f, grey, max = (r - l) / 2, align = Paint.Align.RIGHT)
        }
        var y = t + 84 * dp
        val clocks = Isle.prefs.worldClocks.mapNotNull { id ->
            val z = TimeZone.getTimeZone(id)
            if (z.id == "GMT" && id != "GMT") null else "${id.substringAfterLast('/').replace('_', ' ')} ${timeText(now, z)}"
        }
        if (clocks.isNotEmpty()) {
            write(c, clocks.joinToString("   "), l, y, 13f, white, max = r - l)
            y += 20 * dp
        }
        val info = listOfNotNull(
            islet.island.Meetings.next(Isle.events, now)?.let { "${it.title} ${Format.until(it.begin - now)}" },
            Isle.nextAlarm?.takeIf { it - now < 24 * 3_600_000L }?.let { "Alarm ${timeText(it)}" },
            Lists.nextTodo(Isle.prefs.todos)?.let { "To do: $it" },
        )
        if (info.isNotEmpty()) write(c, info.joinToString(" · "), l, y, 13f, grey, max = r - l)
        val cy = box.bottom - pad - 14 * dp
        var x = l
        for (minutes in listOf(1, 5, 10, 25)) {
            x += chip(c, "${minutes}m", x, cy, dim) { Isle.startFocus(minutes.toDouble()); Isle.island.open("timer"); Isle.changed() } + 8 * dp
        }
        if (x + 120 * dp < r) {
            x += chip(c, "Focus", x, cy, 0xff5a3a00.toInt(), orange) { Isle.startPomodoro(); Isle.island.open("timer"); Isle.changed() } + 8 * dp
        }
        if (x + 50 * dp < r) {
            button(c, r - 15 * dp, cy, 30f, dim, Icon.Stopwatch) {
                Isle.stopwatch.start(Isle.now())
                Isle.syncTimers()
                Isle.island.open("stopwatch")
                Isle.changed()
            }
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
        val vibrate = Controls.ringerMode != android.media.AudioManager.RINGER_MODE_NORMAL
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
        slider(Icon.Sound, Controls.volume, true, { Controls.volume = it }) {}
        slider(Icon.Brightness, Controls.brightness, Controls.canSetBrightness, { Controls.brightness = it }) { Controls.askForBrightness() }
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

            override fun onSingleTapUp(e: MotionEvent): Boolean {
                val x = e.x + window.originX
                val y = e.y + window.originY
                val near = 6 * dp
                hits.lastOrNull { RectF(it.rect).apply { inset(-near, -near) }.contains(x, y) }?.let {
                    it.action()
                    Isle.changed()
                    return true
                }
                tapBackground()
                return true
            }

            override fun onLongPress(e: MotionEvent) {
                if (dragging != null) return
                performHapticFeedback(android.view.HapticFeedbackConstants.LONG_PRESS)
                Isle.island.longPress()
                Isle.changed()
            }

            override fun onFling(e1: MotionEvent?, e2: MotionEvent, vx: Float, vy: Float): Boolean {
                if (dragging != null) return false
                if (abs(vy) > abs(vx) && vy < 0) {
                    Isle.island.close(Isle.now())
                } else if (abs(vx) > abs(vy) && !Isle.island.isOpen) {
                    Isle.island.cycle()
                } else if (abs(vy) > abs(vx) && vy > 0 && !Isle.island.isOpen) {
                    Isle.island.tap(Isle.now())
                }
                Isle.changed()
                return true
            }
        },
    )

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
        val x = e.x + window.originX
        val y = e.y + window.originY
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> dragging = sliders.firstOrNull { it.rect.contains(x, y) }
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
        return gestures.onTouchEvent(e) || super.onTouchEvent(e)
    }

    override fun performClick(): Boolean {
        super.performClick()
        tapBackground()
        return true
    }
}
