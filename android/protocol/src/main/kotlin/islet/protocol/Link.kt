package islet.protocol

import java.io.DataInputStream
import java.io.OutputStream
import java.net.InetSocketAddress
import java.net.Socket
import java.security.SecureRandom

/** What the app does with the link. Called on the link's own threads. */
interface LinkListener {
    /** Connected and recognised: the computer's name. Send the current state now. */
    fun onConnected(computer: String)

    fun onDisconnected(reason: String)

    /** A button on the island's music card: "play_pause", "next" or "previous". */
    fun onCommand(action: String)
}

/**
 * Keeps a connection to the paired computer: dials each of its addresses,
 * proves it has the key, then stays connected, pinging every 30 seconds.
 * When the connection drops it tries again, more slowly each time.
 */
class Link(private val pairing: Pairing, private val phoneName: String, private val listener: LinkListener) {
    @Volatile private var running = false
    @Volatile private var socket: Socket? = null
    @Volatile private var sealing: Channel? = null
    private var out: OutputStream? = null
    private var thread: Thread? = null
    private val writeLock = Any()

    val connected: Boolean get() = sealing != null

    fun start() {
        if (running) return
        running = true
        thread = Thread({ loop() }, "islet-link").apply {
            isDaemon = true
            start()
        }
    }

    fun stop() {
        running = false
        closeQuietly()
        thread?.interrupt()
    }

    /** Try again now rather than after the wait (the network came back). */
    fun retryNow() {
        if (!connected) thread?.interrupt()
    }

    /** Send one message (a `FromPhone` JSON). False when not connected. */
    fun send(json: String): Boolean {
        synchronized(writeLock) {
            val channel = sealing ?: return false
            val stream = out ?: return false
            return try {
                writeFrame(stream, channel.seal(json.toByteArray(Charsets.UTF_8)))
                true
            } catch (e: Exception) {
                closeQuietly()
                false
            }
        }
    }

    private fun closeQuietly() {
        sealing = null
        try {
            socket?.close()
        } catch (e: Exception) {
            // Already gone.
        }
        socket = null
    }

    private fun loop() {
        var wait = FIRST_WAIT_MS
        while (running) {
            val reason = try {
                session()
            } catch (e: InterruptedException) {
                "interrupted"
            } catch (e: Exception) {
                e.message ?: e.javaClass.simpleName
            }
            val wasConnected = sealing != null
            closeQuietly()
            if (!running) break
            listener.onDisconnected(reason)
            // A connection that worked starts the wait from the beginning.
            wait = if (wasConnected) FIRST_WAIT_MS else (wait * 2).coerceAtMost(LONGEST_WAIT_MS)
            try {
                Thread.sleep(wait)
            } catch (e: InterruptedException) {
                // retryNow() or stop(): go round again.
            }
        }
    }

    /** One connection, from dialling to the end. Returns why it ended. */
    private fun session(): String {
        val s = dial() ?: return "the computer didn't answer"
        socket = s
        s.tcpNoDelay = true
        s.soTimeout = HANDSHAKE_MS
        val input = DataInputStream(s.getInputStream())
        val output = s.getOutputStream()

        val greeting = ByteArray(GREETING.size + NONCE_LEN)
        input.readFully(greeting)
        if (!greeting.copyOfRange(0, GREETING.size).contentEquals(GREETING)) return "that isn't Islet"
        val server = greeting.copyOfRange(GREETING.size, greeting.size)
        val client = ByteArray(NONCE_LEN).also { SecureRandom().nextBytes(it) }
        output.write(client)
        output.flush()
        val key = sessionKey(pairing.key, server, client)
        val seal = Channel(key, Side.PHONE)
        val open = Channel(key, Side.PHONE)

        writeFrame(output, seal.seal(FromPhone.hello(phoneName).toByteArray(Charsets.UTF_8)))
        val first = ToPhone.parse(String(open.open(readFrame(input)), Charsets.UTF_8))
        val computer = (first as? ToPhone.Ok)?.name ?: return "the computer didn't recognise this phone"
        synchronized(writeLock) {
            out = output
            sealing = seal
        }
        // The computer answers each ping; silence for three of them ends it.
        s.soTimeout = 3 * PING_MS + 5_000
        listener.onConnected(computer)

        val pinger = Thread({
            try {
                while (sealing === seal) {
                    Thread.sleep(PING_MS.toLong())
                    if (!send(FromPhone.PING)) break
                }
            } catch (e: InterruptedException) {
                // The connection ended.
            }
        }, "islet-ping").apply {
            isDaemon = true
            start()
        }
        try {
            while (running) {
                val json = String(open.open(readFrame(input)), Charsets.UTF_8)
                when (val msg = ToPhone.parse(json)) {
                    is ToPhone.Command -> listener.onCommand(msg.action)
                    else -> {} // pong, or something newer: nothing to do
                }
            }
        } finally {
            pinger.interrupt()
        }
        return "stopped"
    }

    private fun dial(): Socket? {
        for (host in pairing.hosts) {
            if (!running) return null
            val s = Socket()
            try {
                s.connect(InetSocketAddress(host, pairing.port), CONNECT_MS)
                return s
            } catch (e: Exception) {
                try {
                    s.close()
                } catch (ignored: Exception) {
                    // Nothing was open.
                }
            }
        }
        return null
    }

    companion object {
        const val PING_MS = 30_000
        private const val CONNECT_MS = 4_000
        private const val HANDSHAKE_MS = 8_000
        private const val FIRST_WAIT_MS = 2_000L
        private const val LONGEST_WAIT_MS = 60_000L

        fun writeFrame(out: OutputStream, frame: ByteArray) {
            val n = frame.size
            out.write(byteArrayOf((n ushr 24).toByte(), (n ushr 16).toByte(), (n ushr 8).toByte(), n.toByte()))
            out.write(frame)
            out.flush()
        }

        fun readFrame(input: DataInputStream): ByteArray {
            val n = input.readInt()
            if (n < 0 || n > MAX_FRAME) throw LinkException("a message too long to be ours")
            return ByteArray(n).also { input.readFully(it) }
        }
    }
}
