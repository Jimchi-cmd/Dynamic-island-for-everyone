package islet.island

import java.util.Calendar
import java.util.TimeZone
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class ExtrasTest {
    private val utc: TimeZone = TimeZone.getTimeZone("UTC")

    /** Monday 2026-11-16 at hh:mm, UTC. */
    private fun monday(h: Int, m: Int = 0): Long = Calendar.getInstance(utc).apply {
        clear()
        set(2026, Calendar.NOVEMBER, 16, h, m)
    }.timeInMillis

    private fun week(): Timetable {
        var t = Timetable()
        t = t.add(0, 9 * 60, 10 * 60, "Maths", "B12").getOrThrow()
        t = t.add(0, 11 * 60, 12 * 60, "Physics").getOrThrow()
        t = t.add(2, 8 * 60, 9 * 60, "  Art   club ").getOrThrow()
        return t
    }

    @Test
    fun a_timetable_keeps_its_lessons_in_order_and_says_no_to_nonsense() {
        val t = week()
        assertEquals(listOf("Maths", "Physics", "Art club"), t.lessons.map { it.name })
        assertTrue(t.add(0, 600, 540, "Back").isFailure, "ends before it starts")
        assertTrue(t.add(0, 540, 600, " ").isFailure, "no name")
        assertTrue(t.add(7, 540, 600, "Sunday+1").isFailure)
        assertTrue(t.add(0, 9 * 60, 10 * 60, "Maths", "B12").isFailure, "already there")
        assertEquals(t.lessons, Timetable.decode(t.encode()).lessons, "it survives saving")
        assertEquals(listOf("Maths", "Art club"), t.remove(1).lessons.map { it.name })
        assertTrue(Timetable.decode("junk\n9|1|2|x\n0|5|4|y").lessons.isEmpty())
    }

    @Test
    fun the_timetable_line_counts_down_to_lessons_and_gaps() {
        val t = week()
        assertEquals("Maths in 30 min", t.line(0, 8 * 60 + 30))
        assertEquals("Maths at 09:00", t.line(0, 7 * 60))
        assertEquals("Maths at 9:00 AM", t.line(0, 7 * 60, h24 = false))
        assertEquals("Maths · 20 min left", t.line(0, 9 * 60 + 40))
        assertEquals("Free 1 h · then Physics", t.line(0, 10 * 60))
        assertNull(t.line(0, 12 * 60), "the day's lessons are over")
        assertNull(t.line(1, 9 * 60), "nothing on Tuesday")
    }

    @Test
    fun the_island_shows_a_lesson_from_fifteen_minutes_before_until_it_ends() {
        val t = week()
        assertNull(t.current(monday(8, 40), utc))
        assertEquals("Maths", t.current(monday(8, 46), utc)?.first?.name)
        assertEquals(monday(9), t.current(monday(8, 46), utc)?.second)
        assertEquals("Maths", t.current(monday(9, 59), utc)?.first?.name)
        assertNull(t.current(monday(10, 0), utc))
        assertEquals(monday(8, 45), t.nextCheck(monday(8), utc), "wakes when the countdown starts")
        assertEquals(monday(10), t.nextCheck(monday(9, 30), utc), "and when the lesson ends")
        assertEquals(0, Timetable.weekday(monday(12), utc))
        assertEquals(9 * 60 + 5, Timetable.minuteOfDay(monday(9, 5), utc))
    }

    @Test
    fun deadlines_are_found_by_whole_words_and_counted_down() {
        assertTrue(Deadlines.isDeadline("History essay"))
        assertTrue(Deadlines.isDeadline("Hand in lab report"))
        assertTrue(Deadlines.isDeadline("Maths EXAM"))
        assertFalse(Deadlines.isDeadline("Contest"))
        assertFalse(Deadlines.isDeadline("Duet practice"))
        val now = monday(12)
        fun at(ms: Long, allDay: Boolean = false) = CalendarEvent(1, "Essay due", ms, ms + 3_600_000, allDay)
        assertEquals("in 40 min", Deadlines.countdown(at(now + 40 * 60_000L), now, utc))
        assertEquals("in 5 h", Deadlines.countdown(at(now + 5 * 3_600_000L), now, utc))
        assertEquals("now", Deadlines.countdown(at(now - 60_000), now, utc))
        assertEquals("tomorrow", Deadlines.countdown(at(monday(0) + 86_400_000L, allDay = true), now, utc))
        assertEquals("today", Deadlines.countdown(at(monday(0), allDay = true), now, utc))
        assertEquals("in 3 days", Deadlines.countdown(at(now + 3 * 86_400_000L), now, utc))
        val events = listOf(
            CalendarEvent(1, "Team lunch", now + 3_600_000, now + 7_200_000, false),
            CalendarEvent(2, "Physics test", now + 2 * 86_400_000L, now + 2 * 86_400_000L + 3_600_000, false),
            CalendarEvent(3, "Final exam", now + 30 * 86_400_000L, now + 30 * 86_400_000L + 3_600_000, false),
        )
        assertEquals(2L, Deadlines.next(events, now)?.id, "the nearest within two weeks")
    }

    @Test
    fun the_focus_streak_grows_daily_and_breaks_after_a_missed_day() {
        var s = Streak()
        assertNull(s.label(100))
        s = s.record(100)
        assertEquals(1, s.current(100))
        assertNull(s.label(100), "one day isn't a streak yet")
        s = s.record(100).record(101)
        assertEquals("2 days in a row", s.label(101))
        assertEquals(2, s.current(102), "it lasts through the next day")
        assertEquals(0, s.current(103), "a whole day missed ends it")
        s = s.record(105)
        assertEquals(1, s.current(105))
        assertEquals(2, s.best)
        assertEquals(4, s.rounds)
        assertEquals(1, s.doneOn(105))
        assertEquals(s, Streak.decode(s.encode()))
        assertEquals(Streak(), Streak.decode("rubbish"))
    }

    @Test
    fun the_launcher_finds_apps_by_start_word_and_letters() {
        val all = listOf(
            Launchable("YouTube", "com.google.android.youtube"),
            Launchable("Google Maps", "com.google.android.apps.maps"),
            Launchable("Maps link", "https://maps.example", isLink = true),
            Launchable("Messages", "com.google.android.apps.messaging"),
            Launchable("Spotify", "com.spotify.music"),
        )
        assertEquals(listOf("Maps link", "Google Maps"), Launcher.search("maps", all).map { it.name }, "a name that starts with it first")
        assertEquals("YouTube", Launcher.search("yt", all).first().name, "letters in order")
        assertEquals(listOf("Maps link", "Messages", "Google Maps"), Launcher.search("m", all).map { it.name }, "a tie puts your links first")
        assertTrue(Launcher.search("zzz", all).isEmpty())
        assertEquals("Maps link", Launcher.search("", all).first().name, "links lead the empty list")
    }

    @Test
    fun only_safe_links_are_kept() {
        assertTrue(Launcher.safeLink("https://example.com"))
        assertTrue(Launcher.safeLink("tel:+441234"))
        assertTrue(Launcher.safeLink("spotify:playlist:abc"))
        assertFalse(Launcher.safeLink("javascript:alert(1)"))
        assertFalse(Launcher.safeLink("file:///sdcard/x"))
        assertFalse(Launcher.safeLink("intent://x#Intent;end"))
        assertFalse(Launcher.safeLink("nothing"))
        assertEquals("https://example.com", Launcher.tidyLink(" example.com "))
        val links = listOf(Link("Mum", "tel:123"), Link("A|B", "https://x.org"))
        assertEquals(listOf(Link("Mum", "tel:123"), Link("A/B", "https://x.org")), Launcher.decodeLinks(Launcher.encodeLinks(links)))
        assertTrue(Launcher.decodeLinks("Bad|javascript:x").isEmpty())
    }

    @Test
    fun live_bars_follow_the_sound() {
        val silent = ByteArray(128)
        assertContentEquals(FloatArray(4), Bars.levels(silent))
        val bass = ByteArray(128).also { it[2] = 127; it[3] = 127 }
        val l = Bars.levels(bass)
        assertTrue(l[0] > 0.9f, "a loud low note fills the first bar")
        assertEquals(0f, l[3])
        val eased = Bars.smooth(floatArrayOf(0f, 1f, 0f, 0f), floatArrayOf(1f, 0f, 0f, 0f))
        assertTrue(eased[0] > 0.5f && eased[1] > 0.5f, "quick up, slower down")
    }

    @Test
    fun the_song_position_runs_at_the_playback_speed() {
        val normal = Live.Music("A", "B", "P", true, 100_000, 10_000, at = 1_000)
        assertEquals(12_000, normal.positionAt(3_000))
        val fast = normal.copy(speed = 1.5f)
        assertEquals(13_000, fast.positionAt(3_000), "a podcast at 1.5×")
        assertEquals(10_000, normal.copy(playing = false).positionAt(9_000), "paused: it stays put")
    }

    @Test
    fun rates_and_the_sleep_timer_steps() {
        assertEquals("0 KB/s", Format.rate(0.0))
        assertEquals("340 KB/s", Format.rate(340_000.0))
        assertEquals("1.2 MB/s", Format.rate(1_200_000.0))
        assertEquals("12 MB/s", Format.rate(12_400_000.0))
        assertEquals(15, Format.nextSleep(null))
        assertEquals(30, Format.nextSleep(15))
        assertEquals(15, Format.nextSleep(14))
        assertNull(Format.nextSleep(60))
    }
}
