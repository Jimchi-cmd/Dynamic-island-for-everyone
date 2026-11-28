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
    /** The empty island shows the time (left of the camera) and the battery (right), as the computer's does. */
    val idleClock: Boolean = false,
    /** The song has synced lyrics showing: the small music pill grows a row for the line being sung. */
    val lyricsRow: Boolean = false,
    /** Extra room each side of the camera for a longer clock ("Wed 14:05", seconds), dp. */
    val clockExtra: Double = 0.0,
    /**
     * Pop-ups grow down from the camera instead of out to the sides, so the
     * status bar's clock and icons either side of it stay in view: small
     * ones keep a narrow pill with their content under the camera, and
     * cards (notifications) drop below the status bar.
     */
    val popsDown: Boolean = false,
    /** Lyrics are switched on: the music card has room under its controls for two lines. */
    val musicLyrics: Boolean = false,
    /** Control Centre's height in its layout (see `ControlCentre.height`), dp. */
    val controlsHeight: Double = CLOCK_CARD,
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

    /** Room each side of the camera for the empty island's time and battery. */
    val clockWing get() = 40.0 * k

    /** Width for content beside the camera on each side. */
    val wing get() = 62.0 * k

    private val cardWidth get() = minOf(screenWidth - 16, 412.0)

    fun box(scene: Scene): Box = when (scene) {
        Scene.Hidden -> centered(idleWidth * 0.6, idleHeight * 0.6, idleHeight)
        // Under the status bar, the time is the status bar's own: the pill stays round the camera.
        Scene.Idle -> centered(idleWidth + if (idleClock && !popsDown) 2 * (clockWing + clockExtra * k) else 0.0, idleHeight, idleHeight / 2)
        is Scene.Compact -> if (popsDown) {
            // Narrow round the camera (an icon each side), its words in a row under the status bar.
            val rows = DROP_TEXT + if (lyricsRow && scene.live is Live.Music) LYRICS_ROW else 0.0
            centered(compactDropWidth, idleHeight + rows * k, 22.0 * k)
        } else if (lyricsRow && scene.live is Live.Music) {
            centered(idleWidth + 2 * wing + 60 * k, idleHeight + LYRICS_ROW * k, idleHeight / 2)
        } else {
            centered(idleWidth + 2 * wing, idleHeight, idleHeight / 2)
        }
        is Scene.Popped -> if (popsDown) dropped(scene.pop) else when (scene.pop) {
            is Pop.Note -> centered(cardWidth - 8, if (scene.pop.actions.isEmpty()) 82.0 else 112.0, 36.0 * k)
            is Pop.Level -> centered(idleWidth + 2 * wing + 30 * k, idleHeight + 22 * k, idleHeight / 2)
            is Pop.Screenshot, is Pop.TimerDone, is Pop.Bluetooth ->
                centered(cardWidth - 12, 78.0, 34.0 * k)
            else -> centered(idleWidth + 2 * wing + 30 * k, idleHeight + 6 * k, (idleHeight + 6 * k) / 2)
        }
        // Cards keep the height their contents need: the size setting is the pill's (their words and buttons don't scale).
        is Scene.Expanded -> centered(cardWidth, expandedHeight(scene.live), 40.0 * k)
        // The control centre is the clock card, as on the computer; its toggles button opens the toggles.
        Scene.Controls -> centered(cardWidth, controlsHeight, 40.0 * k)
        Scene.Toggles -> centered(cardWidth, 292.0, 40.0 * k)
        Scene.Peek -> centered(idleWidth + 2 * (wing + 30 * k), idleHeight + 4 * k, (idleHeight + 4 * k) / 2)
        Scene.Calendar -> centered(cardWidth, 344.0, 40.0 * k)
    }

    /** A pop-up that grows downward (see `popsDown`). */
    private fun dropped(pop: Pop): Box = when (pop) {
        is Pop.Note, is Pop.Screenshot, is Pop.TimerDone, is Pop.Bluetooth -> {
            val h = if (pop is Pop.Note) (if (pop.actions.isEmpty()) 82.0 else 112.0) else 78.0
            val w = if (pop is Pop.Note) cardWidth - 8 else cardWidth - 12
            centered(w, h, if (pop is Pop.Note) 36.0 * k else 34.0 * k).copy(y = belowStatusBar)
        }
        is Pop.Level -> centered(dropWidth, idleHeight + DROP_LEVEL * k, idleHeight / 2)
        else -> centered(dropWidth, idleHeight + DROP_ROWS * k, idleHeight / 2)
    }

    /** The small island's width when it grows downward: an icon each side of the camera. */
    val compactDropWidth get() = maxOf(idleWidth, COMPACT_DROP_WIDTH * k)

    /** A dropped pop-up's width: the camera's pill, or a little wider for its words. */
    val dropWidth get() = maxOf(idleWidth, DROP_WIDTH * k)

    /** Just under the camera's pill, which is about where the status bar ends. */
    val belowStatusBar get() = top + idleHeight + 4 * k

    /**
     * The band an open activity card leaves empty beside the camera (dp): the
     * phone's status bar icons are drawn over it, so the card's first row
     * (its "‹ Control Centre" button) starts under the camera's pill.
     */
    val cardBand get() = idleHeight + 4 * k

    private fun expandedHeight(live: Live?): Double = when (live) {
        null, is Live.Music -> 0.0
        else -> CARD_HEADER + cardBand
    } + when (live) {
        is Live.Music -> MUSIC_CARD + if (musicLyrics) MUSIC_LYRICS else 0.0
        is Live.Call -> 120.0
        is Live.Navigation -> 120.0
        is Live.Stopwatch -> 140.0
        null -> controlsHeight
        else -> 128.0
    }

    companion object {
        /** The clock card's (and the control centre's) height, as on the computer (dp). */
        const val CLOCK_CARD = 210.0

        /** The music card (designed in Figma), and the room its two lines of lyrics add (dp). */
        const val MUSIC_CARD = 324.0
        const val MUSIC_LYRICS = 60.0

        /** The lyrics row under the small music pill (dp). */
        const val LYRICS_ROW = 24.0

        /** Room at the top of an open card for its "‹ Control Centre" button (dp). */
        const val CARD_HEADER = 40.0

        /** A dropped pop-up: its narrowest width, and the rows under the camera (dp). */
        const val DROP_WIDTH = 150.0
        const val DROP_ROWS = 46.0
        const val DROP_LEVEL = 30.0

        /** The small island growing downward: its width, and its row of words under the camera (dp). */
        const val COMPACT_DROP_WIDTH = 140.0
        const val DROP_TEXT = 28.0
    }

    /** The second activity's round bubble, right of the pill. */
    fun bubble(pill: Box): Box {
        val d = minOf(pill.h, idleHeight)
        val x = minOf(pill.right + 6, screenWidth - 6 - d)
        // Growing downward, the second activity waits beside the words, under the status bar.
        return Box(x, if (popsDown) belowStatusBar else pill.y, d, d, d / 2)
    }
}
