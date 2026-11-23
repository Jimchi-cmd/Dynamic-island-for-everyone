package islet.island

/**
 * Control Centre's layouts and its buttons' order, as on the computer:
 * Classic, Modules (a block of round buttons with search beside it),
 * Sliders (volume and brightness as tall sliders beside a grid) and Dock
 * (a big clock with every button in one labelled row).
 */
object ControlCentre {
    val STYLES = listOf("classic", "modules", "sliders", "dock")

    /** Every button, in the usual order. */
    val BUTTONS = listOf("wifi", "bluetooth", "search", "todo", "speed", "controls", "calendar", "screenshot", "lock")

    /** The buttons in the user's order; ones left out follow in the usual order. */
    fun order(saved: List<String>): List<String> {
        val out = ArrayList<String>()
        for (id in saved) if (id in BUTTONS && id !in out) out.add(id)
        for (id in BUTTONS) if (id !in out) out.add(id)
        return out
    }

    /** The order with one button moved a place up (-1) or down (+1). */
    fun moved(order: List<String>, id: String, by: Int): List<String> {
        val out = order.toMutableList()
        val i = out.indexOf(id)
        val j = i + by
        if (i < 0 || j < 0 || j >= out.size) return out
        out[i] = out[j].also { out[j] = out[i] }
        return out
    }

    /** Room at the top for "Now Playing ›" when opened from another card (dp). */
    const val BACK = 24.0
    const val TOP = 16.0
    /** Modules' and Sliders' header (the time, the date and the weather) and body (dp). */
    const val HEAD = 72.0
    const val BODY = 126.0
    /** Dock's header (a big clock) and its row of buttons (dp). */
    const val DOCK_HEAD = 112.0
    const val DOCK = 84.0

    /** Control Centre's height in a layout (dp, before the island's size). */
    fun height(style: String, back: Boolean): Double {
        val b = if (back) BACK else 0.0
        return when (style) {
            "modules", "sliders" -> TOP + b + HEAD + BODY + 20
            "dock" -> TOP + b + DOCK_HEAD + DOCK + 18
            else -> Geometry.CLOCK_CARD
        }
    }
}
