package islet.island

/**
 * Live activities from other apps (Tasker, Automate, MacroDroid, a
 * script), as `islet activity` puts them on the computer's island: a
 * title and a progress ring, started, updated and ended by id.
 */
object Automation {
    /** At most this many at once, so a runaway automation can't crowd the island. */
    const val MAX = 3

    private val ID = Regex("""[A-Za-z0-9._-]{1,40}""")

    /** The island's id for an outside activity. */
    fun key(id: String) = "auto:$id"

    /**
     * The activity to show for "start" or "update", or null if the request
     * isn't usable: an id of letters, digits, dots, dashes or underscores,
     * a title (cut to 60 characters), and a progress of 0–100 (anything
     * else shows a spinner).
     */
    fun live(id: String?, title: String?, progress: Int?, from: String): Live.Progress? {
        if (id == null || !ID.matches(id)) return null
        val name = title?.trim()?.take(60)?.takeIf { it.isNotEmpty() } ?: return null
        val percent = progress?.takeIf { it in 0..100 } ?: -1
        return Live.Progress(key(id), name, from.take(40), percent)
    }

    /** Whether one more outside activity fits (an update to one already shown always does). */
    fun fits(shown: List<Live>, id: String): Boolean {
        val outside = shown.filter { it.id.startsWith("auto:") }
        return outside.any { it.id == key(id) } || outside.size < MAX
    }
}
