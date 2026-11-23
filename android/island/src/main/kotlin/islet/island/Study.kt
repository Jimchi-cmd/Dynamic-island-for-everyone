package islet.island

import java.util.Calendar
import java.util.TimeZone

/** A lesson in the weekly class timetable. `day` 0 = Monday … 6 = Sunday; times in minutes after midnight. */
data class Lesson(val day: Int, val start: Int, val end: Int, val name: String, val room: String = "")

/**
 * A weekly class timetable, entered once: the island then counts down to
 * each lesson and shows how long is left in it. Kept on the phone only.
 */
class Timetable(lessons: List<Lesson> = emptyList()) {
    val lessons: List<Lesson> = lessons.sortedWith(compareBy({ it.day }, { it.start }, { it.end }))

    /** Add a lesson. Returns the new timetable, or an error to show. */
    fun add(day: Int, start: Int, end: Int, name: String, room: String = ""): Result<Timetable> {
        val n = tidy(name)
        return when {
            n.isEmpty() -> Result.failure(IllegalArgumentException("Give the lesson a name."))
            day !in 0..6 || start !in 0 until 24 * 60 || end !in 1..24 * 60 ->
                Result.failure(IllegalArgumentException("That isn't a time in the week."))
            end <= start -> Result.failure(IllegalArgumentException("A lesson ends after it starts."))
            lessons.size >= MAX -> Result.failure(IllegalArgumentException("A timetable holds up to $MAX lessons. Remove one first."))
            else -> {
                val l = Lesson(day, start, end, n, tidy(room))
                if (l in lessons) Result.failure(IllegalArgumentException("That lesson is already there.")) else Result.success(Timetable(lessons + l))
            }
        }
    }

    fun remove(index: Int) = Timetable(lessons.filterIndexed { i, _ -> i != index })

    /** Where the day stands: a lesson on now, or the next one today. */
    sealed class Status {
        data class Now(val lesson: Lesson, val left: Int) : Status()
        data class Next(val lesson: Lesson, val startsIn: Int, val free: Boolean) : Status()
    }

    fun status(day: Int, minute: Int): Status? {
        val today = lessons.filter { it.day == day }
        today.firstOrNull { minute >= it.start && minute < it.end }?.let { return Status.Now(it, it.end - minute) }
        val next = today.filter { it.start > minute }.minByOrNull { it.start } ?: return null
        return Status.Next(next, next.start - minute, today.any { it.end <= minute })
    }

    /** One line: "Maths · 20 min left", "Free 45 min · then Physics", "Maths in 25 min", "Maths at 14:00". */
    fun line(day: Int, minute: Int, h24: Boolean = true): String? = when (val s = status(day, minute)) {
        null -> null
        is Status.Now -> "${s.lesson.name} · ${mins(s.left)} left"
        is Status.Next -> when {
            s.free -> "Free ${mins(s.startsIn)} · then ${s.lesson.name}"
            s.startsIn <= 90 -> "${s.lesson.name} in ${mins(s.startsIn)}"
            else -> "${s.lesson.name} at ${hm(s.lesson.start, h24)}"
        }
    }

    /**
     * The lesson the island shows at `now`: from [Meetings.SOON] before it
     * starts until it ends. Times are on the wall clock of `zone`.
     */
    fun current(now: Long, zone: TimeZone = TimeZone.getDefault()): Pair<Lesson, Long>? =
        around(now, zone).firstOrNull { (l, begin) -> now < begin + (l.end - l.start) * 60_000L && begin - now <= Meetings.SOON }

    /** When to look again so a countdown starts on time (or a lesson's card goes when it ends). */
    fun nextCheck(now: Long, zone: TimeZone = TimeZone.getDefault()): Long? =
        around(now, zone).flatMap { (l, begin) -> listOf(begin - Meetings.SOON, begin, begin + (l.end - l.start) * 60_000L) }
            .filter { it > now }.minOrNull()

    /** Lessons from yesterday to tomorrow, each with its start time. */
    private fun around(now: Long, zone: TimeZone): List<Pair<Lesson, Long>> {
        val out = ArrayList<Pair<Lesson, Long>>()
        for (shift in -1..1) {
            val cal = Calendar.getInstance(zone).apply {
                timeInMillis = now
                add(Calendar.DAY_OF_MONTH, shift)
                set(Calendar.HOUR_OF_DAY, 0)
                set(Calendar.MINUTE, 0)
                set(Calendar.SECOND, 0)
                set(Calendar.MILLISECOND, 0)
            }
            val day = weekday(cal)
            for (l in lessons.filter { it.day == day }) {
                val at = cal.clone() as Calendar
                at.add(Calendar.MINUTE, l.start)
                out.add(l to at.timeInMillis)
            }
        }
        return out.sortedBy { it.second }
    }

    fun encode(): String = lessons.joinToString("\n") { "${it.day}|${it.start}|${it.end}|${it.name.replace('|', '/')}|${it.room.replace('|', '/')}" }

