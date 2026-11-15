package islet.island

/** A rounded box in dp, from the screen's top left. */
data class Box(val x: Double, val y: Double, val w: Double, val h: Double, val radius: Double) {
    val right get() = x + w
    val bottom get() = y + h
    val centerX get() = x + w / 2

    fun contains(px: Double, py: Double) = px >= x && px <= right && py >= y && py <= bottom

    /** The smallest box holding both (for the window during a change). */
    fun union(o: Box) = Box(minOf(x, o.x), minOf(y, o.y), maxOf(right, o.right) - minOf(x, o.x), maxOf(bottom, o.bottom) - minOf(y, o.y), radius)

    /** Part way from this box to `o` (t 0..1, a little beyond while a spring overshoots). */
    fun lerp(o: Box, t: Double): Box {
        fun m(a: Double, b: Double) = a + (b - a) * t
        return Box(m(x, o.x), m(y, o.y), maxOf(1.0, m(w, o.w)), maxOf(1.0, m(h, o.h)), maxOf(0.0, m(radius, o.radius)))
    }
}

enum class IslandSize(val scale: Double) { Small(0.9), Medium(1.0), Large(1.12) }

/** Where the front camera is (dp), from the display's cutout; null on a phone without one. */
data class Camera(val centerX: Double, val centerY: Double, val width: Double, val height: Double)

/** The island's size and place for each scene. */
class Geometry(
    val screenWidth: Double,
    val camera: Camera?,
    val size: IslandSize = IslandSize.Medium,
    val offsetX: Double = 0.0,
    val offsetY: Double = 0.0,
    /** Extra width round the camera, for phones where the pill should hide a wider cutout. */
    val extraWidth: Double = 0.0,
) {
    private val k get() = size.scale

    /** The empty pill's height: comfortably taller than the camera hole, which sits in its middle. */
    val idleHeight get() = maxOf(32.0, (camera?.height ?: 0.0) + 12) * k

    val idleWidth get() = maxOf(100.0, (camera?.width ?: 0.0) + 64) * k + extraWidth

    private val centerX get() = (camera?.centerX ?: (screenWidth / 2)) + offsetX

    /** The pill's top edge. */
    val top get() = (camera?.let { it.centerY - idleHeight / 2 } ?: 8.0).coerceAtLeast(2.0) + offsetY

    private fun centered(w: Double, h: Double, r: Double): Box {
        val width = minOf(w, screenWidth - 12)
        val x = (centerX - width / 2).coerceIn(6.0, maxOf(6.0, screenWidth - 6 - width))
        return Box(x, top, width, h, minOf(r, h / 2))
    }

    /** Width for content beside the camera on each side. */
    val wing get() = 62.0 * k

    private val cardWidth get() = minOf(screenWidth - 16, 412.0)

    fun box(scene: Scene): Box = when (scene) {
        Scene.Hidden -> centered(idleWidth * 0.6, idleHeight * 0.6, idleHeight)
        Scene.Idle -> centered(idleWidth, idleHeight, idleHeight / 2)
        is Scene.Compact -> centered(idleWidth + 2 * wing, idleHeight, idleHeight / 2)
        is Scene.Popped -> when (scene.pop) {
            is Pop.Note -> centered(cardWidth - 8, (if (scene.pop.actions.isEmpty()) 82.0 else 112.0) * k, 36.0 * k)
            is Pop.Screenshot, is Pop.TimerDone, is Pop.Bluetooth ->
                centered(cardWidth - 12, 78.0 * k, 34.0 * k)
            else -> centered(idleWidth + 2 * wing + 30 * k, idleHeight + 6 * k, (idleHeight + 6 * k) / 2)
        }
        is Scene.Expanded -> centered(cardWidth, expandedHeight(scene.live) * k, 40.0 * k)
        Scene.Controls -> centered(cardWidth, 252.0 * k, 40.0 * k)
        Scene.Peek -> centered(idleWidth + 2 * (wing + 30 * k), idleHeight + 4 * k, (idleHeight + 4 * k) / 2)
        Scene.Calendar -> centered(cardWidth, 300.0 * k, 40.0 * k)
    }

    private fun expandedHeight(live: Live?): Double = when (live) {
        is Live.Music -> 176.0
        is Live.Call -> 120.0
        is Live.Navigation -> 120.0
        is Live.Stopwatch -> 140.0
        null -> 216.0
        else -> 128.0
    }

    /** The second activity's round bubble, right of the pill. */
    fun bubble(pill: Box): Box {
        val d = pill.h
        val x = minOf(pill.right + 6, screenWidth - 6 - d)
        return Box(x, pill.y, d, d, d / 2)
    }
}
