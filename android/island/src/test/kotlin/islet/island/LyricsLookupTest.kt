package islet.island

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class LyricsLookupTest {
    @Test
    fun titles_lose_what_players_and_video_sites_add() {
        assertEquals("Song", Lyrics.cleanTitle("Song (Official Music Video)"))
        assertEquals("Song", Lyrics.cleanTitle("Song [Lyrics] | Official Audio"))
        assertEquals("Song", Lyrics.cleanTitle("Song - Remastered 2011"))
        assertEquals("Song", Lyrics.cleanTitle("Song (feat. X) [Live]"))
        assertEquals("Live - Forever", Lyrics.cleanTitle("Live - Forever"), "only decorations go")
        assertEquals("Happy Birthday (Birthday Song)", Lyrics.cleanTitle("Happy Birthday (Birthday Song)"), "whole words only")
        assertEquals("Song (Without You)", Lyrics.cleanTitle("Song (Without You)"))
        assertEquals("Song (Part 2)", Lyrics.cleanTitle("Song (Part 2)"))
    }

    @Test
    fun artists_lose_channel_names_and_featured_artists() {
        assertEquals("The Band", Lyrics.cleanArtist("The Band - Topic"))
        assertEquals("TheBand", Lyrics.cleanArtist("TheBandVEVO"))
        assertEquals("Ann", Lyrics.cleanArtist("Ann, Bob & Cat"))
        assertEquals("Ann", Lyrics.cleanArtist("Ann feat. Bob"))
    }

    @Test
    fun the_exact_song_first_then_tidier_searches() {
        val urls = Lyrics.lrclibLookups("The Band - Song (Official Video)", "Some Channel", 200_000)
        assertEquals(
            listOf(
                "https://lrclib.net/api/get?track_name=The+Band+-+Song+%28Official+Video%29&artist_name=Some+Channel&duration=200",
                "https://lrclib.net/api/search?track_name=The+Band+-+Song&artist_name=Some+Channel",
                "https://lrclib.net/api/search?track_name=Song&artist_name=The+Band",
                "https://lrclib.net/api/search?q=Some+Channel+The+Band+Song",
            ),
            urls,
        )
    }

    @Test
    fun a_search_answer_gives_the_closest_in_length_with_synced_lyrics() {
        val json = """[
            {"duration": 230, "syncedLyrics": "[00:01.00]too long"},
            {"duration": 201, "syncedLyrics": null, "plainLyrics": "no times"},
            {"duration": 199.5, "syncedLyrics": "[00:02.00]right one"}
        ]"""
        assertEquals("right one", Lyrics.fromSearch(json, 200_000)?.lines?.first()?.second)
        assertNull(Lyrics.fromSearch("""[{"duration": 260, "syncedLyrics": "[00:01.00]x"}]""", 200_000), "nothing within 3 s")
        assertNull(Lyrics.fromSearch("{}", 200_000))
    }
}
