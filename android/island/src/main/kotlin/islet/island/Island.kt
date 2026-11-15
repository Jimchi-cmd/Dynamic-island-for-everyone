package islet.island

/** How the island is drawn right now. */
sealed class Scene {
    /** Nothing to show, and the empty island is switched off. */
    object Hidden : Scene()

    /** The small black pill round the camera. */
    object Idle : Scene()

    /** One activity in the pill, and maybe a second one in the bubble beside it. */
    data class Compact(val live: Live, val second: Live?) : Scene()

    /** A pop-up. */
    data class Popped(val pop: Pop) : Scene()

    /** The big card for one activity (or for the empty island: the clock card). */
    data class Expanded(val live: Live?) : Scene()

    /** Long press: the control centre. */
    object Controls : Scene()
}

/**
 * What the island holds and which of it shows. Plain state with the time
 * passed in, so every rule is tested without a phone.
 */
class Island(var showEmpty: Boolean = true) {
    private val lives = LinkedHashMap<String, Live>()
    private val queue = ArrayList<Pop>()
    private var current: Pop? = null
    private var currentUntil = 0L

    /** The activity the user swiped to, if it is still there. */
    private var chosen: String? = null
    private var expanded = false
    private var controls = false

    val all: List<Live> get() = lives.values.sortedByDescending { it.priority }

    fun get(id: String): Live? = lives[id]

    /** Add or update an activity. */
    fun put(live: Live) {
        lives[live.id] = live
    }

    fun remove(id: String) {
        lives.remove(id)
        if (chosen == id) chosen = null
    }

    /** Remove every activity whose id starts with `prefix` and isn't in `keep`. */
    fun retain(prefix: String, keep: Set<String>) {
        lives.keys.filter { it.startsWith(prefix) && it !in keep }.forEach { remove(it) }
    }

    /**
     * Show a pop-up. One with the same key as the one showing replaces it;
     * otherwise it waits its turn (a newer one with the same key as a
     * waiting one takes that one's place).
     */
    fun pop(p: Pop, now: Long) {
        val showing = current
        if (showing == null || showing.key == p.key || expired(now)) {
            current = p
            currentUntil = now + p.lasts
            return
        }
        queue.removeAll { it.key == p.key }
        queue.add(p)
        if (queue.size > 8) queue.removeAt(0)
    }

    /** The pop-up was tapped away or swiped up. */
    fun dismissPop(now: Long) {
        current = null
        advance(now)
    }

    /** Drop every waiting pop-up with this key (a notification was removed). */
    fun forget(key: String, now: Long) {
        queue.removeAll { it.key == key }
        if (current?.key == key) dismissPop(now)
    }

    private fun expired(now: Long) = current != null && now >= currentUntil

    private fun advance(now: Long) {
        while (current == null || expired(now)) {
            if (queue.isEmpty()) {
                current = null
                return
            }
            val next = queue.removeAt(0)
            current = next
            currentUntil = now + next.lasts
        }
    }

    /** Move on from pop-ups whose time is up. Call before reading `scene`. */
    fun tick(now: Long) {
        if (expired(now)) {
            current = null
            advance(now)
        }
    }

    /** When something next changes on its own (a pop-up ends), or null. */
    fun nextChange(): Long? = current?.let { currentUntil }

    private fun primary(): Live? {
        val sorted = all
        return sorted.firstOrNull { it.id == chosen } ?: sorted.firstOrNull()
    }

    fun scene(now: Long): Scene {
        tick(now)
        if (controls) return Scene.Controls
        if (expanded) return Scene.Expanded(primary())
        current?.let { return Scene.Popped(it) }
        val first = primary() ?: return if (showEmpty) Scene.Idle else Scene.Hidden
        val second = all.firstOrNull { it.id != first.id }
        return Scene.Compact(first, second)
    }

    // ---- What the user does --------------------------------------------------

    /** Tap the island: open it (a pop-up opens its activity's card instead). */
    fun tap(now: Long) {
        tick(now)
        when {
            controls -> controls = false
            expanded -> expanded = false
            current != null -> dismissPop(now)
            else -> expanded = true
        }
    }

    fun longPress() {
        controls = true
        expanded = false
    }

    /** Swipe up, the back gesture or a tap outside: close it. */
    fun close(now: Long) {
        if (controls || expanded) {
            controls = false
            expanded = false
        } else if (current != null) {
            dismissPop(now)
        }
    }

    val isOpen get() = controls || expanded

    /** Swipe sideways on the pill: bring the next activity forward. */
    fun cycle() {
        val sorted = all
        if (sorted.size < 2) return
        val at = sorted.indexOfFirst { it.id == (primary()?.id) }
        chosen = sorted[(at + 1) % sorted.size].id
    }

    /** Bring an activity forward and open it (tapping the bubble). */
    fun open(id: String) {
        if (lives.containsKey(id)) {
            chosen = id
            expanded = true
            controls = false
        }
    }
}
