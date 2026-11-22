package islet.island

/** A calendar event, as read from the phone's calendar. */
data class CalendarEvent(
    val id: Long,
    val title: String,
    val begin: Long,
    val end: Long,
    val allDay: Boolean,
    val location: String = "",
    val description: String = "",
)

object Meetings {
    private val LINK = Regex(
        """https://(?:[\w-]+\.)*(?:meet\.google\.com/[a-z]{3}-[a-z]{4}-[a-z]{3}|zoom\.us/(?:j|my|w)/[\w?=&./-]+|""" +
            """teams\.microsoft\.com/l/meetup-join/[^\s"<>]+|teams\.live\.com/meet/[^\s"<>]+|webex\.com/[^\s"<>]+|""" +
            """whereby\.com/[\w-]+|meet\.jit\.si/[\w-]+)""",
        RegexOption.IGNORE_CASE,
    )

    /** The video-call link in an event's place or notes, if there is one. */
    fun joinLink(e: CalendarEvent): String? =
        LINK.find(e.location)?.value ?: LINK.find(e.description)?.value

    /** How long before an event the island starts counting down. */
    const val SOON = 15 * 60_000L

    /** How long after it starts it stays (for a late join). */
    const val GRACE = 5 * 60_000L

    /**
     * The event the island shows now: the next timed one starting within
     * [SOON], or one that started less than [GRACE] ago.
     */
    fun current(events: List<CalendarEvent>, now: Long): CalendarEvent? =
        events.asSequence()
            .filter { !it.allDay && it.end > now && it.begin - now <= SOON && now - it.begin <= GRACE }
            .minByOrNull { it.begin }

    /** The next timed event today or later, for the clock card. */
    fun next(events: List<CalendarEvent>, now: Long): CalendarEvent? =
        events.filter { !it.allDay && it.end > now }.minByOrNull { it.begin }

    /**
     * Today's agenda, as on the computer's card: the timed events still to
     * come or going on today (ending after `now`, starting before `dayEnd`), in order.
     */
    fun today(events: List<CalendarEvent>, now: Long, dayEnd: Long): List<CalendarEvent> =
        events.filter { !it.allDay && it.end > now && it.begin < dayEnd }.sortedBy { it.begin }

    /** When to look again so a countdown starts on time. */
    fun nextCheck(events: List<CalendarEvent>, now: Long): Long? =
        events.filter { !it.allDay && it.begin - SOON > now }.minOfOrNull { it.begin - SOON }
}
