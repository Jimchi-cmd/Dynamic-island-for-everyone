package io.github.jimchi_cmd.islet

import android.app.Notification
import android.app.PendingIntent
import android.app.RemoteInput
import android.content.ClipboardManager
import android.content.Intent
import islet.island.Pop
import islet.island.Scene
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowToast

/** The phone's side of working with the computer: text in, replies out, sharing. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class TogetherTest {
    private val ctx = RuntimeEnvironment.getApplication()

    private fun popText(): String? = (Isle.island.scene(Isle.now()) as? Scene.Popped)?.let { (it.pop as? Pop.Info)?.text }

    @Test
    fun text_from_the_computer_lands_on_the_clipboard() {
        Isle.init(ctx)
        Isle.island.dismissPop(0)
        Together.received(ctx, "Work laptop", "Room 12B, second floor")
        val clip = ctx.getSystemService(ClipboardManager::class.java).primaryClip
        assertEquals("Room 12B, second floor", clip?.getItemAt(0)?.text?.toString())
        org.robolectric.shadows.ShadowLooper.idleMainLooper()
        assertEquals("Copied from Work laptop", popText())
        assertEquals("https://example.com/a", Together.linkIn(" https://example.com/a "))
        assertNull(Together.linkIn("http://insecure.example"))
        assertNull(Together.linkIn("see https://example.com"))
    }

    @Test
    fun a_message_with_a_reply_button_is_answered_through_it() {
        val sent = Intent("islet.test.REPLY")
        val pending = PendingIntent.getBroadcast(ctx, 0, sent, PendingIntent.FLAG_MUTABLE)
        val input = RemoteInput.Builder("reply_text").setLabel("Reply").build()
        val reply = Notification.Action.Builder(null, "Reply", pending).addRemoteInput(input).build()
        val read = Notification.Action.Builder(null, "Mark as read", pending).build()
        val n = Notification.Builder(ctx, "chat").setContentTitle("Sam").setActions(read, reply).build()
        val found = Together.replyAction(n)
        assertNotNull("the button that takes text", found)
        assertEquals("Reply", found!!.title)
        assertNull("no reply button", Together.replyAction(Notification.Builder(ctx, "chat").setActions(read).build()))
        assertTrue(Together.answer(ctx, found, "On my way"))
        assertFalse("nothing to send", Together.answer(ctx, found, "  "))
        val broadcast = shadowOf(ctx).broadcastIntents.last()
        assertEquals("On my way", RemoteInput.getResultsFromIntent(broadcast)?.getCharSequence("reply_text")?.toString())
    }

    @Test
    fun ringing_can_always_be_stopped() {
        Together.stopRinging(ctx)
        assertFalse(Together.ringing)
    }

    @Test
    fun sharing_without_a_computer_says_so() {
        Isle.init(ctx)
        Hub.computer = null
        val share = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, "https://example.com")
        Robolectric.buildActivity(ShareActivity::class.java, share).setup()
        assertTrue(ShadowToast.getTextOfLatestToast().startsWith("Your computer isn't connected"))
    }
}
