package islet.island

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class CoversTest {
    @Test
    fun covers_are_read_at_about_the_size_shown() {
        assertEquals(16, Covers.sampleSize(3000, 3000, 160), "3000 px read as 187")
        assertEquals(1, Covers.sampleSize(300, 300, 160), "small ones whole")
        assertEquals(2, Covers.sampleSize(640, 400, 160))
        assertEquals(1, Covers.sampleSize(0, 0, 160), "unknown size: whole")
    }

    @Test
    fun a_notification_cover_matches_the_song_worded_a_little_differently() {
        assertTrue(Covers.sameSong("Midnight Drive", "midnight drive "))
        assertTrue(Covers.sameSong("Midnight Drive", "Midnight Drive (Remastered)"))
        assertFalse(Covers.sameSong("Midnight Drive", "Morning Run"))
        assertFalse(Covers.sameSong("", "Song"))
        assertFalse(Covers.sameSong("A", "A Different Song"), "too short to be sure")
    }
}
