package islet.island

import kotlin.math.roundToInt

object Format {
    /** 4:05, 1:02:03. */
    fun clock(ms: Long): String {
        val s = (ms.coerceAtLeast(0) + 999) / 1000
        val h = s / 3600
        val m = (s % 3600) / 60
        val sec = s % 60
        return if (h > 0) "%d:%02d:%02d".format(h, m, sec) else "%d:%02d".format(m, sec)
    }

    /** A stopwatch: 1:04.37, 1:02:03.4. */
    fun stopwatch(ms: Long): String {
        val t = ms.coerceAtLeast(0)
        val h = t / 3_600_000
        val m = (t % 3_600_000) / 60_000
        val s = (t % 60_000) / 1000
        val cs = (t % 1000) / 10
        return if (h > 0) "%d:%02d:%02d.%d".format(h, m, s, cs / 10) else "%d:%02d.%02d".format(m, s, cs)
    }

    /** Elapsed time on a song or a call, counting up: 0:07, 12:30. */
    fun elapsed(ms: Long): String {
        val s = ms.coerceAtLeast(0) / 1000
        val h = s / 3600
        return if (h > 0) "%d:%02d:%02d".format(h, (s % 3600) / 60, s % 60) else "%d:%02d".format(s / 60, s % 60)
    }

    /** "in 12 min", "in 1 h 5 min", "now", "5 min ago". */
    fun until(ms: Long): String {
        val min = ((ms + if (ms >= 0) 59_999 else 0) / 60_000)
        return when {
            ms <= 0 && ms > -60_000 -> "now"
            min < 0 -> "${-min} min ago"
            min < 60 -> "in $min min"
            min % 60 == 0L -> "in ${min / 60} h"
            else -> "in ${min / 60} h ${min % 60} min"
        }
    }

    /** 21°, in Celsius or Fahrenheit. */
    fun temperature(celsius: Double, fahrenheit: Boolean): String {
        val v = if (fahrenheit) celsius * 9 / 5 + 32 else celsius
        return "${v.roundToInt()}°"
    }

    /** "12.4", "940" (Mbps). */
    fun mbps(v: Double): String = if (v >= 100) v.roundToInt().toString() else "%.1f".format(java.util.Locale.ROOT, v)

    /** Shorten to `max` characters with an ellipsis. */
    fun cut(s: String, max: Int): String = if (s.length <= max) s else s.take(max - 1).trimEnd() + "…"
}
