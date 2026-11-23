package islet.island

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

class IslandTest {
    private val music = Live.Music("Song", "Band", "Player", true, 200_000, 10_000, 0)
    private val call = Live.Call("call:1", "Mum", "Phone", false, 1000)

    @Test
    fun the_most_important_activity_leads_and_the_next_gets_the_bubble() {
        val i = Island()
        assertEquals(Scene.Idle, i.scene(0))
        i.put(music)
        assertEquals(Scene.Compact(music, null), i.scene(0))
        i.put(call)
        assertEquals(Scene.Compact(call, music), i.scene(0))
        i.cycle()
        assertEquals(Scene.Compact(music, call), i.scene(0), "a swipe brings the next one forward")
        i.remove("music")
        assertEquals(Scene.Compact(call, null), i.scene(0))
        i.remove("call:1")
        i.showEmpty = false
        assertEquals(Scene.Hidden, i.scene(0))
    }

    @Test
    fun pop_ups_show_in_turn_and_same_kind_replaces() {
        val i = Island()
        val a = Pop.Note("n1", "Chat", "Ann", "Hi")
        val b = Pop.Note("n2", "Chat", "Bob", "Yo")
        i.pop(a, 0)
        i.pop(b, 100)
        assertEquals(Scene.Popped(a), i.scene(1000))
        assertEquals(Scene.Popped(b), i.scene(5000), "the second waits for the first")
        assertEquals(Scene.Idle, i.scene(10_100))
        i.pop(Pop.Charging(50, true), 20_000)
        i.pop(Pop.LowBattery(9), 20_500)
        assertEquals(Scene.Popped(Pop.LowBattery(9)), i.scene(20_600), "power pop-ups replace each other")
        assertEquals(20_500 + 4000L, i.nextChange())
    }

    @Test
    fun the_control_centre_goes_back_to_the_card_it_came_from() {
        val i = Island()
        i.put(music)
        i.open("music")
        i.showControls()
        assertEquals(Scene.Controls, i.scene(0))
        i.back()
        assertEquals(Scene.Expanded(music), i.scene(0), "back to the music card")
        i.close(0)
        i.remove("music")
        i.showCalendar()
        i.showControls()
        i.back()
        assertEquals(Scene.Calendar, i.scene(0), "back to the calendar")
        i.close(0)
        i.longPress()
        i.back()
        assertEquals(Scene.Expanded(null), i.scene(0), "opened by a long press: back to the main card")
        i.close(0)
        i.back()
        assertEquals(false, i.isOpen, "back does nothing when the control centre isn't open")
        // The control centre's toggles, and back from them.
        i.put(music)
        i.open("music")
        i.showControls()
        assertEquals(music, i.openedFrom, "it knows the card it came from (\"Now Playing ›\")")
        i.showToggles()
        assertEquals(Scene.Toggles, i.scene(0))
        i.back()
        assertEquals(Scene.Controls, i.scene(0), "back to the control centre")
        i.back()
        assertEquals(Scene.Expanded(music), i.scene(0), "and on to the music card")
        i.showToggles()
        i.back()
        assertEquals(Scene.Expanded(music), i.scene(0), "toggles from a card go back to it")
        i.longPress()
        assertNull(i.openedFrom, "a long press comes from no card")
        i.showToggles()
        i.tap(0)
        assertEquals(false, i.isOpen, "a tap closes the toggles")
    }

    @Test
    fun tapping_opens_and_closes_and_long_press_shows_controls() {
        val i = Island()
        i.put(music)
        i.tap(0)
        assertEquals(Scene.Expanded(music), i.scene(0))
        i.tap(0)
        assertIs<Scene.Compact>(i.scene(0))
        i.longPress()
        assertEquals(Scene.Controls, i.scene(0))
        i.close(0)
        assertIs<Scene.Compact>(i.scene(0))
        i.pop(Pop.Info("Flashlight on"), 0)
        i.tap(0)
        assertIs<Scene.Compact>(i.scene(0), "a tap sends a pop-up away")
        i.remove("music")
        i.tap(1000)
        assertEquals(Scene.Peek, i.scene(1000), "the empty island peeks first")
        assertEquals(1000 + Island.PEEK, i.nextChange())
        assertEquals(Scene.Idle, i.scene(1000 + Island.PEEK), "and the strip goes by itself")
        i.tap(9000)
        i.tap(9500)
        assertEquals(Scene.Expanded(null), i.scene(9500), "a second tap opens the clock card")
        i.showCalendar()
        assertEquals(Scene.Calendar, i.scene(9500))
        i.tap(9500)
        assertEquals(Scene.Idle, i.scene(9500))
        i.put(music)
        i.put(call)
        i.open("music")
        assertEquals(Scene.Expanded(music), i.scene(0), "tapping the bubble opens that one")
    }

