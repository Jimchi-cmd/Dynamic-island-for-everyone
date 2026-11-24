package islet.island

/**
 * Something the island shows for as long as it lasts: a call, a trip, a
 * timer, a song. The most important one sits in the island; a second one
 * gets the small bubble beside it. Times are milliseconds on the
 * `System.currentTimeMillis()` clock unless a field says otherwise.
 */
sealed class Live {
    abstract val id: String

    /** Higher is more important. */
    abstract val priority: Int

    /** A phone or app call. `since` 0 while it is ringing or dialling. */
    data class Call(
        override val id: String,
        val name: String,
        val app: String,
        val incoming: Boolean,
        val since: Long,
    ) : Live() {
        override val priority get() = 100
    }

    /** Turn-by-turn directions from a maps app's notification. */
    data class Navigation(override val id: String, val title: String, val text: String, val app: String) : Live() {
        override val priority get() = 80
    }

    /** The island's own focus timer (or a break between focus sessions). */
    data class Timer(
        val label: String,
        val endsAt: Long,
        val total: Long,
        val pausedLeft: Long?,
        val onBreak: Boolean,
    ) : Live() {
        override val id get() = "timer"
        override val priority get() = 70

        fun left(now: Long) = pausedLeft ?: (endsAt - now).coerceAtLeast(0)
    }

    /** A countdown another app shows (the Clock app's timer, for example). */
    data class Countdown(override val id: String, val label: String, val endsAt: Long, val app: String) : Live() {
        override val priority get() = 70
    }

    /** The island's stopwatch. */
    data class Stopwatch(val startedAt: Long, val before: Long, val running: Boolean, val laps: List<Long>) : Live() {
        override val id get() = "stopwatch"
        override val priority get() = 65

        fun elapsed(now: Long) = before + if (running) (now - startedAt).coerceAtLeast(0) else 0
    }

    /** A calendar event that starts soon or has just started. */
    data class Event(override val id: String, val title: String, val startsAt: Long, val join: String?) : Live() {
        override val priority get() = 60
    }

    /**
     * What is playing. `position` was read at `at` (milliseconds on the
     * elapsed-time clock), so the live position is worked out from it.
     */
    data class Music(
        val title: String,
        val artist: String,
        val app: String,
        val playing: Boolean,
        val duration: Long,
        val position: Long,
        val at: Long,
        /** 1 at normal speed (podcasts and audiobooks can play faster). */
        val speed: Float = 1f,
    ) : Live() {
        override val id get() = "music"
        override val priority get() = 50

        fun positionAt(elapsed: Long): Long {
            val p = if (playing) position + ((elapsed - at).coerceAtLeast(0) * speed).toLong() else position
            return if (duration > 0) p.coerceIn(0, duration) else p.coerceAtLeast(0)
        }
    }

    /** A lesson from the class timetable: counting down to it, then the time left in it. */
    data class Lesson(override val id: String, val name: String, val room: String, val startsAt: Long, val endsAt: Long) : Live() {
        override val priority get() = 62

        fun started(now: Long) = now >= startsAt
    }

    /** A download, upload or other task with a progress bar. `percent` -1: unknown. */
    data class Progress(override val id: String, val title: String, val app: String, val percent: Int) : Live() {
        override val priority get() = 40
    }
}

/** A short pop-up: it grows out of the island, then goes away on its own. */
sealed class Pop {
    /** How long it stays, in milliseconds. */
    abstract val lasts: Long

    companion object {
        /**
         * How long a notification shows (ms): set from the app's choice
         * (`NOTE_LENGTHS`). Short by default, so it is gone before the
         * phone's own notification banner has finished.
         */
        @Volatile var noteMs: Long = 3000L

        /** "brief", "normal" or "long", and how long each shows a notification (ms). */
        val NOTE_LENGTHS = listOf("brief" to 2000L, "normal" to 3000L, "long" to 5000L)

        fun noteMsFor(choice: String): Long = NOTE_LENGTHS.firstOrNull { it.first == choice }?.second ?: 3000L
    }

    /** Pop-ups with the same key replace each other instead of queueing. */
    open val key: String get() = this::class.java.simpleName

    /**
     * `actions`: the notification's own buttons ("Reply", "Mark as read"),
     * at most two; `count`: how many this app has sent lately.
     */
    data class Note(
        override val key: String,
        val app: String,
        val title: String,
        val text: String,
        val actions: List<String> = emptyList(),
        val count: Int = 1,
    ) : Pop() {
        /** Half as long again with buttons, for time to reach them. */
        override val lasts get() = if (actions.isEmpty()) noteMs else noteMs * 3 / 2
    }

    /** A card payment, read from a wallet app's notification. */
    data class Payment(val merchant: String, val amount: String) : Pop() {
        override val lasts get() = 4000L
    }

    data class Charging(val level: Int, val charging: Boolean) : Pop() {
        override val lasts get() = 2500L
        override val key get() = "power"
    }

    data class LowBattery(val level: Int) : Pop() {
        override val lasts get() = 4000L
        override val key get() = "power"
    }

    enum class RingerMode { Ring, Vibrate, Silent }

    data class Ringer(val mode: RingerMode) : Pop() {
        override val lasts get() = 2000L
    }

    data class Airplane(val on: Boolean) : Pop() {
        override val lasts get() = 2500L
    }

    data class DoNotDisturb(val on: Boolean) : Pop() {
        override val lasts get() = 2500L
    }

    /** `name` empty when the network's name can't be read. */
    data class Wifi(val name: String) : Pop() {
        override val lasts get() = 2500L
    }

    /** `battery`: the device's charge (%), or -1 when it doesn't say. */
    data class Bluetooth(val name: String, val connected: Boolean, val battery: Int = -1) : Pop() {
        override val lasts get() = 3000L
        override val key get() = "bluetooth"
    }

    enum class LevelKind { Volume, Brightness }

    /** The volume or brightness changed (0..1), as the computer's island shows it. */
    data class Level(val kind: LevelKind, val fraction: Float) : Pop() {
        override val lasts get() = 1500L
        override val key get() = "level"
    }

    data class Screenshot(val uri: String) : Pop() {
        override val lasts get() = 8000L
    }

    data class TimerDone(val label: String) : Pop() {
        override val lasts get() = 8000L
    }

    /** A plain message, such as "Flashlight on". */
    data class Info(val text: String) : Pop() {
        override val lasts get() = 2000L
        override val key get() = "info"
    }
}
