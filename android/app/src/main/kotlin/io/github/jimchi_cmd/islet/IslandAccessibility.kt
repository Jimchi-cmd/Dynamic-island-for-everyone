package io.github.jimchi_cmd.islet

import android.accessibilityservice.AccessibilityService
import android.view.accessibility.AccessibilityEvent

/**
 * Only here to place the island over the status bar, round the camera, and
 * for the control centre's screenshot and lock buttons. It asks for no
 * events and reads nothing on the screen (see res/xml/island_accessibility.xml).
 */
class IslandAccessibility : AccessibilityService() {
    override fun onServiceConnected() {
        instance = this
        IslandHost.accessibilityUp(this)
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) = Unit

    override fun onInterrupt() = Unit

    override fun onUnbind(intent: android.content.Intent?): Boolean {
        if (instance === this) instance = null
        IslandHost.accessibilityDown()
        return super.onUnbind(intent)
    }

    override fun onDestroy() {
        if (instance === this) instance = null
        IslandHost.accessibilityDown()
        super.onDestroy()
    }

    companion object {
        @Volatile var instance: IslandAccessibility? = null
    }
}