    @Test
    fun a_removed_notification_takes_its_pop_up_with_it() {
        val i = Island()
        i.pop(Pop.Note("a", "X", "t", "x"), 0)
        i.pop(Pop.Note("b", "X", "t", "x"), 0)
        i.forget("a", 10)
        assertEquals(Scene.Popped(Pop.Note("b", "X", "t", "x")), i.scene(10))
    }

    @Test
    fun notifications_are_sorted_into_calls_trips_timers_progress_and_pop_ups() {
        val base = NoteFacts(key = "k", pkg = "com.chat", app = "Chat", title = "Ann", text = "Hello")
        assertEquals(NoteKind.Popup, Notes.classify(base, "me"))
        assertEquals(NoteKind.Ignore, Notes.classify(base, "com.chat"), "never our own")
        assertEquals(NoteKind.Ignore, Notes.classify(base.copy(ongoing = true), "me"))
        assertEquals(NoteKind.Ignore, Notes.classify(base.copy(groupSummary = true), "me"))
        assertEquals(NoteKind.Ignore, Notes.classify(base.copy(media = true), "me"))
        assertEquals(NoteKind.Ignore, Notes.classify(base.copy(title = "", text = " "), "me"))
        assertEquals(
            NoteKind.Call("Mum", true, 0),
            Notes.classify(base.copy(category = "call", title = "Mum", ongoing = true, actions = listOf("decline", "answer")), "me"),
        )
        assertEquals(
            NoteKind.Call("Mum", false, 5000),
            Notes.classify(base.copy(callStyle = true, title = "Mum", ongoing = true, chronometer = true, whenMs = 5000, actions = listOf("hang up")), "me"),
        )
        assertEquals(
            NoteKind.Navigation("200 m", "Turn left onto Main St"),
            Notes.classify(base.copy(pkg = "com.google.android.apps.maps", ongoing = true, title = "200 m", text = "Turn left onto Main St"), "me"),
        )
        assertEquals(
            NoteKind.Countdown("Timer", 90_000),
            Notes.classify(base.copy(ongoing = true, chronometer = true, countDown = true, whenMs = 90_000, title = "Timer"), "me"),
        )
        assertEquals(NoteKind.Progress("file.zip", 25), Notes.classify(base.copy(title = "file.zip", progress = 25, progressMax = 100, ongoing = true), "me"))
        assertEquals(NoteKind.Progress("Ann", -1), Notes.classify(base.copy(indeterminate = true), "me"))
        // Finished, but the notification stays with a full bar: no ring.
        val done = base.copy(title = "file.zip", progress = 100, progressMax = 100)
        assertEquals(NoteKind.Finished("file.zip"), Notes.classify(done, "me"))
        assertEquals(NoteKind.Progress("file.zip", 100), Notes.classify(done.copy(ongoing = true), "me"), "still running")
        assertEquals(NoteKind.Progress("file.zip", 40), Notes.classify(done.copy(progress = 40), "me"), "paused")
        val wallet = base.copy(pkg = "com.google.android.apps.walletnfcrel", title = "Coffee Corner", text = "\$12.50 with Visa ••1234")
        assertEquals(NoteKind.Payment("Coffee Corner", "\$12.50"), Notes.classify(wallet, "me"))
        assertEquals(NoteKind.Payment("Supermarket", "UGX 25,000"), Notes.classify(wallet.copy(title = "UGX 25,000", text = "Supermarket"), "me"))
        assertEquals(NoteKind.Popup, Notes.classify(wallet.copy(title = "Card added", text = "Ready to pay"), "me"), "no amount: an ordinary pop-up")
        assertEquals("12,50 €", Notes.amount("Paid 12,50 € at Bäckerei"))
    }

