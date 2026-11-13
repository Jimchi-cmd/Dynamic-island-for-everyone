package islet.protocol

import java.io.DataInputStream
import java.net.ServerSocket
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

private fun hex(bytes: ByteArray) = bytes.joinToString("") { "%02x".format(it) }

class ProtocolTest {
    // The same values as crates/islet-core/src/phone.rs.
    private val key = ByteArray(32) { it.toByte() }
    private val server = ByteArray(16) { 0xA0.toByte() }
    private val client = ByteArray(16) { 0xB1.toByte() }

    @Test
    fun `the vectors match the computer's side`() {
        val session = sessionKey(key, server, client)
        assertEquals("2d4df88021832e07dedd23b231d0d9a846c014f644903c5bcfe8f959fa71f65e", hex(session))
        val frame = Channel(session, Side.PHONE).seal(FromPhone.PING.toByteArray())
        assertEquals("0000000000000001b158e8ab0848a57bcbb1eb1276826dc2777d0e8c47f8ea6d07569641", hex(frame))
    }

    @Test
    fun `hmac matches RFC 4231`() {
        assertEquals(
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
            hex(hmacSha256("Jefe".toByteArray(), "what do ya want for nothing?".toByteArray())),
        )
    }

    @Test
    fun `both ends agree and replays are refused`() {
        val session = sessionKey(key, server, client)
        val computer = Channel(session, Side.COMPUTER)
        val phone = Channel(session, Side.PHONE)
        val frame = phone.seal("one".toByteArray())
        assertContentEquals("one".toByteArray(), computer.open(frame))
        assertFailsWith<LinkException> { computer.open(frame) }
        val bent = phone.seal("two".toByteArray()).also { it[it.size - 1] = (it[it.size - 1].toInt() xor 1).toByte() }
        assertFailsWith<LinkException> { computer.open(bent) }
        val own = computer.seal("hi".toByteArray())
        assertFailsWith<LinkException> { computer.open(own) }
        assertContentEquals("hi".toByteArray(), phone.open(own))
    }

    @Test
    fun `the pairing link is read and refuses anything odd`() {
        val uri = "islet://pair?h=192.168.1.20,10.0.0.5&p=47821&k=AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8&n=Joram%27s%20laptop"
        val p = assertNotNull(Pairing.fromUri(uri))
        assertEquals(listOf("192.168.1.20", "10.0.0.5"), p.hosts)
        assertEquals(47821, p.port)
        assertContentEquals(key, p.key)
        assertEquals("Joram's laptop", p.name)
        assertEquals(p, Pairing.fromUri(p.toUri()))
        assertNull(Pairing.fromUri("https://example.com"))
        assertNull(Pairing.fromUri("islet://pair?h=a&p=1&k=short"))
        assertNull(Pairing.fromUri("islet://pair?h=bad host&p=1&k=AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"))
        assertNull(Pairing.fromUri("islet://pair?h=a&p=99999&k=AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"))
    }

    @Test
    fun `messages are the JSON the computer expects`() {
        assertEquals("""{"t":"battery","level":82,"charging":true}""", FromPhone.battery(82, true))
        assertEquals(
            """{"t":"note","app":"Messages","title":"Amina","text":"She said \"hi\"\nand left"}""",
            FromPhone.note("Messages", "Amina", "She said \"hi\"\nand left"),
        )
        assertEquals(
            """{"t":"media","title":"Song","artist":"Band","app":"Spotify","playing":true,"duration":200.000,"position":50.500}""",
            FromPhone.media("Song", "Band", "Spotify", true, 200.0, 50.5),
        )
        assertEquals(ToPhone.Command("next"), ToPhone.parse("""{"t":"command","do":"next"}"""))
        assertEquals(ToPhone.Ok("My \"laptop\""), ToPhone.parse("""{"t":"ok","name":"My \"laptop\""}"""))
        assertEquals(ToPhone.Pong, ToPhone.parse("""{ "t" : "pong" }"""))
        assertNull(ToPhone.parse("""{"t":"shell","cmd":"rm"}"""))
        assertNull(ToPhone.parse("not json"))
        assertEquals(mapOf("a" to "b"), flatObject("""{"n":12,"a":"b","ok":true}"""))
    }

    /** Plays the computer: one connection, as crates/islet/src/providers/phone.rs serves it. */
    private class FakeComputer(private val key: ByteArray) : AutoCloseable {
        val server = ServerSocket(0)
        val heard = LinkedBlockingQueue<String>()
        private var seal: Channel? = null
        private var out: java.io.OutputStream? = null

        fun serveOne() = Thread {
            server.accept().use { s ->
                val input = DataInputStream(s.getInputStream())
                val output = s.getOutputStream()
                val serverNonce = ByteArray(NONCE_LEN) { 5 }
                output.write(GREETING + serverNonce)
                val clientNonce = ByteArray(NONCE_LEN).also { input.readFully(it) }
                val session = sessionKey(key, serverNonce, clientNonce)
                val open = Channel(session, Side.COMPUTER)
                seal = Channel(session, Side.COMPUTER)
                out = output
                try {
                    while (true) {
                        val json = String(open.open(Link.readFrame(input)))
                        heard.put(json)
                        if (json.contains("\"hello\"")) say("""{"t":"ok","name":"Test laptop"}""")
                    }
                } catch (e: Exception) {
                    heard.put("closed: ${e.message}")
                }
            }
        }.apply { isDaemon = true; start() }

        fun say(json: String) = Link.writeFrame(out!!, seal!!.seal(json.toByteArray()))

        override fun close() = server.close()
    }

    @Test
    fun `the link connects, reports and takes commands`() {
        FakeComputer(key).use { computer ->
            computer.serveOne()
            val events = LinkedBlockingQueue<String>()
            val link = Link(
                Pairing(listOf("127.0.0.1"), computer.server.localPort, key, "Test laptop"),
                "Pixel 8",
                object : LinkListener {
                    override fun onConnected(computer: String) = events.put("connected to $computer")
                    override fun onDisconnected(reason: String) = events.put("disconnected")
                    override fun onCommand(action: String) = events.put("command $action")
                },
            )
            link.start()
            assertEquals("connected to Test laptop", events.poll(5, TimeUnit.SECONDS))
            assertEquals("""{"t":"hello","name":"Pixel 8"}""", computer.heard.poll(5, TimeUnit.SECONDS))
            assertTrue(link.send(FromPhone.battery(82, false)))
            assertEquals("""{"t":"battery","level":82,"charging":false}""", computer.heard.poll(5, TimeUnit.SECONDS))
            computer.say("""{"t":"command","do":"play_pause"}""")
            assertEquals("command play_pause", events.poll(5, TimeUnit.SECONDS))
            link.stop()
            assertTrue(!link.send(FromPhone.PING), "nothing is sent once stopped")
        }
    }

    @Test
    fun `a computer with another key is not trusted`() {
        FakeComputer(ByteArray(32) { 9 }).use { computer ->
            computer.serveOne()
            val events = LinkedBlockingQueue<String>()
            val link = Link(
                Pairing(listOf("127.0.0.1"), computer.server.localPort, key, "x"),
                "Pixel 8",
                object : LinkListener {
                    override fun onConnected(computer: String) = events.put("connected")
                    override fun onDisconnected(reason: String) = events.put("disconnected")
                    override fun onCommand(action: String) = events.put("command")
                },
            )
            link.start()
            assertEquals("disconnected", events.poll(8, TimeUnit.SECONDS))
            assertTrue(!link.connected)
            link.stop()
        }
    }
}
