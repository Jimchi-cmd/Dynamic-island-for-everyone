package islet.island

/**
 * The focus timer: a countdown, optionally in Pomodoro rounds (focus,
 * short break, focus… and a long break after every fourth focus).
 */
class FocusTimer {
    var label = "Focus"
        private set
    private var endsAt = 0L
    private var total = 0L
    private var pausedLeft: Long? = null
    private var running = false
    var pomodoro = false
        private set
    var onBreak = false
        private set

    /** Focus sessions finished in the current Pomodoro run. */
    var rounds = 0
        private set

    var focusMinutes = 25
    var breakMinutes = 5
    var longBreakMinutes = 15

    val active get() = running

    fun start(minutes: Double, now: Long, label: String = "Focus", pomodoro: Boolean = false) {
        total = (minutes * 60_000).toLong().coerceAtLeast(1000)
        endsAt = now + total
        pausedLeft = null
        running = true
        this.label = label
        this.pomodoro = pomodoro
        onBreak = false
        if (!pomodoro) rounds = 0
    }

    fun startPomodoro(now: Long) {
        rounds = 0
        start(focusMinutes.toDouble(), now, "Focus", pomodoro = true)
    }

    fun pause(now: Long) {
        if (running && pausedLeft == null) pausedLeft = (endsAt - now).coerceAtLeast(0)
    }

    fun resume(now: Long) {
        pausedLeft?.let {
            endsAt = now + it
            pausedLeft = null
        }
    }

    val paused get() = pausedLeft != null

    /** Add minutes (negative to take away); never below zero. */
    fun add(minutes: Int, now: Long) {
        if (!running) return
        val ms = minutes * 60_000L
        pausedLeft?.let { pausedLeft = (it + ms).coerceAtLeast(0) } ?: run { endsAt = maxOf(now, endsAt + ms) }
        total = (total + ms).coerceAtLeast(1000)
    }

    fun stop() {
        running = false
        pausedLeft = null
        pomodoro = false
        onBreak = false
    }

    fun left(now: Long): Long = if (!running) 0 else pausedLeft ?: (endsAt - now).coerceAtLeast(0)

    /**
     * Call when the time may be up. Returns what just finished ("Focus",
     * "Break" or the label), and in Pomodoro mode starts the next part.
     */
    fun check(now: Long): String? {
        if (!running || pausedLeft != null || now < endsAt) return null
        val finished = if (onBreak) "Break" else label
        if (pomodoro) {
            if (onBreak) {
                onBreak = false
                label = "Focus"
                total = focusMinutes * 60_000L
            } else {
                rounds++
                onBreak = true
                label = "Break"
                total = (if (rounds % 4 == 0) longBreakMinutes else breakMinutes) * 60_000L
            }
            endsAt = now + total
        } else {
            running = false
        }
        return finished
    }

    fun live(): Live.Timer? =
        if (!running) null else Live.Timer(label, endsAt, total, pausedLeft, onBreak)
}

/** A stopwatch with laps. */
class Stopwatch {
    private var startedAt = 0L
    private var before = 0L
    var running = false
        private set
    private val laps = ArrayList<Long>()

    val used get() = running || before > 0

    fun start(now: Long) {
        if (running) return
        startedAt = now
        running = true
    }

    fun pause(now: Long) {
        if (!running) return
        before += (now - startedAt).coerceAtLeast(0)
        running = false
    }

    fun reset() {
        running = false
        before = 0
        laps.clear()
    }

    fun elapsed(now: Long) = before + if (running) (now - startedAt).coerceAtLeast(0) else 0

    /** Mark a lap; laps hold each lap's own length. */
    fun lap(now: Long) {
        if (!running) return
        val total = elapsed(now)
        laps.add(total - laps.sum())
        if (laps.size > 99) laps.removeAt(0)
    }

    fun laps(): List<Long> = laps.toList()

    fun live(): Live.Stopwatch? = if (!used) null else Live.Stopwatch(startedAt, before, running, laps.toList())
}
