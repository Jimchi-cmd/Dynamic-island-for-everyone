package io.github.jimchi_cmd.islet

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import islet.island.Automation

/**
 * Other apps put live activities on the island with a broadcast (only
 * while "Activities from other apps" is on):
 *
 *   action  io.github.jimchi_cmd.islet.ACTIVITY
 *   extras  command = start | update | end, id, title, progress (0–100)
 *
 * The same as `islet activity start build --title Build --progress 40` on
 * the computer.
 */
class AutomationReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ACTION) return
        Isle.init(context)
        if (!Isle.prefs.automation) return
        val id = intent.getStringExtra("id") ?: return
        when (intent.getStringExtra("command") ?: "start") {
            "end", "stop" -> Isle.island.remove(Automation.key(id))
            else -> {
                val progress = if (intent.hasExtra("progress")) {
                    // Tasker sends numbers as text; other apps as numbers.
                    intent.getStringExtra("progress")?.trim()?.toIntOrNull() ?: intent.getIntExtra("progress", -1)
                } else {
                    -1
                }
                val live = Automation.live(id, intent.getStringExtra("title"), progress, intent.getStringExtra("app") ?: "Automation") ?: return
                if (!Automation.fits(Isle.island.all, id)) return
                Isle.island.put(live)
            }
        }
        Isle.changed()
    }

    companion object {
        const val ACTION = "io.github.jimchi_cmd.islet.ACTIVITY"
    }
}
