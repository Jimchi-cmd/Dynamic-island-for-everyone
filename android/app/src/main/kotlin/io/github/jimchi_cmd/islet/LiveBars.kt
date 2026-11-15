package io.github.jimchi_cmd.islet

import android.Manifest
import android.content.pm.PackageManager
import android.media.audiofx.Visualizer
import android.os.SystemClock
import islet.island.Bars
import islet.island.Live

/**
 * Bars that follow the music: the loudness of what the phone is playing,
 * from Android's Visualizer on the output mix. Android asks for the
 * microphone permission for this, but the microphone isn't used and
 * nothing is recorded or kept: only four levels, many times a second.
 * Runs only while music plays, the screen is on and the switch is on.
 */
object LiveBars {
    private var viz: Visualizer? = null

    /** Android said no (an old phone or a locked-down one): fall back to the drawn bars. */
    var failed = false
        private set

    fun allowed(): Boolean = Isle.app.checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    /** Start or stop to match what's going on. Call after anything changes. */
    fun update() {
        val music = Isle.island.get("music") as? Live.Music
        val want = Isle.prefs.liveBars && music?.playing == true && Isle.screenOn && !failed && allowed()
        if (want && viz == null) start() else if (!want && viz != null) stop()
    }

    /** Fresh levels to draw, or null to draw the plain moving bars. */
    fun levels(): FloatArray? =
        if (viz != null && SystemClock.elapsedRealtime() - Isle.levelsAt < 500) Isle.levels else null

    private fun start() {
        try {
            val v = Visualizer(0)
            v.enabled = false
            val range = Visualizer.getCaptureSizeRange()
            v.captureSize = maxOf(range[0], minOf(256, range[1]))
            v.setDataCaptureListener(
                object : Visualizer.OnDataCaptureListener {
                    override fun onWaveFormDataCapture(v: Visualizer?, waveform: ByteArray?, rate: Int) = Unit

                    override fun onFftDataCapture(v: Visualizer?, fft: ByteArray?, rate: Int) {
                        if (fft == null) return
                        Isle.levels = Bars.smooth(Isle.levels, Bars.levels(fft))
                        Isle.levelsAt = SystemClock.elapsedRealtime()
                    }
                },
                minOf(Visualizer.getMaxCaptureRate(), 20_000),
                false,
                true,
            )
            v.enabled = true
            viz = v
        } catch (e: Exception) {
            // Some phones don't allow the output mix to be read: keep the plain bars.
            failed = true
            stop()
        } catch (e: UnsatisfiedLinkError) {
            failed = true
            stop()
        }
    }

    fun stop() {
        try {
            viz?.enabled = false
            viz?.release()
        } catch (e: Exception) {
            // Already gone.
        }
        viz = null
        Isle.levels = FloatArray(4)
    }

    /** The switch was turned on again: try once more. */
    fun retry() {
        failed = false
        update()
    }
}
