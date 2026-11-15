package islet.island

import kotlin.math.max
import kotlin.math.min

/** Colours as 0xRRGGBB ints (alpha ignored). */
object Colors {
    private fun r(c: Int) = (c shr 16) and 0xff

    private fun g(c: Int) = (c shr 8) and 0xff

    private fun b(c: Int) = c and 0xff

    /** 0..1: how colourful. */
    fun saturation(c: Int): Double {
        val hi = max(r(c), max(g(c), b(c)))
        val lo = min(r(c), min(g(c), b(c)))
        return if (hi == 0) 0.0 else (hi - lo).toDouble() / hi
    }

    fun brightness(c: Int): Int = max(r(c), max(g(c), b(c)))

    /**
     * The main colour of a picture (album art), for the island's border.
     * Colourful pixels count most; black, white and greys only when the
     * picture has nothing else. Null for an empty picture.
     */
    fun dominant(pixels: IntArray): Int? {
        if (pixels.isEmpty()) return null
        val weight = HashMap<Int, Double>()
        val sums = HashMap<Int, LongArray>()
        var colourful = 0
        for (p in pixels) {
            if ((p ushr 24) < 128 && (p ushr 24) != 0) continue
            val c = p and 0xffffff
            val sat = saturation(c)
            val bright = brightness(c)
            val vivid = sat > 0.25 && bright in 50..250
            if (vivid) colourful++
            val bucket = ((r(c) shr 4) shl 8) or ((g(c) shr 4) shl 4) or (b(c) shr 4)
            val w = if (vivid) 1.0 + 3 * sat else 0.05
            weight[bucket] = (weight[bucket] ?: 0.0) + w
            val s = sums.getOrPut(bucket) { LongArray(4) }
            s[0] += r(c).toLong()
            s[1] += g(c).toLong()
            s[2] += b(c).toLong()
            s[3]++
        }
        val best = weight.maxByOrNull { it.value }?.key ?: return null
        val s = sums.getValue(best)
        val c = ((s[0] / s[3]).toInt() shl 16) or ((s[1] / s[3]).toInt() shl 8) or (s[2] / s[3]).toInt()
        return if (colourful == 0 && saturation(c) < 0.25) null else c
    }

    /** Lift a dark colour so it still shows as a border on black. */
    fun forBorder(c: Int): Int {
        val hi = brightness(c)
        if (hi >= 150) return c
        if (hi == 0) return 0x9a9a9a
        val k = 150.0 / hi
        fun up(v: Int) = min(255, (v * k).toInt())
        return (up(r(c)) shl 16) or (up(g(c)) shl 8) or up(b(c))
    }
}