    @Test
    fun the_focus_timer_runs_pauses_and_goes_round_in_pomodoro() {
        val t = FocusTimer()
        t.start(1.0, 0)
        assertEquals(60_000, t.left(0))
        t.pause(20_000)
        assertEquals(40_000, t.left(50_000), "paused time doesn't count")
        t.resume(50_000)
        assertNull(t.check(80_000))
        assertEquals("Focus", t.check(90_000))
        assertNull(t.live(), "a plain timer stops when done")

        t.focusMinutes = 1
        t.breakMinutes = 1
        t.longBreakMinutes = 2
        t.startPomodoro(0)
        var now = 0L
        val seen = ArrayList<String>()
        repeat(8) {
            now += 200_000
            seen.add(t.check(now) ?: "-")
        }
        assertEquals(listOf("Focus", "Break", "Focus", "Break", "Focus", "Break", "Focus", "Break"), seen)
        assertEquals(4, t.rounds)
        t.add(5, now)
        assertTrue(t.live()!!.total > 60_000)
    }

    @Test
    fun the_stopwatch_keeps_laps() {
        val s = Stopwatch()
        assertNull(s.live())
        s.start(1000)
        s.lap(3000)
        s.lap(7000)
        s.pause(8000)
        assertEquals(7000, s.elapsed(99_000))
        assertEquals(listOf(2000L, 4000L), s.laps())
        s.start(10_000)
        assertEquals(9000, s.elapsed(12_000))
        s.reset()
        assertNull(s.live())
    }

    @Test
    fun times_read_like_a_clock() {
        assertEquals("4:05", Format.clock(244_100))
        assertEquals("1:02:03", Format.clock(3_723_000))
        assertEquals("0:00", Format.clock(-5))
        assertEquals("1:04.37", Format.stopwatch(64_370))
        assertEquals("0:07", Format.elapsed(7_900))
        assertEquals("in 12 min", Format.until(11 * 60_000 + 1))
        assertEquals("in 1 h 5 min", Format.until(65 * 60_000))
        assertEquals("now", Format.until(-1000))
        assertEquals("5 min ago", Format.until(-5 * 60_000))
        assertEquals("21°", Format.temperature(21.4, false))
        assertEquals("70°", Format.temperature(21.0, true))
        assertEquals("Hello w…", Format.cut("Hello world", 8))
    }

    @Test
    fun the_spring_overshoots_a_little_then_settles() {
        val s = Spring(0.0)
        s.target = 1.0
        var peak = 0.0
        var steps = 0
        while (!s.step(1.0 / 60) && steps < 600) {
            peak = maxOf(peak, s.value)
            steps++
        }
        assertEquals(1.0, s.value)
        assertTrue(peak in 1.0..1.15, "overshoot $peak")
        assertTrue(steps in 12..30, "quick, as on an iPhone: about a third of a second, $steps frames")
    }

    @Test
    fun lyrics_follow_the_song() {
        val l = Lyrics.parse("[ar:x]\n[00:01.00]One\n[00:03.5][00:09.00]Two\n[00:05.250]\n")!!
        assertNull(l.lineAt(500))
        assertEquals("One", l.lineAt(2000))
        assertEquals("Two", l.lineAt(3600))
        assertNull(l.lineAt(6000), "an empty line clears it")
        assertEquals("Two", l.lineAt(9000))
        assertEquals("Two", l.lineAt(8000, offset = 1000))
        // What's coming next, for the big music card (empty lines skipped).
        assertEquals("One", l.nextAfter(0))
        assertEquals("Two", l.nextAfter(2000))
        assertEquals("Two", l.nextAfter(4000))
        assertNull(l.nextAfter(9500))
        assertEquals("One", Lyrics.fromLrclib("""{"syncedLyrics":"[00:00.10]One","instrumental":false}""")!!.lineAt(200))
        assertNull(Lyrics.fromLrclib("""{"instrumental":true,"syncedLyrics":null}"""))
        assertTrue(Lyrics.lrclibUrl("A B", "C&D", 200_000).endsWith("track_name=A+B&artist_name=C%26D&duration=200"))
    }

