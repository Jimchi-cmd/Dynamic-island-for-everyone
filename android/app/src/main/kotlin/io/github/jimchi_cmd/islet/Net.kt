package io.github.jimchi_cmd.islet

import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

/**
 * The few web requests the island makes, each only for something the user
 * switched on (weather for a place they chose, lyrics, a speed test).
 * Never call on the main thread.
 */
object Net {
    private const val AGENT = "Islet (https://github.com/Jimchi-cmd/Dynamic-island-for-everyone)"

    fun get(url: String, limit: Int = 512 * 1024): String? = try {
        val c = URL(url).openConnection() as HttpURLConnection
        c.connectTimeout = 8000
        c.readTimeout = 10_000
        c.setRequestProperty("User-Agent", AGENT)
        c.setRequestProperty("Accept", "application/json")
        try {
            if (c.responseCode !in 200..299) {
                null
            } else {
                c.inputStream.use { s ->
                    val bytes = s.readNBytesCompat(limit)
                    String(bytes, Charsets.UTF_8)
                }
            }
        } finally {
            c.disconnect()
        }
    } catch (e: IOException) {
        null
    } catch (e: IllegalArgumentException) {
        null
    }

    private fun java.io.InputStream.readNBytesCompat(limit: Int): ByteArray {
        val out = java.io.ByteArrayOutputStream()
        val buf = ByteArray(8192)
        while (out.size() < limit) {
            val n = read(buf, 0, minOf(buf.size, limit - out.size()))
            if (n < 0) break
            out.write(buf, 0, n)
        }
        return out.toByteArray()
    }

    data class Speed(val downMbps: Double, val upMbps: Double, val latencyMs: Double)

    /**
     * Cloudflare's public speed-test servers: about 25 MB down and 10 MB
     * up. Only when the user presses the button.
     */
    fun speedTest(): Speed? = try {
        val base = "https://speed.cloudflare.com"
        val pings = (1..4).map {
            val t = System.nanoTime()
            val c = URL("$base/__down?bytes=0").openConnection() as HttpURLConnection
            c.setRequestProperty("User-Agent", AGENT)
            c.inputStream.use { s -> s.readBytes() }
            c.disconnect()
            (System.nanoTime() - t) / 1e6
        }
        val down = run {
            val bytes = 25_000_000
            val t = System.nanoTime()
            val c = URL("$base/__down?bytes=$bytes").openConnection() as HttpURLConnection
            c.setRequestProperty("User-Agent", AGENT)
            var got = 0L
            c.inputStream.use { s ->
                val buf = ByteArray(64 * 1024)
                while (true) {
                    val n = s.read(buf)
                    if (n < 0) break
                    got += n
                }
            }
            c.disconnect()
            got * 8 / ((System.nanoTime() - t) / 1e9) / 1e6
        }
        val up = run {
            val body = ByteArray(10_000_000)
            val t = System.nanoTime()
            val c = URL("$base/__up").openConnection() as HttpURLConnection
            c.requestMethod = "POST"
            c.doOutput = true
            c.setFixedLengthStreamingMode(body.size)
            c.setRequestProperty("User-Agent", AGENT)
            c.setRequestProperty("Content-Type", "application/octet-stream")
            c.outputStream.use { it.write(body) }
            c.inputStream.use { it.readBytes() }
            c.disconnect()
            body.size * 8 / ((System.nanoTime() - t) / 1e9) / 1e6
        }
        Speed(down, up, pings.drop(1).minOrNull() ?: pings.first())
    } catch (e: IOException) {
        null
    }
}
