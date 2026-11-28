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

        private const val API = "https://lrclib.net/api"

        private fun e(s: String) = URLEncoder.encode(s, "UTF-8")

        /** LRCLIB's exact lookup for one song (a free, keyless service). */
        fun lrclibUrl(title: String, artist: String, durationMs: Long): String {
            val d = if (durationMs > 0) "&duration=${durationMs / 1000}" else ""
            return "$API/get?track_name=${e(title)}&artist_name=${e(artist)}$d"
        }

        /**
         * Where to look, best first: the exact song; then searches with the
         * title and first artist tidied, "Artist - Song" read from the title
         * (as YouTube videos are named, under the channel's name), and a
         * free-text search. A search answer is a list (see [fromSearch]).
         */
        fun lrclibLookups(title: String, artist: String, durationMs: Long): List<String> {
            val t = cleanTitle(title)
            val a = cleanArtist(artist)
            val out = mutableListOf(lrclibUrl(title, artist, durationMs), "$API/search?track_name=${e(t)}&artist_name=${e(a)}")
            val dash = t.indexOf(" - ")
            if (dash > 0) {
                val song = cleanTitle(t.substring(dash + 3))
                if (song.isNotEmpty()) out.add("$API/search?track_name=${e(song)}&artist_name=${e(cleanArtist(t.substring(0, dash)))}")
            }
            out.add("$API/search?q=${e("$a $t".replace(" - ", " "))}")
            return out.distinct()
        }

        // Whole words only (and a few word starts: "Remastered", "Lyrics"),
        // so "(Birthday)", "Oliver" or "(Without You)" stay.
        private val STARTS = listOf("remaster", "featuring", "version", "official", "lyric", "visuali", "explicit", "stereo")
        private val WORDS = setOf("feat", "ft", "live", "edit", "edited", "mono", "audio", "video", "hd", "4k", "hq", "mv", "with", "mix")

        private fun noisy(s: String) = s.lowercase().split(Regex("[^\\p{L}\\p{N}]+")).any { w ->
            w.isNotEmpty() && (STARTS.any { w.startsWith(it) } || w in WORDS)
        }

        /** The title without what players and video sites add: "(Official Video)", "[Lyrics]", " - Remastered 2011". */
        fun cleanTitle(title: String): String {
            var t = title.trim()
            val bar = t.indexOf(" | ")
            if (bar > 0) t = t.substring(0, bar).trim()
            for ((open, close) in listOf('(' to ')', '[' to ']')) {
                while (true) {
                    val i = t.indexOf(open)
                    if (i < 0) break
                    val j = t.indexOf(close, i)
                    if (j < 0 || !noisy(t.substring(i, j))) break
                    t = t.substring(0, i) + t.substring(j + 1)
                }
            }
            val dash = t.trimEnd().lastIndexOf(" - ")
            if (dash > 0 && noisy(t.substring(dash + 3))) t = t.substring(0, dash)
            val out = t.split(Regex("\\s+")).filter { it.isNotEmpty() }.joinToString(" ")
            return out.ifEmpty { title.trim() }
        }

        /** The first artist, without "- Topic", "VEVO", "feat. …", ", B & C". */
        fun cleanArtist(artist: String): String {
            var a = artist.trim()
            for (tail in listOf(" - Topic", "VEVO", "Vevo", " - Official")) if (a.endsWith(tail)) a = a.removeSuffix(tail).trim()
            val lower = a.lowercase()
            val cut = listOf(", ", " & ", " x ", " feat. ", " feat ", " ft. ", " featuring ", " and ", "; ", " / ")
                .map { lower.indexOf(it) }.filter { it > 0 }.minOrNull()
            if (cut != null) a = a.substring(0, cut)
            return a.trim().ifEmpty { artist.trim() }
        }

        /** The best synced lyrics in a search answer: closest in length to the song (within 3 s when known). */
        fun fromSearch(json: String, durationMs: Long): Lyrics? {
            val list = Json.parse(json) as? List<*> ?: return null
            fun off(r: Map<*, *>): Double {
                val have = (r["duration"] as? Number)?.toDouble() ?: return 0.0
                return if (durationMs > 0) kotlin.math.abs(have - durationMs / 1000.0) else 0.0
            }
            return list.filterIsInstance<Map<*, *>>()
                .filter { (it["syncedLyrics"] as? String)?.isNotBlank() == true && off(it) <= 3.0 }
                .minByOrNull { off(it) }
                ?.let { parse(it["syncedLyrics"] as String) }
        }

        /** The synced lyrics in LRCLIB's answer, if it has any. */
        fun fromLrclib(json: String): Lyrics? {
            val v = Json.parse(json) ?: return null
            if (v.at("instrumental") == true) return null
            return (v.at("syncedLyrics") as? String)?.let { parse(it) }
        }
    }
}
