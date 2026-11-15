package io.github.jimchi_cmd.islet

import android.graphics.Bitmap
import android.media.MediaMetadata
import android.media.session.PlaybackState
import islet.island.Live
import islet.island.Scene
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** What the island does with a player's session: covers, pausing, lyrics. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class MusicTest {
    private val ctx = RuntimeEnvironment.getApplication()

    private class Player(var meta: MediaMetadata, var state: PlaybackState) {
        fun release() = Unit
    }

    private fun send(p: Player) = Isle.song("com.example.player", p.meta, p.state)

    private fun session(playing: Boolean, art: Bitmap? = null): Player {
        val meta = MediaMetadata.Builder()
            .putString(MediaMetadata.METADATA_KEY_TITLE, "Already Rich")
            .putString(MediaMetadata.METADATA_KEY_ARTIST, "Example")
            .putLong(MediaMetadata.METADATA_KEY_DURATION, 180_000)
        if (art != null) meta.putBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART, art)
        return Player(meta.build(), playState(playing))
    }

    private fun playState(playing: Boolean) = PlaybackState.Builder()
        .setState(if (playing) PlaybackState.STATE_PLAYING else PlaybackState.STATE_PAUSED, 10_000, 1f)
        .build()

    private fun state(p: Player, playing: Boolean) {
        p.state = playState(playing)
    }

    private fun fresh() {
        Isle.init(ctx)
        Isle.island.all.forEach { Isle.island.remove(it.id) }
        Isle.island.close(0)
        Isle.prefs.lyrics = false
        Isle.clock = { System.currentTimeMillis() }
        Isle.media(null)
    }

    @Test
    fun paused_music_leaves_after_a_while_and_the_time_comes_back() {
        fresh()
        val s = session(playing = true)
        send(s)
        assertTrue(Isle.island.scene(Isle.now()) is Scene.Compact)
        state(s, false)
        send(s)
        assertEquals(false, (Isle.island.get("music") as Live.Music).playing)
        val later = Isle.now() + Isle.PAUSE_LINGER + 1000
        Isle.clock = { later }
        Isle.changed()
        assertNull("gone after the linger", Isle.island.get("music"))
        assertEquals(Scene.Idle, Isle.island.scene(Isle.now()))
        // Still paused on the next update: it doesn't come back.
        send(s)
        assertNull(Isle.island.get("music"))
        // Playing again: back it comes.
        state(s, true)
        send(s)
        assertNotNull(Isle.island.get("music"))
        s.release()
    }

    @Test
    fun music_already_paused_when_first_seen_stays_off_the_island() {
        fresh()
        val s = session(playing = false)
        send(s)
        assertNull(Isle.island.get("music"))
        s.release()
    }

    @Test
    fun the_cover_and_its_colour_come_from_the_song() {
        fresh()
        val red = Bitmap.createBitmap(64, 64, Bitmap.Config.ARGB_8888).apply { eraseColor(0xffd02040.toInt()) }
        val s = session(playing = true, art = red)
        send(s)
        assertNotNull(Isle.art)
        assertNotNull("the border takes the cover's colour", Isle.artColor)
        s.release()
    }

    @Test
    fun the_lyrics_button_switches_lyrics_on_and_off() {
        fresh()
        Isle.toggleLyrics()
        assertTrue(Isle.prefs.lyrics)
        Isle.toggleLyrics()
        assertEquals(false, Isle.prefs.lyrics)
    }
}
