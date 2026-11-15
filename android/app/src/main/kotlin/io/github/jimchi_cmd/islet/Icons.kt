package io.github.jimchi_cmd.islet

import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import islet.island.Sky

/**
 * The island's own icons, drawn with a few strokes on a 24-unit grid.
 * Simple shapes of our own, not copied from any icon set.
 */
enum class Icon {
    Play, Pause, Next, Previous, Phone, HangUp, Bell, BellOff, Vibrate, Plane, Moon, Wifi, Bluetooth, Bolt,
    Timer, Stopwatch, Arrow, Download, Camera, Mic, Flashlight, Sun, Cloud, Rain, Snow, Storm, Fog, Lock,
    Screenshot, Note, Check, Plus, Minus, Calendar, Video, Close, Gear, Sound, Brightness, Music, Flag, Reset, Clipboard, Gauge, Toggles,
    Book, Search, Battery, Link, Quote,
}

object Icons {
    private val path = Path()
    private val oval = RectF()
    private val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeCap = Paint.Cap.ROUND
        strokeJoin = Paint.Join.ROUND
    }
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }

    fun sky(s: Sky, day: Boolean): Icon = when (s) {
        Sky.Clear -> if (day) Icon.Sun else Icon.Moon
        Sky.PartlyCloudy, Sky.Cloudy -> Icon.Cloud
        Sky.Fog -> Icon.Fog
        Sky.Drizzle, Sky.Rain -> Icon.Rain
        Sky.Snow -> Icon.Snow
        Sky.Storm -> Icon.Storm
    }

    /** Draw `icon` filling a square of side `size` centred on (cx, cy). */
    fun draw(c: Canvas, icon: Icon, cx: Float, cy: Float, size: Float, color: Int) {
        val k = size / 24f
        stroke.color = color
        fill.color = color
        c.save()
        c.translate(cx - 12 * k, cy - 12 * k)
        c.scale(k, k)
        stroke.strokeWidth = 2f
        fun p(vararg pts: Float, close: Boolean = false, paint: Paint = stroke) {
            path.reset()
            path.moveTo(pts[0], pts[1])
            var i = 2
            while (i < pts.size) {
                path.lineTo(pts[i], pts[i + 1])
                i += 2
            }
            if (close) path.close()
            c.drawPath(path, paint)
        }
        fun circle(x: Float, y: Float, r: Float, paint: Paint = stroke) = c.drawCircle(x, y, r, paint)
        fun arc(l: Float, t: Float, r: Float, b: Float, start: Float, sweep: Float) {
            oval.set(l, t, r, b)
            c.drawArc(oval, start, sweep, false, stroke)
        }
        fun cloud(y: Float = 0f) {
            path.reset()
            path.moveTo(7f, 18f + y)
            path.cubicTo(3.5f, 18f + y, 3f, 12.5f + y, 7f, 12f + y)
            path.cubicTo(7.5f, 7.5f + y, 14f, 6.5f + y, 15.5f, 10.5f + y)
            path.cubicTo(19.5f, 9.5f + y, 22f, 13.5f + y, 19.5f, 16.5f + y)
            path.cubicTo(19f, 17.5f + y, 18f, 18f + y, 17f, 18f + y)
            path.close()
            c.drawPath(path, stroke)
        }
        when (icon) {
            Icon.Play -> p(7f, 4.5f, 19.5f, 12f, 7f, 19.5f, close = true, paint = fill)
            Icon.Pause -> {
                c.drawRoundRect(6f, 4.5f, 10f, 19.5f, 1.2f, 1.2f, fill)
                c.drawRoundRect(14f, 4.5f, 18f, 19.5f, 1.2f, 1.2f, fill)
            }
            Icon.Next -> {
                p(3f, 6f, 11.5f, 12f, 3f, 18f, close = true, paint = fill)
                p(11.5f, 6f, 20f, 12f, 11.5f, 18f, close = true, paint = fill)
            }
            Icon.Previous -> {
                p(21f, 6f, 12.5f, 12f, 21f, 18f, close = true, paint = fill)
                p(12.5f, 6f, 4f, 12f, 12.5f, 18f, close = true, paint = fill)
            }
            Icon.Phone -> {
                path.reset()
                path.moveTo(6.5f, 3.5f)
                path.lineTo(9.5f, 3.5f)
                path.lineTo(11f, 8f)
                path.lineTo(8.8f, 9.6f)
                path.cubicTo(9.8f, 12f, 12f, 14.2f, 14.4f, 15.2f)
                path.lineTo(16f, 13f)
                path.lineTo(20.5f, 14.5f)
                path.lineTo(20.5f, 17.5f)
                path.cubicTo(20.5f, 19.5f, 18.5f, 20.8f, 16.5f, 20.3f)
                path.cubicTo(10f, 18.6f, 5.4f, 14f, 3.7f, 7.5f)
                path.cubicTo(3.2f, 5.5f, 4.5f, 3.5f, 6.5f, 3.5f)
                path.close()
                c.drawPath(path, fill)
            }
            Icon.HangUp -> {
                c.save()
                c.rotate(135f, 12f, 12f)
                draw(c, Icon.Phone, 12f, 12f, 24f, color)
                c.restore()
            }
            Icon.Bell, Icon.BellOff -> {
                path.reset()
                path.moveTo(5f, 17f)
                path.lineTo(19f, 17f)
                path.lineTo(17.5f, 14.5f)
                path.lineTo(17.5f, 10f)
                path.cubicTo(17.5f, 6.8f, 15f, 4.5f, 12f, 4.5f)
                path.cubicTo(9f, 4.5f, 6.5f, 6.8f, 6.5f, 10f)
                path.lineTo(6.5f, 14.5f)
                path.close()
                c.drawPath(path, stroke)
                arc(9.5f, 17.5f, 14.5f, 22f, 0f, 180f)
                if (icon == Icon.BellOff) p(4f, 3f, 20f, 21f)
            }
            Icon.Vibrate -> {
                c.drawRoundRect(8f, 3.5f, 16f, 20.5f, 2f, 2f, stroke)
                p(4.5f, 8f, 3f, 10f, 4.5f, 12f, 3f, 14f, 4.5f, 16f)
                p(19.5f, 8f, 21f, 10f, 19.5f, 12f, 21f, 14f, 19.5f, 16f)
            }
            Icon.Plane -> p(
                12f, 2.5f, 13.5f, 4f, 13.5f, 9.5f, 21f, 14f, 21f, 15.8f, 13.5f, 13.5f, 13.5f, 18.5f, 16f, 20.3f, 16f, 21.5f,
                12f, 20.5f, 8f, 21.5f, 8f, 20.3f, 10.5f, 18.5f, 10.5f, 13.5f, 3f, 15.8f, 3f, 14f, 10.5f, 9.5f, 10.5f, 4f,
                close = true, paint = fill,
            )
            Icon.Moon -> {
                path.reset()
                path.moveTo(15f, 3.5f)
                path.cubicTo(9f, 3.8f, 5f, 8f, 5.5f, 13.5f)
                path.cubicTo(6f, 18f, 10f, 21f, 14.5f, 20.5f)
                path.cubicTo(17.2f, 20.2f, 19.3f, 18.8f, 20.5f, 16.8f)
                path.cubicTo(14.5f, 17.5f, 10.5f, 10f, 15f, 3.5f)
                path.close()
                c.drawPath(path, fill)
            }
            Icon.Wifi -> {
                arc(2f, 5f, 22f, 25f, 225f, 90f)
                arc(5.5f, 8.5f, 18.5f, 21.5f, 225f, 90f)
                arc(9f, 12f, 15f, 18f, 225f, 90f)
                circle(12f, 19f, 1.6f, fill)
            }
            Icon.Bluetooth -> p(7f, 7.5f, 17f, 16.5f, 12f, 21f, 12f, 3f, 17f, 7.5f, 7f, 16.5f)
            Icon.Bolt -> p(13.5f, 2.5f, 5.5f, 13.5f, 11.5f, 13.5f, 10.5f, 21.5f, 18.5f, 10.5f, 12.5f, 10.5f, close = true, paint = fill)
            Icon.Timer -> {
                circle(12f, 13.5f, 7.5f)
                p(12f, 13.5f, 12f, 9.5f)
                p(10f, 3f, 14f, 3f)
                p(18.5f, 6.5f, 19.8f, 5.2f)
            }
            Icon.Stopwatch -> {
                circle(12f, 13.5f, 7.5f)
                p(12f, 13.5f, 15f, 10.5f)
                p(12f, 3f, 12f, 6f)
                p(10f, 3f, 14f, 3f)
            }
            Icon.Arrow -> {
                p(12f, 21f, 12f, 10f, 7f, 10f)
                p(9.5f, 7f, 6.5f, 10f, 9.5f, 13f)
                p(12f, 10f, 12f, 8f, 17f, 3.5f)
            }
            Icon.Download -> {
                p(12f, 3.5f, 12f, 15f)
                p(7f, 10.5f, 12f, 15.5f, 17f, 10.5f)
                p(5f, 20f, 19f, 20f)
            }
            Icon.Camera, Icon.Video -> {
                c.drawRoundRect(2.5f, 6.5f, 16f, 17.5f, 2.5f, 2.5f, stroke)
                p(16f, 10.5f, 21.5f, 7.5f, 21.5f, 16.5f, 16f, 13.5f, close = true)
            }
            Icon.Mic -> {
                c.drawRoundRect(9f, 3f, 15f, 14f, 3f, 3f, fill)
                arc(6f, 6.5f, 18f, 17.5f, 0f, 180f)
                p(12f, 17.5f, 12f, 21f)
            }
            Icon.Flashlight -> {
                p(7f, 3f, 17f, 3f, 17f, 7f, 14.5f, 11f, 14.5f, 21f, 9.5f, 21f, 9.5f, 11f, 7f, 7f, close = true)
                p(12f, 14f, 12f, 16f)
            }
            Icon.Sun, Icon.Brightness -> {
                circle(12f, 12f, 4f, if (icon == Icon.Sun) fill else stroke)
                for (i in 0 until 8) {
                    c.save()
                    c.rotate(i * 45f, 12f, 12f)
                    p(12f, 2.5f, 12f, 4.5f)
                    c.restore()
                }
            }
            Icon.Cloud -> cloud()
            Icon.Rain -> {
                cloud(-3f)
                p(8f, 18f, 7f, 21f)
                p(12f, 18f, 11f, 21f)
                p(16f, 18f, 15f, 21f)
            }
            Icon.Snow -> {
                cloud(-3f)
                circle(8f, 19.5f, 1f, fill)
                circle(12f, 21f, 1f, fill)
                circle(16f, 19.5f, 1f, fill)
            }
            Icon.Storm -> {
                cloud(-3f)
                p(12.5f, 15.5f, 10f, 19f, 13f, 19f, 11f, 22.5f)
            }
            Icon.Fog -> {
                p(4f, 9f, 20f, 9f)
                p(3f, 13f, 21f, 13f)
                p(5f, 17f, 19f, 17f)
            }
            Icon.Lock -> {
                c.drawRoundRect(5f, 10.5f, 19f, 20.5f, 2.5f, 2.5f, fill)
                arc(8f, 3.5f, 16f, 15f, 180f, 180f)
                p(8f, 9.25f, 8f, 10.5f)
                p(16f, 9.25f, 16f, 10.5f)
            }
            Icon.Screenshot -> {
                p(4f, 9f, 4f, 4f, 9f, 4f)
                p(15f, 4f, 20f, 4f, 20f, 9f)
                p(20f, 15f, 20f, 20f, 15f, 20f)
                p(9f, 20f, 4f, 20f, 4f, 15f)
            }
            Icon.Note -> {
                c.drawRoundRect(5f, 3f, 19f, 21f, 2.5f, 2.5f, stroke)
                p(8.5f, 8f, 15.5f, 8f)
                p(8.5f, 12f, 15.5f, 12f)
                p(8.5f, 16f, 12.5f, 16f)
            }
            Icon.Check -> p(5f, 12.5f, 10f, 17.5f, 19f, 6.5f)
            Icon.Plus -> {
                p(12f, 5f, 12f, 19f)
                p(5f, 12f, 19f, 12f)
            }
            Icon.Minus -> p(5f, 12f, 19f, 12f)
            Icon.Calendar -> {
                c.drawRoundRect(3.5f, 5f, 20.5f, 20.5f, 2.5f, 2.5f, stroke)
                p(3.5f, 9.5f, 20.5f, 9.5f)
                p(8f, 3f, 8f, 6.5f)
                p(16f, 3f, 16f, 6.5f)
            }
            Icon.Close -> {
                p(6f, 6f, 18f, 18f)
                p(18f, 6f, 6f, 18f)
            }
            Icon.Gear -> {
                circle(12f, 12f, 3f)
                circle(12f, 12f, 7.5f)
                for (i in 0 until 6) {
                    c.save()
                    c.rotate(i * 60f, 12f, 12f)
                    p(12f, 2.5f, 12f, 4.5f)
                    c.restore()
                }
            }
            Icon.Sound -> {
                p(3.5f, 9f, 7.5f, 9f, 12.5f, 4.5f, 12.5f, 19.5f, 7.5f, 15f, 3.5f, 15f, close = true, paint = fill)
                arc(10f, 7f, 20f, 17f, -50f, 100f)
            }
            Icon.Music -> {
                p(9f, 18f, 9f, 5f, 19f, 3f, 19f, 16f)
                circle(6.5f, 18f, 2.5f, fill)
                circle(16.5f, 16f, 2.5f, fill)
            }
            Icon.Flag -> {
                p(5f, 21f, 5f, 3.5f)
                p(5f, 4f, 18f, 4f, 15f, 8f, 18f, 12f, 5f, 12f, close = true, paint = fill)
            }
            Icon.Clipboard -> {
                c.drawRoundRect(5.5f, 4.5f, 18.5f, 21f, 2.5f, 2.5f, stroke)
                c.drawRoundRect(9f, 2.5f, 15f, 6.5f, 1.5f, 1.5f, fill)
                p(9f, 11f, 15f, 11f)
                p(9f, 15f, 13.5f, 15f)
            }
            Icon.Gauge -> {
                arc(3.5f, 4.5f, 20.5f, 21.5f, 150f, 240f)
                p(12f, 13f, 15.5f, 8.5f)
                circle(12f, 13f, 1.6f, fill)
            }
            Icon.Toggles -> {
                c.drawRoundRect(3f, 4f, 21f, 11f, 3.5f, 3.5f, stroke)
                circle(17.5f, 7.5f, 2f, fill)
                c.drawRoundRect(3f, 13f, 21f, 20f, 3.5f, 3.5f, stroke)
                circle(6.5f, 16.5f, 2f, fill)
            }
            Icon.Reset -> {
                arc(4f, 4f, 20f, 20f, 200f, 290f)
                p(3.5f, 4.5f, 4.5f, 9.5f, 9.5f, 8.5f)
            }
            Icon.Book -> {
                // An open book: two pages meeting at the spine.
                p(12f, 6.5f, 12f, 20f)
                p(12f, 6.5f, 8.5f, 5f, 3.5f, 5f, 3.5f, 18.5f, 8.5f, 18.5f, 12f, 20f)
                p(12f, 6.5f, 15.5f, 5f, 20.5f, 5f, 20.5f, 18.5f, 15.5f, 18.5f, 12f, 20f)
            }
            Icon.Search -> {
                circle(10.5f, 10.5f, 6f, stroke)
                p(15f, 15f, 20f, 20f)
            }
            Icon.Battery -> {
                c.drawRoundRect(3f, 7f, 18.5f, 17f, 2.5f, 2.5f, stroke)
                c.drawRoundRect(5.5f, 9.5f, 12.5f, 14.5f, 1f, 1f, fill)
                p(21f, 10.5f, 21f, 13.5f)
            }
            Icon.Quote -> {
                // A speech bubble with lines: lyrics.
                c.drawRoundRect(3f, 4f, 21f, 16.5f, 4f, 4f, stroke)
                p(8f, 16.5f, 7f, 21f, 12.5f, 16.5f)
                p(7.5f, 9f, 16.5f, 9f)
                p(7.5f, 12f, 13f, 12f)
            }
            Icon.Link -> {
                c.drawRoundRect(3f, 9f, 13f, 15f, 3f, 3f, stroke)
                c.drawRoundRect(11f, 9f, 21f, 15f, 3f, 3f, stroke)
            }
        }
        c.restore()
    }
}
