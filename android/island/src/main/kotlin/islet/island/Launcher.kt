package islet.island

/** Something the quick launcher can open: an installed app, or one of your own links. */
data class Launchable(val name: String, val target: String, val isLink: Boolean = false)

/** A shortcut of your own: a name and a link (https://, tel:, mailto:, or an app's own link). */
data class Link(val name: String, val url: String)

object Launcher {
    /**
     * The launchables matching what was typed, best first: a name that
     * starts with it, then a word in the name that does, then the letters
     * in order ("yt" finds "YouTube"). Links come before apps on a tie.
     */
    fun search(query: String, all: List<Launchable>, limit: Int = 12): List<Launchable> {
        val q = query.trim().lowercase()
        if (q.isEmpty()) return all.sortedWith(compareBy({ !it.isLink }, { it.name.lowercase() })).take(limit)
        return all.mapNotNull { a -> score(q, a.name.lowercase())?.let { a to it } }
            .sortedWith(compareBy({ it.second }, { !it.first.isLink }, { it.first.name.length }, { it.first.name.lowercase() }))
            .map { it.first }
            .take(limit)
    }

    private fun score(q: String, name: String): Int? = when {
        name.startsWith(q) -> 0
        name.split(' ', '-', '.', '_').any { it.startsWith(q) } -> 1
        q in name -> 2
        inOrder(q, name) -> 3
        else -> null
    }

    private fun inOrder(q: String, name: String): Boolean {
        var i = 0
        for (ch in name) if (i < q.length && ch == q[i]) i++
        return i == q.length
    }

    /** Only links that open something on the phone: no javascript: or file: links. */
    fun safeLink(url: String): Boolean {
        val u = url.trim()
        val scheme = u.substringBefore(':', "").lowercase()
        return scheme.isNotEmpty() && scheme.all { it.isLetterOrDigit() || it in "+-." } &&
            scheme !in setOf("javascript", "file", "content", "data", "intent") && u.length > scheme.length + 1
    }

    /** "example.com" → "https://example.com". */
    fun tidyLink(url: String): String {
        val u = url.trim()
        return if (':' !in u && '.' in u && ' ' !in u) "https://$u" else u
    }

    fun encodeLinks(links: List<Link>): String =
        links.joinToString("\n") { Lists.clean(it.name).replace('|', '/') + "|" + Lists.clean(it.url) }

    fun decodeLinks(s: String?): List<Link> = s.orEmpty().lines().mapNotNull { line ->
        val name = line.substringBefore('|', "").trim()
        val url = line.substringAfter('|', "").trim()
        if (name.isEmpty() || !safeLink(url)) null else Link(name, url)
    }
}

/** Live bars from the sound that's playing: four levels, 0..1, from an FFT. */
object Bars {
    /**
     * `fft` as Android's Visualizer gives it: pairs of (real, imaginary)
     * bytes, bin 0 first. Groups the bins into four bands (bass to treble,
     * spaced out like hearing), on a dB scale.
     */
    fun levels(fft: ByteArray): FloatArray {
        val bins = fft.size / 2
        val out = FloatArray(4)
        if (bins < 8) return out
        val edges = intArrayOf(1, maxOf(2, bins / 32), maxOf(3, bins / 10), maxOf(4, bins / 4), bins)
        for (b in 0 until 4) {
            var peak = 0.0
            for (k in edges[b] until maxOf(edges[b] + 1, edges[b + 1])) {
                val re = fft.getOrElse(2 * k) { 0 }.toDouble()
                val im = fft.getOrElse(2 * k + 1) { 0 }.toDouble()
                peak = maxOf(peak, Math.hypot(re, im))
            }
            // 0 dB at full scale (about 181 for a byte pair), silence below -40 dB.
            val db = if (peak <= 0) -60.0 else 20 * Math.log10(peak / 181.0)
            out[b] = ((db + 40) / 40).toFloat().coerceIn(0f, 1f)
        }
        return out
    }

    /** Ease toward the new levels so the bars don't flicker: quick up, slower down. */
    fun smooth(old: FloatArray, new: FloatArray): FloatArray =
        FloatArray(new.size) { i ->
            val o = old.getOrElse(i) { 0f }
            if (new[i] > o) o + (new[i] - o) * 0.6f else o + (new[i] - o) * 0.25f
        }
}
