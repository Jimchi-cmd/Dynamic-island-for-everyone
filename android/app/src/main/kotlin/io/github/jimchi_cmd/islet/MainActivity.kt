package io.github.jimchi_cmd.islet

import android.content.ComponentName
import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import android.service.notification.NotificationListenerService
import android.text.InputType
import android.util.TypedValue
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.appcompat.widget.SwitchCompat
import androidx.core.app.NotificationManagerCompat
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import islet.protocol.Pairing

/** The app's one screen: pair with the computer, allow notification access, see whether it is connected. */
class MainActivity : AppCompatActivity() {
    private lateinit var status: TextView
    private lateinit var detail: TextView
    private lateinit var accessButton: Button
    private lateinit var scanButton: Button
    private lateinit var pasteButton: Button
    private lateinit var forgetButton: Button
    private lateinit var notesSwitch: SwitchCompat
    private lateinit var mediaSwitch: SwitchCompat

    private val scan = registerForActivityResult(ScanContract()) { result ->
        result.contents?.let { pairWith(it) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(buildScreen())
        handleLink(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleLink(intent)
    }

    override fun onStart() {
        super.onStart()
        Hub.onChange = { runOnUiThread { render() } }
        // Ask Android to start the listener if it is allowed but not running.
        if (hasAccess() && IsletListener.instance == null) {
            NotificationListenerService.requestRebind(ComponentName(this, IsletListener::class.java))
        }
        render()
    }

    override fun onStop() {
        Hub.onChange = null
        super.onStop()
    }

    /** A pairing link opened from a camera app or a browser. */
    private fun handleLink(intent: Intent?) {
        val link = intent?.dataString ?: return
        if (link.startsWith("islet://pair")) {
            intent.data = null
            pairWith(link)
        }
    }

    private fun pairWith(code: String) {
        val pairing = Pairing.fromUri(code)
        if (pairing == null) {
            message("That isn't an Islet pairing code. On your computer, open Islet Settings → Islet Pro → Your Android phone.")
            return
        }
        Store.pair(this, pairing)
        IsletListener.instance?.reload()
        render()
        if (!hasAccess()) askForAccess()
    }

    private fun hasAccess() = NotificationManagerCompat.getEnabledListenerPackages(this).contains(packageName)

    private fun askForAccess() {
        AlertDialog.Builder(this)
            .setTitle("Allow notification access")
            .setMessage(
                "Islet needs notification access to show your phone's notifications and music on your computer. " +
                    "On the next screen, turn on Islet.\n\n" +
                    "If the switch is greyed out: open this app's App info, tap the three dots at the top right, " +
                    "choose \"Allow restricted settings\", then come back.",
            )
            .setPositiveButton("Open settings") { _, _ -> startActivity(Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS)) }
            .setNegativeButton("Not now", null)
            .show()
    }

    private fun message(text: String) {
        AlertDialog.Builder(this).setMessage(text).setPositiveButton("OK", null).show()
    }

    private fun pasteCode() {
        val input = EditText(this).apply {
            hint = "islet://pair?…"
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
        }
        AlertDialog.Builder(this)
            .setTitle("Paste the pairing code")
            .setView(input)
            .setPositiveButton("Pair") { _, _ -> pairWith(input.text.toString()) }
            .setNegativeButton("Cancel", null)
            .show()
    }

    private fun render() {
        val pairing = Store.pairing(this)
        val access = hasAccess()
        val computer = Hub.computer
        status.text = when {
            pairing == null -> "Not paired"
            !access -> "Paired with ${pairing.name}"
            computer != null -> "Connected to $computer"
            else -> "Looking for ${pairing.name}…"
        }
        detail.text = when {
            pairing == null ->
                "On your computer, open Islet Settings → Islet Pro → Your Android phone and choose Pair a phone. " +
                    "Then scan the code it shows. Both devices must be on the same Wi-Fi."
            !access -> "One more step: allow notification access, so Islet can see your notifications and what is playing."
            computer != null -> "Your notifications, music and battery level now show on the island."
            else ->
                "Make sure the computer is on, Islet is running and both are on the same Wi-Fi." +
                    (if (Hub.problem.isNotEmpty()) "\n\nLast try: ${Hub.problem}." else "")
        }
        accessButton.visibility = if (pairing != null && !access) View.VISIBLE else View.GONE
        scanButton.text = if (pairing == null) "Scan the code" else "Scan a new code"
        forgetButton.visibility = if (pairing != null) View.VISIBLE else View.GONE
        notesSwitch.visibility = if (pairing != null) View.VISIBLE else View.GONE
        mediaSwitch.visibility = notesSwitch.visibility
    }

    private fun dp(value: Int) =
        TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value.toFloat(), resources.displayMetrics).toInt()

    private fun buildScreen(): View {
        val column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(24), dp(32), dp(24), dp(24))
        }
        fun add(view: View, top: Int = 12) {
            val params = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            params.topMargin = dp(top)
            column.addView(view, params)
        }
        add(TextView(this).apply {
            text = "Islet"
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 30f)
            setTypeface(typeface, android.graphics.Typeface.BOLD)
        }, 0)
        status = TextView(this).apply { setTextSize(TypedValue.COMPLEX_UNIT_SP, 20f) }
        add(status, 20)
        detail = TextView(this).apply { setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f) }
        add(detail, 8)

        accessButton = Button(this).apply {
            text = "Allow notification access"
            setOnClickListener { askForAccess() }
        }
        add(accessButton, 20)
        scanButton = Button(this).apply {
            setOnClickListener {
                scan.launch(
                    ScanOptions()
                        .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                        .setPrompt("Point at the code in Islet Settings on your computer")
                        .setBeepEnabled(false)
                        .setOrientationLocked(false),
                )
            }
        }
        add(scanButton, 20)
        pasteButton = Button(this).apply {
            text = "Paste a code instead"
            setOnClickListener { pasteCode() }
        }
        add(pasteButton)

        notesSwitch = SwitchCompat(this).apply {
            text = "Send notifications"
            isChecked = Store.sendNotes(this@MainActivity)
            setOnCheckedChangeListener { _, on -> Store.setSendNotes(this@MainActivity, on) }
        }
        add(notesSwitch, 24)
        mediaSwitch = SwitchCompat(this).apply {
            text = "Send what is playing"
            isChecked = Store.sendMedia(this@MainActivity)
            setOnCheckedChangeListener { _, on ->
                Store.setSendMedia(this@MainActivity, on)
                IsletListener.instance?.reload()
            }
        }
        add(mediaSwitch)

        forgetButton = Button(this).apply {
            text = "Forget this computer"
            setOnClickListener {
                Store.forget(this@MainActivity)
                IsletListener.instance?.reload()
                render()
            }
        }
        add(forgetButton, 24)
        add(TextView(this).apply {
            text = "Your phone talks straight to your computer over your Wi-Fi, encrypted with a key only the two hold. " +
                "Nothing goes through the internet, and Islet has no account or server."
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
            alpha = 0.7f
        }, 28)
        return ScrollView(this).apply { addView(column) }
    }
}
