package io.github.jimchi_cmd.islet

import android.app.Notification
import android.app.RemoteInput
import android.content.Intent
import android.os.Bundle
import android.text.InputType
import android.view.inputmethod.EditorInfo
import android.widget.EditText
import android.widget.FrameLayout
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity

/**
 * A small box for a notification's "Reply" button, opened from the island
 * (the island's own window can't take typing). The text goes back to the
 * app the way its own notification would send it.
 */
class ReplyActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val action = pending
        pending = null
        val inputs = action?.remoteInputs
        if (action == null || inputs.isNullOrEmpty()) {
            finish()
            return
        }
        val box = EditText(this).apply {
            hint = inputs[0].label ?: action.title
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES or InputType.TYPE_TEXT_FLAG_MULTI_LINE
            imeOptions = EditorInfo.IME_ACTION_SEND
            maxLines = 4
        }
        val padded = FrameLayout(this).apply {
            val d = resources.displayMetrics.density
            setPadding((22 * d).toInt(), (8 * d).toInt(), (22 * d).toInt(), 0)
            addView(box)
        }
        val dialog = AlertDialog.Builder(this)
            .setTitle(action.title)
            .setView(padded)
            .setPositiveButton(T.t("Send")) { _, _ -> send(action, inputs, box.text.toString()) }
            .setNegativeButton(T.t("Cancel"), null)
            .setOnDismissListener { finish() }
            .create()
        box.setOnEditorActionListener { _, id, _ ->
            if (id == EditorInfo.IME_ACTION_SEND) {
                send(action, inputs, box.text.toString())
                dialog.dismiss()
                true
            } else {
                false
            }
        }
        dialog.window?.setSoftInputMode(android.view.WindowManager.LayoutParams.SOFT_INPUT_STATE_VISIBLE)
        dialog.show()
        box.requestFocus()
    }

    private fun send(action: Notification.Action, @Suppress("UNUSED_PARAMETER") inputs: Array<RemoteInput>, text: String) {
        Together.answer(this, action, text)
    }

    companion object {
        /** The button pressed on the island, handed over to the box. */
        var pending: Notification.Action? = null
    }
}
