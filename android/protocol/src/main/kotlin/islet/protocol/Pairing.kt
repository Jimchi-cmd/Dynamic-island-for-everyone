package islet.protocol

import java.net.URLDecoder
import java.util.Base64

/** What the computer's QR code says: where it is, and the pairing key. */
data class Pairing(val hosts: List<String>, val port: Int, val key: ByteArray, val name: String) {
    /** The same link again, for storing. */
    fun toUri(): String {
        val k = Base64.getUrlEncoder().withoutPadding().encodeToString(key)
        val n = java.net.URLEncoder.encode(name, "UTF-8").replace("+", "%20")
        return "islet://pair?h=${hosts.joinToString(",")}&p=$port&k=$k&n=$n"
    }

    override fun equals(other: Any?): Boolean =
        other is Pairing && hosts == other.hosts && port == other.port && key.contentEquals(other.key) && name == other.name

    override fun hashCode(): Int = hosts.hashCode() * 31 + port

    companion object {
        /** `islet://pair?h=192.168.1.20,10.0.0.5&p=47821&k=…&n=My%20laptop`, or null. */
        fun fromUri(uri: String): Pairing? {
            val query = uri.trim().removePrefix("islet://pair?")
            if (query == uri.trim()) return null
            var hosts = emptyList<String>()
            var port: Int? = null
            var key: ByteArray? = null
            var name = ""
            for (part in query.split('&')) {
                val at = part.indexOf('=')
                if (at < 0) return null
                val value = part.substring(at + 1)
                when (part.substring(0, at)) {
                    "h" -> hosts = value.split(',').filter { it.isNotEmpty() }
                    "p" -> port = value.toIntOrNull()?.takeIf { it in 1..65535 }
                    "k" -> key = try {
                        Base64.getUrlDecoder().decode(value).takeIf { it.size == KEY_LEN }
                    } catch (e: IllegalArgumentException) {
                        null
                    }
                    "n" -> name = try {
                        URLDecoder.decode(value.replace("+", "%2B"), "UTF-8")
                    } catch (e: IllegalArgumentException) {
                        value
                    }
                }
            }
            val plain = hosts.isNotEmpty() && hosts.all { h -> h.all { it.isLetterOrDigit() && it.code < 128 || it in ".:-" } }
            if (!plain) return null
            return Pairing(hosts, port ?: return null, key ?: return null, name)
        }
    }
}
