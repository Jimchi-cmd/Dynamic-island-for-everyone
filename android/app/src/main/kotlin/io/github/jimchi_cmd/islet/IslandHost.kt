package io.github.jimchi_cmd.islet

import android.accessibilityservice.AccessibilityService
import android.content.Context
import android.provider.Settings
import android.view.WindowManager

/**
 * Which service holds the island's window. The accessibility service is
 * preferred: its window may cover the status bar round the camera. Without
 * it, the notification listener shows the island as an ordinary overlay
 * ("Display over other apps"), just below the status bar's touch area.
 */
object IslandHost {
    private var window: IslandWindow? = null
    private var accessibility: AccessibilityService? = null
    private var listener: Context? = null

    fun accessibilityUp(s: AccessibilityService) {
        accessibility = s
        Sensors.start(s)
        rebuild()
    }

    fun accessibilityDown() {
        if (accessibility == null) return
        accessibility = null
        Sensors.stop()
        rebuild()
    }

    fun listenerUp(c: Context) {
        listener = c
        Sensors.start(c)
        rebuild()
    }

    fun listenerDown() {
        if (listener == null) return
        listener = null
        Sensors.stop()
        rebuild()
    }

    val hosted get() = window != null

    val overAccessibility get() = window != null && accessibility != null

    /** Put the window on the best host there is (or take it away). */
    fun rebuild() {
        window?.detach()
        window = null
        IslandWindow.current = null
        Isle.onChange = null
        if (!Isle.prefs.islandOn) return
        val a = accessibility
        val l = listener
        val w = when {
            a != null -> IslandWindow(a, WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY)
            l != null && Settings.canDrawOverlays(l) -> IslandWindow(l, WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY)
            else -> null
        } ?: return
        w.attach()
        window = w
        IslandWindow.current = w
        Isle.changed()
    }
}
