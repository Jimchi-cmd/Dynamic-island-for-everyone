package io.github.jimchi_cmd.islet

import android.accessibilityservice.AccessibilityService
import android.app.NotificationManager
import android.content.Intent
import android.hardware.camera2.CameraCharacteristics
import android.hardware.camera2.CameraManager
import android.media.AudioManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.service.notification.NotificationListenerService
import islet.island.Pop

/**
 * The control centre's buttons and sliders. Each uses only what Android
 * lets an app do; where it doesn't (Wi-Fi, Bluetooth and airplane mode
 * since Android 10), the button opens the system's own panel instead.
 */
object Controls {
    private val app get() = Isle.app

    fun flashlight() {
        val cm = app.getSystemService(CameraManager::class.java) ?: return
        val id = try {
            cm.cameraIdList.firstOrNull { cm.getCameraCharacteristics(it).get(CameraCharacteristics.FLASH_INFO_AVAILABLE) == true }
        } catch (e: Exception) {
            null
        } ?: return Isle.pop(Pop.Info("No flashlight"))
        try {
            cm.setTorchMode(id, !Isle.torchOn)
        } catch (e: Exception) {
            Isle.pop(Pop.Info("The camera is busy"))
        }
    }

    /** Do Not Disturb, through the notification listener (no extra permission). */
    fun doNotDisturb() {
        val listener = IsletListener.instance
        if (listener == null) {
            Isle.pop(Pop.Info("Allow notification access first"))
            return
        }
        listener.requestInterruptionFilter(
            if (Isle.dnd) NotificationListenerService.INTERRUPTION_FILTER_ALL else NotificationListenerService.INTERRUPTION_FILTER_PRIORITY,
        )
    }

    val ringerMode: Int get() = app.getSystemService(AudioManager::class.java)?.ringerMode ?: AudioManager.RINGER_MODE_NORMAL

    /** Ring ⇄ vibrate. (Silent needs Do Not Disturb access, so it's left to the phone's own button.) */
    fun ringer() {
        val am = app.getSystemService(AudioManager::class.java) ?: return
        try {
            am.ringerMode = if (am.ringerMode == AudioManager.RINGER_MODE_NORMAL) AudioManager.RINGER_MODE_VIBRATE else AudioManager.RINGER_MODE_NORMAL
        } catch (e: SecurityException) {
            Isle.pop(Pop.Info("Change it with the volume buttons"))
        }
    }

    fun wifi() = panel(if (Build.VERSION.SDK_INT >= 29) Settings.Panel.ACTION_WIFI else Settings.ACTION_WIFI_SETTINGS)

    fun bluetooth() = panel(Settings.ACTION_BLUETOOTH_SETTINGS)

    fun airplane() = panel(Settings.ACTION_AIRPLANE_MODE_SETTINGS)

    private fun panel(action: String) {
        Isle.island.close(Isle.now())
        Isle.changed()
        Isle.start(Intent(action))
    }

    /** Through the accessibility service (Android 9+). */
    fun screenshot() = global(AccessibilityService.GLOBAL_ACTION_TAKE_SCREENSHOT)

    fun lock() = global(AccessibilityService.GLOBAL_ACTION_LOCK_SCREEN)

    val canScreenshot get() = Build.VERSION.SDK_INT >= 28 && IslandAccessibility.instance != null

    private fun global(action: Int) {
        val a = IslandAccessibility.instance
        if (a == null || Build.VERSION.SDK_INT < 28) {
            Isle.pop(Pop.Info("Needs Islet's accessibility switch"))
            return
        }
        Isle.island.close(Isle.now())
        Isle.changed()
        // Let the island shrink out of the picture first.
        Isle.app.mainLooper.let { android.os.Handler(it).postDelayed({ a.performGlobalAction(action) }, 450) }
    }

    /** 0..1 */
    var volume: Float
        get() {
            val am = app.getSystemService(AudioManager::class.java) ?: return 0f
            val max = am.getStreamMaxVolume(AudioManager.STREAM_MUSIC).coerceAtLeast(1)
            return am.getStreamVolume(AudioManager.STREAM_MUSIC).toFloat() / max
        }
        set(v) {
            val am = app.getSystemService(AudioManager::class.java) ?: return
            val max = am.getStreamMaxVolume(AudioManager.STREAM_MUSIC)
            try {
                am.setStreamVolume(AudioManager.STREAM_MUSIC, (v.coerceIn(0f, 1f) * max + 0.5f).toInt(), 0)
            } catch (e: SecurityException) {
                // Do Not Disturb blocks it.
            }
        }

    val canSetBrightness get() = Settings.System.canWrite(app)

    /** 0..1 */
    var brightness: Float
        get() = try {
            Settings.System.getInt(app.contentResolver, Settings.System.SCREEN_BRIGHTNESS) / 255f
        } catch (e: Exception) {
            0.5f
        }
        set(v) {
            if (!canSetBrightness) return
            try {
                Settings.System.putInt(app.contentResolver, Settings.System.SCREEN_BRIGHTNESS_MODE, Settings.System.SCREEN_BRIGHTNESS_MODE_MANUAL)
                Settings.System.putInt(app.contentResolver, Settings.System.SCREEN_BRIGHTNESS, (v.coerceIn(0.02f, 1f) * 255).toInt())
            } catch (e: Exception) {
                // Not allowed after all.
            }
        }

    /** Ask for "Modify system settings", which the brightness slider needs. */
    fun askForBrightness() {
        Isle.island.close(Isle.now())
        Isle.changed()
        Isle.start(Intent(Settings.ACTION_MANAGE_WRITE_SETTINGS, Uri.parse("package:${app.packageName}")))
    }

    val dndAllowed get() = app.getSystemService(NotificationManager::class.java) != null && IsletListener.instance != null
}
