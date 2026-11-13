package io.github.jimchi_cmd.islet

import android.content.Context
import islet.protocol.Pairing

/** What the app remembers: the paired computer and two switches. Kept in
 *  the app's private storage, which other apps can't read and which is
 *  left out of backups. */
object Store {
    private const val FILE = "islet"
    private const val PAIRING = "pairing"
    private const val NOTES = "send_notes"
    private const val MEDIA = "send_media"

    private fun prefs(context: Context) = context.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    fun pairing(context: Context): Pairing? = prefs(context).getString(PAIRING, null)?.let { Pairing.fromUri(it) }

    fun pair(context: Context, pairing: Pairing) {
        prefs(context).edit().putString(PAIRING, pairing.toUri()).apply()
    }

    fun forget(context: Context) {
        prefs(context).edit().remove(PAIRING).apply()
    }

    fun sendNotes(context: Context) = prefs(context).getBoolean(NOTES, true)

    fun sendMedia(context: Context) = prefs(context).getBoolean(MEDIA, true)

    fun setSendNotes(context: Context, on: Boolean) = prefs(context).edit().putBoolean(NOTES, on).apply()

    fun setSendMedia(context: Context, on: Boolean) = prefs(context).edit().putBoolean(MEDIA, on).apply()
}

/** What the screen shows about the link; the listener service updates it. */
object Hub {
    /** The computer's name while connected. */
    @Volatile var computer: String? = null

    /** Why the last attempt failed, in the link's words. */
    @Volatile var problem: String = ""

    /** The screen, while it is showing. */
    @Volatile var onChange: (() -> Unit)? = null

    fun changed() {
        onChange?.invoke()
    }
}
