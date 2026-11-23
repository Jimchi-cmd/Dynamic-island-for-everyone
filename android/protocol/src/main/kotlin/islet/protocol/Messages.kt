package islet.protocol

// The messages are small, flat JSON objects. Written and read by hand, so
// this module needs no library and runs the same on Android and in tests.

/** A JSON string, quoted and escaped. */
fun jsonString(s: String): String {
    val out = StringBuilder(s.length + 2).append('"')
    for (c in s) {
        when {
            c == '"' -> out.append("\\\"")
            c == '\\' -> out.append("\\\\")
            c == '\n' -> out.append("\\n")
            c == '\r' -> out.append("\\r")
            c == '\t' -> out.append("\\t")
            c.code < 0x20 -> out.append(String.format("\\u%04x", c.code))
            else -> out.append(c)
        }
    }
    return out.append('"').toString()
}

/** What the phone tells the computer. */
object FromPhone {
    fun hello(name: String) = """{"t":"hello","name":${jsonString(name)}}"""

    fun battery(level: Int, charging: Boolean) = """{"t":"battery","level":${level.coerceIn(0, 100)},"charging":$charging}"""

    /** `id` names it for a reply; `reply`: the computer may answer it. */
    fun note(app: String, title: String, text: String, id: String = "", reply: Boolean = false) =
        """{"t":"note","app":${jsonString(app)},"title":${jsonString(title)},"text":${jsonString(text)},""" +
            """"id":${jsonString(id)},"reply":$reply}"""

    /** Text or a link from the phone's Share menu, for the computer's clipboard. */
    fun share(text: String) = """{"t":"share","text":${jsonString(text.take(MAX_TEXT))}}"""

    /** `title` empty: nothing is playing. Times in seconds (0: unknown). */
    fun media(title: String, artist: String, app: String, playing: Boolean, duration: Double, position: Double) =
        """{"t":"media","title":${jsonString(title)},"artist":${jsonString(artist)},"app":${jsonString(app)},""" +
            """"playing":$playing,"duration":${number(duration)},"position":${number(position)}}"""

    const val PING = """{"t":"ping"}"""

    private fun number(v: Double) = if (v.isFinite() && v > 0) String.format(java.util.Locale.ROOT, "%.3f", v) else "0"
}

/** The longest text sent either way (characters). */
const val MAX_TEXT = 4000

/** What the computer tells the phone. */
sealed class ToPhone {
    data class Ok(val name: String) : ToPhone()

    /** "play_pause", "next" or "previous". */
    data class Command(val action: String) : ToPhone()

    object Pong : ToPhone()

    /** Answer the notification `id` (from `FromPhone.note`). */
    data class Reply(val id: String, val text: String) : ToPhone()

    /** Ring loudly until found. */
    object Ring : ToPhone()

    /** Text or a link for the phone's clipboard. */
    data class Send(val text: String) : ToPhone()

    companion object {
        /** Null for anything this version doesn't know. */
        fun parse(json: String): ToPhone? {
            val fields = flatObject(json) ?: return null
            return when (fields["t"]) {
                "ok" -> Ok(fields["name"] ?: "")
                "command" -> fields["do"]?.let { Command(it) }
                "pong" -> Pong
                "reply" -> Reply(fields["id"] ?: return null, (fields["text"] ?: return null).take(MAX_TEXT))
                "ring" -> Ring
                "send" -> Send((fields["text"] ?: return null).take(MAX_TEXT))
                else -> null
            }
        }
    }
}

/** The string fields of a flat JSON object (other values are skipped). */
fun flatObject(json: String): Map<String, String>? {
    val s = json.trim()
    if (s.length < 2 || s.first() != '{' || s.last() != '}') return null
    val out = HashMap<String, String>()
    var i = 1
    fun skipSpace() {
        while (i < s.length && s[i].isWhitespace()) i++
    }
    fun string(): String? {
        if (i >= s.length || s[i] != '"') return null
        i++
        val b = StringBuilder()
        while (i < s.length && s[i] != '"') {
            var c = s[i++]
            if (c == '\\') {
                if (i >= s.length) return null
                c = when (val e = s[i++]) {
                    'n' -> '\n'
                    'r' -> '\r'
                    't' -> '\t'
                    'b' -> '\b'
                    'f' -> '\u000c'
                    'u' -> {
                        if (i + 4 > s.length) return null
                        val code = s.substring(i, i + 4).toIntOrNull(16) ?: return null
                        i += 4
                        code.toChar()
                    }
                    else -> e
                }
            }
            b.append(c)
        }
        if (i >= s.length) return null
        i++
        return b.toString()
    }
    while (true) {
        skipSpace()
        if (i >= s.length) return null
        if (s[i] == '}') return out
        val key = string() ?: return null
        skipSpace()
        if (i >= s.length || s[i] != ':') return null
        i++
        skipSpace()
        if (i < s.length && s[i] == '"') {
            out[key] = string() ?: return null
        } else {
            // A number, true, false or null: not needed, skipped.
            val start = i
            while (i < s.length && s[i] != ',' && s[i] != '}') i++
            if (i == start || s.substring(start, i).any { it == '{' || it == '[' }) return null
        }
        skipSpace()
        if (i < s.length && s[i] == ',') i++
    }
}
