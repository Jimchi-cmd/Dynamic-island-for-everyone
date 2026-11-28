package islet.island

/** Album covers: reading them at the size shown, and matching them to the song. */
object Covers {
    /**
     * How much to shrink a picture while reading it (a power of two) so it
     * is still at least `side` px across: a 3000 px cover read for a 160 px
     * thumbnail takes a moment and a lot of memory.
     */
    fun sampleSize(width: Int, height: Int, side: Int): Int {
        var n = 1
        while (width / (n * 2) >= side && height / (n * 2) >= side) n *= 2
        return n
    }

    /** Whether a media notification's title is the playing song's (players word them slightly differently). */
    fun sameSong(a: String, b: String): Boolean {
        val x = a.trim().lowercase()
        val y = b.trim().lowercase()
        if (x.isEmpty() || y.isEmpty()) return false
        return x == y || (minOf(x.length, y.length) >= 4 && (x.startsWith(y) || y.startsWith(x)))
    }
}
