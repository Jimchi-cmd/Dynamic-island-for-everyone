package islet.island

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class ControlCentreTest {
    @Test
    fun every_button_stays_with_the_users_first() {
        val o = ControlCentre.order(listOf("lock", "nonsense", "lock", "wifi"))
        assertEquals(listOf("lock", "wifi", "bluetooth"), o.take(3))
        assertEquals(ControlCentre.BUTTONS.sorted(), o.sorted())
        assertEquals(ControlCentre.BUTTONS, ControlCentre.order(emptyList()))
        assertEquals(listOf("b", "a", "c"), ControlCentre.moved(listOf("a", "b", "c"), "b", -1))
        assertEquals(listOf("a", "b", "c"), ControlCentre.moved(listOf("a", "b", "c"), "a", -1))
    }

    @Test
    fun each_layout_has_its_height_and_room_for_the_way_back() {
        assertEquals(Geometry.CLOCK_CARD, ControlCentre.height("classic", true))
        assertEquals(Geometry.CLOCK_CARD, ControlCentre.height("nonsense", false))
        for (s in listOf("modules", "sliders", "dock")) {
            val h = ControlCentre.height(s, false)
            assertTrue(h in 200.0..260.0, "$s is $h")
            assertEquals(h + ControlCentre.BACK, ControlCentre.height(s, true))
        }
        val g = Geometry(400.0, null, controlsHeight = 240.0)
        assertEquals(240.0, g.box(Scene.Controls).h)
    }
}
