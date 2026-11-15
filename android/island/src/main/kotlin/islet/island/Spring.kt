package islet.island

import kotlin.math.abs
import kotlin.math.sqrt

/**
 * A spring that pulls `value` towards `target`, the motion the island
 * grows and shrinks with. `damping` is the ratio to critical damping:
 * below 1 it overshoots a little before it settles.
 */
class Spring(var value: Double = 0.0, var stiffness: Double = 600.0, var damping: Double = 0.82) {
    var target = value
    var velocity = 0.0
        private set

    /** Close enough that nothing on screen would move by a pixel more. */
    val settled get() = abs(target - value) < 0.002 && abs(velocity) < 0.02

    /** Jump straight to `v` (reduced motion, first frame). */
    fun snap(v: Double) {
        value = v
        target = v
        velocity = 0.0
    }

    /** Advance by `dt` seconds. Returns true once it has settled. */
    fun step(dt: Double): Boolean {
        var left = dt.coerceIn(0.0, 0.1)
        val c = 2 * damping * sqrt(stiffness)
        while (left > 0) {
            val h = minOf(left, 1.0 / 240)
            val a = -stiffness * (value - target) - c * velocity
            velocity += a * h
            value += velocity * h
            left -= h
        }
        if (settled) {
            value = target
            velocity = 0.0
            return true
        }
        return false
    }
}
