package islet.island

/**
 * A small JSON reader for the few web answers the island reads (weather,
 * places, lyrics). Objects become maps, arrays lists, numbers doubles.
 * Null for text that isn't JSON.
 */
object Json {
    fun parse(text: String): Any? {
        val p = Parser(text)
        return try {
            val v = p.value()
            p.space()
            if (p.i != text.length) null else v
        } catch (e: IllegalStateException) {
            null
        }
    }

    private class Parser(val s: String) {
        var i = 0

        fun space() {
            while (i < s.length && s[i].isWhitespace()) i++
        }

        fun fail(): Nothing = throw IllegalStateException("bad JSON at $i")

        fun value(): Any? {
            space()
            if (i >= s.length) fail()
            return when (s[i]) {
                '{' -> obj()
                '[' -> arr()
                '"' -> str()
                't' -> word("true", true)
                'f' -> word("false", false)
                'n' -> word("null", null)
                else -> num()
            }
        }

        fun word(w: String, v: Any?): Any? {
            if (!s.startsWith(w, i)) fail()
            i += w.length
            return v
        }

        fun num(): Double {
            val start = i
            while (i < s.length && (s[i].isDigit() || s[i] in "+-.eE")) i++
            return s.substring(start, i).toDoubleOrNull() ?: fail()
        }

        fun str(): String {
            i++
            val b = StringBuilder()
            while (true) {
                if (i >= s.length) fail()
                val c = s[i++]
                when (c) {
                    '"' -> return b.toString()
                    '\\' -> {
                        if (i >= s.length) fail()
                        when (val e = s[i++]) {
                            'n' -> b.append('\n')
                            'r' -> b.append('\r')
                            't' -> b.append('\t')
                            'b' -> b.append('\b')
                            'f' -> b.append('\u000c')
                            'u' -> {
                                if (i + 4 > s.length) fail()
                                b.append(s.substring(i, i + 4).toIntOrNull(16)?.toChar() ?: fail())
                                i += 4
                            }
                            else -> b.append(e)
                        }
                    }
                    else -> b.append(c)
                }
            }
        }

        fun arr(): List<Any?> {
            i++
            val out = ArrayList<Any?>()
            space()
            if (i < s.length && s[i] == ']') {
                i++
                return out
            }
            while (true) {
                out.add(value())
                space()
                if (i >= s.length) fail()
                when (s[i++]) {
                    ',' -> continue
                    ']' -> return out
                    else -> fail()
                }
            }
        }

        fun obj(): Map<String, Any?> {
            i++
            val out = LinkedHashMap<String, Any?>()
            space()
            if (i < s.length && s[i] == '}') {
                i++
                return out
            }
            while (true) {
                space()
                if (i >= s.length || s[i] != '"') fail()
                val k = str()
                space()
                if (i >= s.length || s[i] != ':') fail()
                i++
                out[k] = value()
                space()
                if (i >= s.length) fail()
                when (s[i++]) {
                    ',' -> continue
                    '}' -> return out
                    else -> fail()
                }
            }
        }
    }
}

/** `obj["a"]["b"]` for parsed JSON, or null. */
@Suppress("UNCHECKED_CAST")
fun Any?.at(vararg path: Any): Any? {
    var v: Any? = this
    for (k in path) {
        v = when {
            k is String && v is Map<*, *> -> (v as Map<String, Any?>)[k]
            k is Int && v is List<*> -> v.getOrNull(k)
            else -> return null
        }
    }
    return v
}

/** A JSON string for a request body. */
fun quote(s: String): String {
    val out = StringBuilder(s.length + 2).append('"')
    for (c in s) {
        when {
            c == '"' -> out.append("\\\"")
            c == '\\' -> out.append("\\\\")
            c == '\n' -> out.append("\\n")
            c.code < 0x20 -> out.append(String.format("\\u%04x", c.code))
            else -> out.append(c)
        }
    }
    return out.append('"').toString()
}