    @Test
    fun weather_and_places_are_read_from_open_meteo() {
        val w = Forecast.parse(
            """{"current":{"temperature_2m":23.4,"weather_code":61,"is_day":0},"daily":{"temperature_2m_max":[27.1],"temperature_2m_min":[18.0]}}""",
        )!!
        assertEquals(Weather(23.4, 61, false, 27.1, 18.0), w)
        assertEquals(Sky.Rain, w.sky)
        assertEquals("Light rain", w.words)
        assertNull(Forecast.parse("""{"error":true}"""))
        val p = Forecast.places(
            """{"results":[{"name":"Kampala","latitude":0.31,"longitude":32.58,"country":"Uganda","admin1":"Central Region"},{"name":"x"}]}""",
        )
        assertEquals(listOf(Place("Kampala", "Central Region", "Uganda", 0.31, 32.58)), p)
        assertEquals("Kampala, Central Region, Uganda", p[0].label)
        assertTrue(Forecast.url(0.31, 32.58).contains("latitude=0.3100&longitude=32.5800"))
        assertTrue(Forecast.searchUrl("New York").contains("name=New+York"))
    }

    @Test
    fun json_reads_what_the_services_send() {
        val v = Json.parse(""" {"a":[1,2.5,{"b":"x\"yé"}],"t":true,"n":null} """)
        assertEquals(2.5, v.at("a", 1))
        assertEquals("x\"yé", v.at("a", 2, "b"))
        assertEquals(true, v.at("t"))
        assertNull(v.at("n"))
        assertNull(Json.parse("{\"a\":}"))
        assertNull(Json.parse("[1,2] x"))
        assertEquals("\"a\\\"b\\n\"", quote("a\"b\n"))
    }

    @Test
    fun meetings_count_down_and_offer_the_join_link() {
        val e = CalendarEvent(1, "Standup", 1_000_000, 2_000_000, false, description = "Join: https://meet.google.com/abc-defg-hij ok")
        assertEquals("https://meet.google.com/abc-defg-hij", Meetings.joinLink(e))
        assertEquals("https://us02web.zoom.us/j/123?pwd=x", Meetings.joinLink(e.copy(description = "", location = "https://us02web.zoom.us/j/123?pwd=x")))
        assertNull(Meetings.joinLink(e.copy(description = "Room 4")))
        val allDay = e.copy(id = 2, allDay = true, begin = 0)
        assertNull(Meetings.current(listOf(e, allDay), 1_000_000 - Meetings.SOON - 1))
        assertEquals(e, Meetings.current(listOf(e, allDay), 1_000_000 - 60_000))
        assertEquals(e, Meetings.current(listOf(e), 1_000_000 + 60_000))
        assertNull(Meetings.current(listOf(e), 1_000_000 + Meetings.GRACE + 1))
        assertEquals(1_000_000 - Meetings.SOON, Meetings.nextCheck(listOf(e), 0))
        assertEquals(e, Meetings.next(listOf(allDay, e), 0))
        // Today's agenda: what's left of today, in order; not all-day events, not tomorrow's.
        val later = e.copy(id = 3, title = "Review", begin = 3_000_000, end = 4_000_000)
        val tomorrow = e.copy(id = 4, begin = 9_000_000, end = 9_500_000)
        val over = e.copy(id = 5, begin = 100, end = 500)
        assertEquals(listOf(e, later), Meetings.today(listOf(tomorrow, later, allDay, over, e), 1_000, 5_000_000))
        assertEquals(listOf(later), Meetings.today(listOf(e, later), 2_500_000, 5_000_000), "one that's over drops off")
    }

    @Test
    fun the_album_colour_skips_black_and_white() {
        val red = 0xffd02020.toInt()
        val pixels = IntArray(100) { if (it < 70) 0xff000000.toInt() else if (it < 85) 0xffffffff.toInt() else red }
        assertEquals(0xd02020, Colors.dominant(pixels))
        assertNull(Colors.dominant(IntArray(10) { 0xff808080.toInt() }), "a grey cover has no colour")
        assertNull(Colors.dominant(IntArray(0)))
        assertEquals(0x96 shl 16, Colors.forBorder(0x500000))
        assertEquals(0xd02020, Colors.forBorder(0xd02020))
    }

