package io.github.jimchi_cmd.islet

import android.content.ComponentName
import android.provider.Settings
import islet.island.Pop
import islet.island.Scene
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/** Lock and screenshot need the accessibility switch running: without it, they say so. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class ControlsTest {
    private val ctx = RuntimeEnvironment.getApplication()

    private fun popText(): String? = (Isle.island.scene(Isle.now()) as? Scene.Popped)?.let { (it.pop as? Pop.Info)?.text }

    @Test
    fun a_switch_that_is_on_but_stopped_is_spotted_and_explained() {
        Isle.init(ctx)
        IslandAccessibility.instance = null
        val me = ComponentName(ctx, IslandAccessibility::class.java).flattenToString()
        Settings.Secure.putString(ctx.contentResolver, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES, "other/.Service:$me")
        assertTrue(IslandAccessibility.switchedOn(ctx))
        Controls.lock()
        org.robolectric.shadows.ShadowLooper.idleMainLooper()
        assertEquals("Turn Islet island off and on again", popText())
    }

    @Test
    fun a_switch_that_is_off_says_to_turn_it_on() {
        Isle.init(ctx)
        IslandAccessibility.instance = null
        Settings.Secure.putString(ctx.contentResolver, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES, "")
        assertFalse(IslandAccessibility.switchedOn(ctx))
        Controls.screenshot()
        org.robolectric.shadows.ShadowLooper.idleMainLooper()
        assertEquals("Turn on Islet island to use this", popText())
    }
}
