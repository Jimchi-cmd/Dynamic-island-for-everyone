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

    /**
     * A tap on the empty island: a strip with the battery, the time and
     * memory use, as the computer's island shows when hovered. Tap again
     * for the clock card.
     */
    object Peek : Scene()

    /** The month, with timer buttons (from the clock card's calendar button). */
    object Calendar : Scene()
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
    private var calendar = false
    private var peekUntil = 0L

    private fun peeking(now: Long) = now < peekUntil

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
    fun nextChange(): Long? = listOfNotNull(current?.let { currentUntil }, peekUntil.takeIf { it > 0 }).minOrNull()

    private fun primary(): Live? {
        val sorted = all
        return sorted.firstOrNull { it.id == chosen } ?: sorted.firstOrNull()
    }

    fun scene(now: Long): Scene {
        tick(now)
        if (calendar) return Scene.Calendar
        if (controls) return Scene.Controls
        if (expanded) return Scene.Expanded(primary())
        current?.let { return Scene.Popped(it) }
        if (peekUntil in 1..now) peekUntil = 0
        val first = primary() ?: return when {
            peeking(now) -> Scene.Peek
            showEmpty -> Scene.Idle
            else -> Scene.Hidden
        }
        val second = all.firstOrNull { it.id != first.id }
        return Scene.Compact(first, second)
    }

    // ---- What the user does --------------------------------------------------

    /**
     * Tap the island. An open card closes; a pop-up goes; an activity
     * opens. The empty island first peeks (the strip), then opens the
     * clock card on a second tap.
     */
    fun tap(now: Long) {
        tick(now)
        when {
            calendar -> calendar = false
            controls -> controls = false
            expanded -> expanded = false
            current != null -> dismissPop(now)
            primary() == null && !peeking(now) -> peekUntil = now + PEEK
            else -> {
                expanded = true
                peekUntil = 0
            }
        }
    }

    fun longPress() {
        controls = true
        expanded = false
        calendar = false
        peekUntil = 0
    }

    /** The clock card's calendar button. */
    fun showCalendar() {
        calendar = true
        controls = false
        expanded = false
    }

    /** The clock card's controls button. */
    fun showControls() = longPress()

    /** Swipe up, the back gesture or a tap outside: close it. */
    fun close(now: Long) {
        peekUntil = 0
        if (controls || expanded || calendar) {
            controls = false
            expanded = false
            calendar = false
        } else if (current != null) {
            dismissPop(now)
        }
    }

    val isOpen get() = controls || expanded || calendar

    companion object {
        /** How long the strip stays after a tap. */
        const val PEEK = 4000L
    }

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