    @Test
    fun the_island_hugs_the_camera_and_grows_into_cards() {
        val g = Geometry(400.0, Camera(200.0, 20.0, 24.0, 24.0))
        val idle = g.box(Scene.Idle)
        assertEquals(200.0, idle.centerX)
        assertEquals(20.0, idle.y + idle.h / 2, "centred on the camera")
        assertTrue(idle.w > 24)
        val compact = g.box(Scene.Compact(music, call))
        assertTrue(compact.w > idle.w && compact.h == idle.h)
        val bubble = g.bubble(compact)
        assertTrue(bubble.x > compact.right && bubble.w == compact.h)
        val card = g.box(Scene.Expanded(music))
        assertTrue(card.w <= 400 - 12 && card.h > 150 && card.y == idle.y)
        val offCenter = Geometry(400.0, Camera(40.0, 20.0, 24.0, 24.0)).box(Scene.Expanded(music))
        assertTrue(offCenter.x >= 6.0, "a corner camera still keeps the card on screen")
        val none = Geometry(360.0, null).box(Scene.Idle)
        assertEquals(180.0, none.centerX)
        assertEquals(8.0, none.y)
        assertEquals(idle.lerp(card, 0.5).w, (idle.w + card.w) / 2)
    }

    @Test
    fun pop_ups_can_grow_down_clear_of_the_status_bar() {
        val cam = Camera(205.0, 18.0, 24.0, 24.0)
        val side = Geometry(411.0, cam)
        val down = Geometry(411.0, cam, popsDown = true)
        val idle = down.box(Scene.Idle)
        val charging = Scene.Popped(Pop.Charging(64, true))
        // Sideways, the pill spreads over the status bar's clock and icons.
        assertTrue(side.box(charging).w > 200)
        // Downward, it stays about as narrow as the camera's pill and grows down.
        val small = down.box(charging)
        assertTrue(small.w <= 160 && small.w >= idle.w, "narrow: ${small.w}")
        assertEquals(idle.y, small.y, "from the camera")
        assertEquals(idle.centerX, small.centerX)
        assertTrue(small.h > idle.h + 30, "taller, for the words under the camera")
        val level = down.box(Scene.Popped(Pop.Level(Pop.LevelKind.Volume, 0.5f)))
        assertTrue(level.w == small.w && level.h > idle.h && level.h < small.h)
        // Cards (a notification) open under the status bar, not over it.
        val note = down.box(Scene.Popped(Pop.Note("k", "Chat", "Sam", "Hi", emptyList(), 1)))
        assertTrue(note.y >= idle.bottom, "below the camera's pill: ${note.y} vs ${idle.bottom}")
        assertTrue(note.w > 300)
        assertEquals(side.box(Scene.Popped(Pop.Note("k", "Chat", "Sam", "Hi", emptyList(), 1))).h, note.h)
        // Nothing else changes.
        assertEquals(side.box(Scene.Compact(music, null)), down.box(Scene.Compact(music, null)))
        assertEquals(side.box(Scene.Expanded(music)), down.box(Scene.Expanded(music)))
    }

    @Test
    fun the_music_card_makes_room_for_lyrics_only_when_they_are_on() {
        val cam = Camera(200.0, 20.0, 24.0, 24.0)
        val plain = Geometry(400.0, cam).box(Scene.Expanded(music))
        val withLyrics = Geometry(400.0, cam, musicLyrics = true).box(Scene.Expanded(music))
        assertEquals(Geometry.MUSIC_CARD, plain.h, 0.001)
        assertEquals(Geometry.MUSIC_LYRICS, withLyrics.h - plain.h, 0.001, "two lines of lyrics")
        assertEquals(plain.w, withLyrics.w)
        assertEquals(plain.y, withLyrics.y, "it grows down from the same top")
        // Other cards don't change with it.
        assertEquals(Geometry(400.0, cam).box(Scene.Expanded(call)), Geometry(400.0, cam, musicLyrics = true).box(Scene.Expanded(call)))
    }

    @Test
    fun the_to_do_list_survives_being_stored() {
        val items = listOf(Todo("Buy milk", false), Todo("Essay\nmaybe", true))
        val back = Lists.decodeTodos(Lists.encodeTodos(items))
        assertEquals(listOf(Todo("Buy milk", false), Todo("Essay maybe", true)), back)
        assertEquals("Buy milk", Lists.nextTodo(back))
        assertEquals(listOf("a", "b"), Lists.decode(Lists.encode(listOf(" a ", "", "b"))))
    }
}
