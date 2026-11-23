package io.github.jimchi_cmd.islet

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.widget.Toast

/**
 * "Islet" in the phone's Share menu: text or a link goes to the paired
 * computer's clipboard. No window of its own; a short message says how it went.
 */
class ShareActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Isle.init(this)
        val text = if (intent?.action == Intent.ACTION_SEND) {
            listOfNotNull(intent.getStringExtra(Intent.EXTRA_SUBJECT), intent.getStringExtra(Intent.EXTRA_TEXT))
                .map { it.trim() }
                .filter { it.isNotEmpty() }
                .distinct()
                .joinToString("\n")
        } else {
            ""
        }
        val computer = Hub.computer
        val words = when {
            text.isEmpty() -> T.t("Only text and links can be sent to the computer.")
            computer == null || IsletListener.instance?.share(text) != true -> T.t("Your computer isn't connected. Open Islet on it, on the same Wi-Fi.")
            else -> T.f("Sent to {}", computer)
        }
        Toast.makeText(this, words, Toast.LENGTH_SHORT).show()
        finish()
    }
}