    companion object {
        const val MAX = 80
        const val MAX_LEN = 40
        val DAYS = listOf("Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday")

        fun decode(s: String?): Timetable = Timetable(
            s.orEmpty().lines().mapNotNull { line ->
                val p = line.split('|')
                if (p.size < 4) return@mapNotNull null
                val day = p[0].toIntOrNull() ?: return@mapNotNull null
                val start = p[1].toIntOrNull() ?: return@mapNotNull null
                val end = p[2].toIntOrNull() ?: return@mapNotNull null
                if (day !in 0..6 || end <= start || p[3].isBlank()) null else Lesson(day, start, end, p[3], p.getOrElse(4) { "" })
            },
        )

        /** 0 = Monday for a calendar's day. */
        fun weekday(cal: Calendar): Int = Math.floorMod(cal.get(Calendar.DAY_OF_WEEK) - Calendar.MONDAY, 7)

        fun weekday(now: Long, zone: TimeZone = TimeZone.getDefault()): Int =
            weekday(Calendar.getInstance(zone).apply { timeInMillis = now })

        fun minuteOfDay(now: Long, zone: TimeZone = TimeZone.getDefault()): Int {
            val c = Calendar.getInstance(zone).apply { timeInMillis = now }
            return c.get(Calendar.HOUR_OF_DAY) * 60 + c.get(Calendar.MINUTE)
        }

        /** "45 min", "1 h", "1 h 30". */
        fun mins(m: Int): String = when {
            m < 60 -> Words.f("{} min", m)
            m % 60 == 0 -> Words.f("{} h", m / 60)
            else -> Words.f("{} h {}", m / 60, "%02d".format(m % 60))
        }

        /** "14:00", or "2:00 PM". */
        fun hm(minute: Int, h24: Boolean): String {
            val h = minute / 60
            val m = minute % 60
            if (h24) return "%02d:%02d".format(h, m)
            val h12 = if (h % 12 == 0) 12 else h % 12
            return "%d:%02d %s".format(h12, m, Words.t(if (h < 12) "AM" else "PM"))
        }

        private fun tidy(s: String) = s.split(Regex("\\s+")).filter { it.isNotEmpty() }.joinToString(" ").take(MAX_LEN)
    }
}

/**
 * Deadlines: calendar events whose title says exam, essay, assignment,
 * "due" and the like, counted down on the calendar card.
 */
object Deadlines {
    private val WORDS = setOf(
        "deadline", "due", "essay", "exam", "exams", "test", "quiz", "assignment", "homework", "coursework",
        "submit", "submission", "midterm", "finals", "hand-in", "project", "report", "presentation",
    )

    /** Whole words only, so "Contest" and "Duet practice" are not deadlines. */
    fun isDeadline(title: String): Boolean {
        val lower = title.lowercase()
        return "hand in" in lower || lower.split(Regex("[^\\p{L}\\p{N}-]+")).any { it in WORDS }
    }

    /** How long until it: "in 40 min", "in 5 h", "today", "tomorrow", "in 3 days". */
    fun countdown(e: CalendarEvent, now: Long, zone: TimeZone = TimeZone.getDefault()): String {
        val left = e.begin - now
        if (!e.allDay && left <= 0) return "now"
        if (!e.allDay && left < 3_600_000) return Words.f("in {} min", (left + 59_999) / 60_000)
        // All-day events are stored at midnight UTC.
        val days = Streak.dayOf(e.begin, if (e.allDay) UTC else zone) - Streak.dayOf(now, zone)
        if (!e.allDay && left < 24 * 3_600_000 && days <= 1) return Words.f("in {} h", (left + 1_800_000) / 3_600_000)
        return when {
            days <= 0 -> Words.t("today")
            days == 1L -> Words.t("tomorrow")
            else -> Words.f("in {} days", days)
        }
    }

    /** The nearest deadline in the next two weeks (one already under way today still counts). */
    fun next(events: List<CalendarEvent>, now: Long): CalendarEvent? =
        events.asSequence()
            .filter { isDeadline(it.title) && it.end > now && it.begin - now <= 14 * 86_400_000L }
            .minByOrNull { it.begin }

    private val UTC: TimeZone = TimeZone.getTimeZone("UTC")
}

/** The focus streak: how many days in a row at least one focus round was finished. */
data class Streak(val lastDay: Long = 0, val days: Int = 0, val best: Int = 0, val rounds: Int = 0, val today: Int = 0) {
    /** A focus round was finished on `day` (days since 1970, local). */
    fun record(day: Long): Streak = when {
        days == 0 || day > lastDay + 1 -> Streak(day, 1, maxOf(best, 1), rounds + 1, 1)
        day == lastDay + 1 -> Streak(day, days + 1, maxOf(best, days + 1), rounds + 1, 1)
        // The clock went backwards: count the round, leave the run alone.
        day < lastDay -> copy(rounds = rounds + 1)
        else -> copy(rounds = rounds + 1, today = today + 1)
    }

    /** Days in a row as of `day`: the run survives until a whole day is missed. */
    fun current(day: Long): Int = if (days > 0 && day <= lastDay + 1) days else 0

    fun doneOn(day: Long): Int = if (days > 0 && day == lastDay) today else 0

    /** "3 days in a row", or null before the second day. */
    fun label(day: Long): String? = current(day).takeIf { it >= 2 }?.let { Words.f("{} days in a row", it) }

    fun encode() = "$lastDay,$days,$best,$rounds,$today"

    companion object {
        fun decode(s: String?): Streak {
            val p = s.orEmpty().split(',').map { it.trim().toLongOrNull() }
            if (p.size != 5 || p.any { it == null }) return Streak()
            return Streak(p[0]!!, p[1]!!.toInt(), p[2]!!.toInt(), p[3]!!.toInt(), p[4]!!.toInt())
        }

        /** Days since 1970 on the local calendar. */
        fun dayOf(ms: Long, zone: TimeZone = TimeZone.getDefault()): Long = Math.floorDiv(ms + zone.getOffset(ms), 86_400_000L)
    }
}
