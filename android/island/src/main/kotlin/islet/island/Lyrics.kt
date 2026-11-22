package islet.island

import java.net.URLEncoder

/** Synced lyrics in LRC form: `[01:02.50] a line`. */
class Lyrics(val lines: List<Pair<Long, String>>) {
    /** The line being sung at `ms` (with `offset` added), or null before the first. */
    fun lineAt(ms: Long, offset: Long = 0): String? {
        val t = ms + offset
        var found: String? = null
        for ((at, text) in lines) {
            if (at > t) break
            found = text
        }
        return found?.takeIf { it.isNotBlank() }
    }

    /** The line after the one being sung at `ms` (what's coming), or the first before it starts. */
    fun nextAfter(ms: Long, offset: Long = 0): String? {
        val t = ms + offset
        return lines.firstOrNull { (at, text) -> at > t && text.isNotBlank() }?.second
    }

    companion object {
        /**
         * Show each line this much early (ms), as the computer does: a line
         * that appears exactly on its beat reads as late.
         */
        const val LEAD = 150L

        private val TAG = Regex("""\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?]""")

        fun parse(lrc: String): Lyrics? {
            val out = ArrayList<Pair<Long, String>>()
            for (raw in lrc.lineSequence()) {
                val tags = TAG.findAll(raw).toList()
                if (tags.isEmpty()) continue
                val text = raw.substring(tags.last().range.last + 1).trim()
                for (t in tags) {
                    val (m, s, frac) = t.destructured
                    val f = when (frac.length) {
                        0 -> 0L
                        1 -> frac.toLong() * 100
                        2 -> frac.toLong() * 10
                        else -> frac.take(3).toLong()
                    }
                    out.add((m.toLong() * 60 + s.toLong()) * 1000 + f to text)
                }
            }
            if (out.isEmpty()) return null
            out.sortBy { it.first }
            return Lyrics(out)
        }

        /** LRCLIB's lookup for one song (a free, keyless service). */
        fun lrclibUrl(title: String, artist: String, durationMs: Long): String {
            fun e(s: String) = URLEncoder.encode(s, "UTF-8")
            val d = if (durationMs > 0) "&duration=${durationMs / 1000}" else ""
            return "https://lrclib.net/api/get?track_name=${e(title)}&artist_name=${e(artist)}$d"
        }

        /** The synced lyrics in LRCLIB's answer, if it has any. */
        fun fromLrclib(json: String): Lyrics? {
            val v = Json.parse(json) ?: return null
            if (v.at("instrumental") == true) return null
            return (v.at("syncedLyrics") as? String)?.let { parse(it) }
        }
    }
}
