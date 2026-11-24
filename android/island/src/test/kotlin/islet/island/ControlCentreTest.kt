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

class NoteLengthTest {
    @Test
    fun notifications_go_quickly_unless_asked_to_stay() {
        val before = Pop.noteMs
        try {
            assertEquals(3000L, Pop.noteMsFor("normal"))
            assertEquals(2000L, Pop.noteMsFor("brief"))
            assertEquals(5000L, Pop.noteMsFor("long"))
            assertEquals(3000L, Pop.noteMsFor("nonsense"))
            Pop.noteMs = Pop.noteMsFor("brief")
            assertEquals(2000L, Pop.Note("k", "Messages", "Sam", "Hi").lasts)
            assertEquals(3000L, Pop.Note("k", "Messages", "Sam", "Hi", actions = listOf("Reply")).lasts, "longer with buttons")
            // It leaves on time, and the next one waiting comes on.
            val island = Island()
            island.pop(Pop.Note("a", "Messages", "Sam", "Hi"), 0)
            island.pop(Pop.Note("b", "Mail", "News", "…"), 10)
            assertTrue(island.scene(1999) is Scene.Popped)
            val next = island.scene(2001) as? Scene.Popped
            assertEquals("b", (next?.pop as? Pop.Note)?.key)
        } finally {
            Pop.noteMs = before
        }
    }
}
