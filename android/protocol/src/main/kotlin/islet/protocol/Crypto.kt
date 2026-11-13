package islet.protocol

import java.nio.ByteBuffer
import javax.crypto.Cipher
import javax.crypto.Mac
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

// The link to the computer, as crates/islet-core/src/phone.rs describes it:
// a key for each connection from the pairing key and both sides' random
// bytes (HMAC-SHA-256), then AES-256-GCM with a counter that only goes up.

const val KEY_LEN = 32
const val NONCE_LEN = 16
const val MAX_FRAME = 64 * 1024
val GREETING: ByteArray = "ISLET1".toByteArray(Charsets.US_ASCII)
private val LABEL: ByteArray = "islet-phone-v1".toByteArray(Charsets.US_ASCII)

fun hmacSha256(key: ByteArray, data: ByteArray): ByteArray {
    val mac = Mac.getInstance("HmacSHA256")
    mac.init(SecretKeySpec(key, "HmacSHA256"))
    return mac.doFinal(data)
}

/** The key for one connection. */
fun sessionKey(pairingKey: ByteArray, server: ByteArray, client: ByteArray): ByteArray {
    require(pairingKey.size == KEY_LEN && server.size == NONCE_LEN && client.size == NONCE_LEN)
    return hmacSha256(pairingKey, LABEL + server + client)
}

enum class Side(val id: Byte) {
    COMPUTER(1),
    PHONE(2),
}

class LinkException(message: String) : Exception(message)

/** One end of a connection, once both sides have the session key. */
class Channel(sessionKey: ByteArray, private val side: Side) {
    private val key = SecretKeySpec(sessionKey, "AES")
    private var sent = 0L
    private var received = 0L

    private fun nonce(side: Side, counter: Long): ByteArray {
        val n = ByteArray(12)
        n[0] = side.id
        ByteBuffer.wrap(n, 4, 8).putLong(counter)
        return n
    }

    /** Seal a message: its counter (8 bytes), then the ciphertext and tag. */
    @Synchronized
    fun seal(plain: ByteArray): ByteArray {
        sent += 1
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key, GCMParameterSpec(128, nonce(side, sent)))
        return ByteBuffer.allocate(8).putLong(sent).array() + cipher.doFinal(plain)
    }

    /** Open the other side's message; refuses a wrong key, a changed byte or a replay. */
    @Synchronized
    fun open(frame: ByteArray): ByteArray {
        if (frame.size < 8 + 16) throw LinkException("too short")
        val counter = ByteBuffer.wrap(frame, 0, 8).long
        if (counter <= received) throw LinkException("an old message, sent again")
        val from = if (side == Side.COMPUTER) Side.PHONE else Side.COMPUTER
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, nonce(from, counter)))
        val plain = try {
            cipher.doFinal(frame, 8, frame.size - 8)
        } catch (e: java.security.GeneralSecurityException) {
            throw LinkException("not sealed with our key")
        }
        received = counter
        return plain
    }
}
