package io.github.jimchi_cmd.islet

import android.Manifest
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.service.notification.NotificationListenerService
import android.text.InputType
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.CheckBox
import android.widget.EditText
import android.widget.ImageButton
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.appcompat.widget.SwitchCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import islet.island.Forecast
import islet.island.Format
import islet.island.IslandSize
import islet.island.Place
import islet.island.Todo
import islet.protocol.Pairing

/**
 * The app's screen: set up the island on this phone, its looks and what it
 * shows, timers, weather, clocks, to-dos and notes, and the link to the
 * computer. Built in code, like the island itself, so the app needs no UI
 * library beyond AppCompat.
 */
class MainActivity : AppCompatActivity() {
    private lateinit var prefs: Prefs
    private lateinit var scroll: ScrollView
    private lateinit var column: LinearLayout
    private var speedResult = ""

    private val scan = registerForActivityResult(ScanContract()) { result ->
        result.contents?.let { pairWith(it) }
    }

    private val askPermission = registerForActivityResult(ActivityResultContracts.RequestPermission()) {
        Sensors.refresh()
        build()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Isle.init(this)
        prefs = Isle.prefs
        scroll = ScrollView(this)
        setContentView(scroll)
        handleLink(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleLink(intent)
    }

    override fun onStart() {
        super.onStart()
        Hub.onChange = { runOnUiThread { build() } }
        // Ask Android to start the listener if it is allowed but not running.
        if (hasAccess() && IsletListener.instance == null) {
            NotificationListenerService.requestRebind(ComponentName(this, IsletListener::class.java))
        }
        // An overlay allowed since last time: show the island now.
        if (!IslandHost.hosted) IslandHost.rebuild()
        Sensors.refresh()
        build()
    }

    override fun onStop() {
        Hub.onChange = null
        super.onStop()
    }

    // ---- Helpers for building the screen ---------------------------------------------------

    private fun dp(value: Int) =
        TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value.toFloat(), resources.displayMetrics).toInt()

    private val accent = 0xff0a84ff.toInt()

    private fun add(view: View, top: Int = 10, into: LinearLayout = column) {
        val params = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        params.topMargin = dp(top)
        into.addView(view, params)
    }

    private fun label(s: String, sp: Float = 15f, bold: Boolean = false, alpha: Float = 1f) = TextView(this).apply {
        text = s
        setTextSize(TypedValue.COMPLEX_UNIT_SP, sp)
        if (bold) setTypeface(typeface, Typeface.BOLD)
        this.alpha = alpha
    }

    private fun heading(s: String) = add(label(s, 20f, bold = true), 30)

    private fun note(s: String, top: Int = 6) = add(label(s, 13f, alpha = 0.7f), top)

    private fun button(s: String, primary: Boolean = false, action: () -> Unit) = Button(this).apply {
        text = s
        isAllCaps = false
        if (primary) {
            background = GradientDrawable().apply {
                cornerRadius = dp(22).toFloat()
                setColor(accent)
            }
            setTextColor(Color.WHITE)
        }
        setOnClickListener { action() }
    }

    private fun toggle(s: String, get: () -> Boolean, set: (Boolean) -> Unit) = add(
        SwitchCompat(this).apply {
            text = s
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
            isChecked = get()
            setOnCheckedChangeListener { _, on ->
                set(on)
                Isle.changed()
            }
        },
        4,
    )

    private fun row(vararg views: View, top: Int = 8): LinearLayout {
        val r = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        for (v in views) {
            val p = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            p.marginEnd = dp(8)
            r.addView(v, p)
        }
        add(r, top)
        return r
    }

    /** A labelled number with − and + buttons. */
    private fun stepper(name: String, unit: String, step: Int, get: () -> Int, set: (Int) -> Unit) {
        val value = label("", 15f)
        fun show() {
            value.text = "$name: ${get()}$unit"
        }
        show()
        val minus = button("−") {
            set(get() - step)
            show()
            Isle.changed()
        }
        val plus = button("+") {
            set(get() + step)
            show()
            Isle.changed()
        }
        row(minus, plus, value, top = 2)
    }

    private fun choices(options: List<Pair<String, String>>, current: String, pick: (String) -> Unit) {
        val buttons = options.map { (id, name) ->
            button(if (id == current) "✓ $name" else name) {
                pick(id)
                Isle.changed()
                build()
            }
        }
        val wrap = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        buttons.chunked(3).forEach { chunk ->
            val r = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
            chunk.forEach { r.addView(it, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)) }
            wrap.addView(r)
        }
        add(wrap, 4)
    }

    private fun granted(p: String) = ContextCompat.checkSelfPermission(this, p) == PackageManager.PERMISSION_GRANTED

    // ---- The screen ---------------------------------------------------------------------------

    private fun build() {
        val y = scroll.scrollY
        column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(22), dp(28), dp(22), dp(40))
        }
        add(label("Islet", 30f, bold = true), 0)
        islandSection()
        lookSection()
        showsSection()
        timerSection()
        weatherSection()
        todoSection()
        controlsSection()
        musicSection()
        speedSection()
        computerSection()
        add(
            label(
                "Islet has no account, no ads and no tracking. The island works on the phone; it only goes online for " +
                    "what you switch on here (weather for your place, lyrics, a speed test).",
                12f,
                alpha = 0.6f,
            ),
            32,
        )
        scroll.removeAllViews()
        scroll.addView(column)
        scroll.post { scroll.scrollTo(0, y) }
    }

    private fun hasAccess() = NotificationManagerCompat.getEnabledListenerPackages(this).contains(packageName)

    private fun islandSection() {
        heading("The island on this phone")
        toggle("Show the island", { prefs.islandOn }) {
            prefs.islandOn = it
            IslandHost.rebuild()
        }
        val access = hasAccess()
        val accessibility = IslandAccessibility.instance != null
        val overlay = Settings.canDrawOverlays(this)
        val status = when {
            !prefs.islandOn -> "The island is off."
            IslandHost.overAccessibility -> "The island is showing round the camera."
            IslandHost.hosted -> "The island is showing (just below the status bar). Turn on the accessibility switch to place it round the camera."
            !access -> "Two steps to go: allow notification access, then let the island show."
            else -> "One more step: let the island show over other apps."
        }
        add(label(status, 15f, bold = true), 8)

        fun step(done: Boolean, title: String, why: String, action: String, go: () -> Unit) {
            add(label((if (done) "✓ " else "○ ") + title, 15f, bold = true), 12)
            note(why, 2)
            if (!done) row(button(action, primary = true, action = go), top = 4)
        }
        step(
            access,
            "Notification access",
            "For your notifications, music, calls, directions and downloads on the island.",
            "Allow",
        ) { askForAccess() }
        step(
            accessibility,
            "Round the camera (recommended)",
            "Settings → Accessibility → Islet island. Lets the island sit over the status bar round the front camera, " +
                "and gives the control centre its screenshot and lock buttons. Islet reads nothing on your screen.",
            "Open Accessibility",
        ) { startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)) }
        if (!accessibility) {
            step(
                overlay,
                "Or: display over other apps",
                "Shows the island without the accessibility switch, just below the status bar.",
                "Allow",
            ) { startActivity(Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, Uri.parse("package:$packageName"))) }
        }
        note(
            "If a switch is greyed out: open Settings → Apps → Islet, tap ⋮ at the top right, choose " +
                "\"Allow restricted settings\", and try again. Android asks this of apps installed outside the Play Store.",
            10,
        )
        add(label("Optional", 15f, bold = true), 16)
        fun optional(p: String, what: String) {
            if (!granted(p)) row(button("Allow: $what") { askPermission.launch(p) }, top = 2)
        }
        optional(Manifest.permission.READ_CALENDAR, "meeting countdowns")
        optional(if (Build.VERSION.SDK_INT >= 33) Manifest.permission.READ_MEDIA_IMAGES else Manifest.permission.READ_EXTERNAL_STORAGE, "screenshot previews")
        if (Build.VERSION.SDK_INT >= 31) optional(Manifest.permission.BLUETOOTH_CONNECT, "Bluetooth device names")
        if (!Settings.System.canWrite(this)) {
            row(button("Allow: brightness slider") { startActivity(Intent(Settings.ACTION_MANAGE_WRITE_SETTINGS, Uri.parse("package:$packageName"))) }, top = 2)
        }
        note(
            "Tap the island to open it, long-press for the control centre, swipe up to close it, and swipe sideways " +
                "to switch between two activities.",
            12,
        )
    }

    private fun lookSection() {
        heading("Look")
        add(label("Size", 15f), 6)
        choices(IslandSize.entries.map { it.name to it.name }, prefs.size.name) { prefs.size = IslandSize.valueOf(it) }
        note("If the pill doesn't sit exactly on your camera, nudge it:", 10)
        stepper("Left and right", " dp", 2, { prefs.offsetX }) { prefs.offsetX = it }
        stepper("Up and down", " dp", 1, { prefs.offsetY }) { prefs.offsetY = it }
        stepper("Wider", " dp", 4, { prefs.extraWidth }) { prefs.extraWidth = it }
        add(label("Font", 15f), 10)
        choices(
            listOf("default" to "Default", "light" to "Light", "condensed" to "Condensed", "serif" to "Serif", "mono" to "Mono"),
            prefs.font,
        ) { prefs.font = it }
        toggle("Show the empty island round the camera", { prefs.showEmpty }) { prefs.showEmpty = it }
        toggle("Border in the album's colour while music plays", { prefs.albumBorder }) { prefs.albumBorder = it }
        toggle("Moving bars beside the song", { prefs.bars }) { prefs.bars = it }
        toggle("Temperatures in °F", { prefs.fahrenheit }) { prefs.fahrenheit = it }
        toggle("Plane and moon on the island in airplane mode and Do Not Disturb", { prefs.modeIcons }) { prefs.modeIcons = it }
        toggle("Green and orange dots while the camera or microphone is on", { prefs.privacyDots }) { prefs.privacyDots = it }
        toggle("Hide the island in landscape (videos and games)", { prefs.hideInLandscape }) { prefs.hideInLandscape = it }
    }

    private fun showsSection() {
        heading("What it shows")
        toggle("Notifications pop up", { prefs.popNotes }) { prefs.popNotes = it }
        toggle("Charging and low battery", { prefs.popPower }) { prefs.popPower = it }
        toggle("Ring, vibrate and silent; airplane mode; Do Not Disturb", { prefs.popModes }) { prefs.popModes = it }
        toggle("Wi-Fi and Bluetooth connecting", { prefs.popConnections }) { prefs.popConnections = it }
        toggle("New screenshots", { prefs.popScreenshots }) { prefs.popScreenshots = it }
        toggle("Calls, with answer and hang-up buttons", { prefs.calls }) { prefs.calls = it }
        toggle("Directions from Google Maps, Waze and other maps apps", { prefs.navigation }) { prefs.navigation = it }
        toggle("Downloads and other progress", { prefs.progress }) { prefs.progress = it }
        toggle("Count down to meetings, with a Join button", { prefs.calendar }) {
            prefs.calendar = it
            Sensors.refresh()
        }
        val quiet = prefs.quietApps
        row(button(if (quiet.isEmpty()) "Apps that never pop up…" else "Apps that never pop up (${quiet.size})…") { pickQuietApps() })
    }

    private fun launchableApps(): List<Pair<String, String>> {
        val pm = packageManager
        val main = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        return pm.queryIntentActivities(main, 0)
            .map { it.activityInfo.packageName to it.loadLabel(pm).toString() }
            .filter { it.first != packageName }
            .distinctBy { it.first }
            .sortedBy { it.second.lowercase() }
    }

    private fun pickApps(title: String, chosen: Set<String>, limit: Int, done: (List<String>) -> Unit) {
        val apps = launchableApps()
        val checked = BooleanArray(apps.size) { apps[it].first in chosen }
        AlertDialog.Builder(this)
            .setTitle(title)
            .setMultiChoiceItems(apps.map { it.second }.toTypedArray(), checked) { d, i, on ->
                if (on && checked.count { it } > limit) {
                    (d as AlertDialog).listView.setItemChecked(i, false)
                    checked[i] = false
                } else {
                    checked[i] = on
                }
            }
            .setPositiveButton("Done") { _, _ -> done(apps.filterIndexed { i, _ -> checked[i] }.map { it.first }) }
            .setNegativeButton("Cancel", null)
            .show()
    }

    private fun pickQuietApps() = pickApps("Never pop up", prefs.quietApps, 500) {
        prefs.quietApps = it.toSet()
        build()
    }

    private fun timerSection() {
        heading("Timers")
        note("Also on the island: open it while it's empty for 1, 5, 10 and 25-minute timers.", 2)
        row(
            button("1 min") { Isle.startFocus(1.0) },
            button("5 min") { Isle.startFocus(5.0) },
            button("10 min") { Isle.startFocus(10.0) },
            button("25 min") { Isle.startFocus(25.0) },
        )
        row(
            button("Start focus rounds", primary = true) { Isle.startPomodoro() },
            button("Stopwatch") {
                Isle.stopwatch.start(Isle.now())
                Isle.syncTimers()
            },
            button("Stop") {
                Isle.focus.stop()
                Isle.stopwatch.reset()
                Isle.syncTimers()
            },
        )
        stepper("Focus", " min", 5, { prefs.focusMinutes }) { prefs.focusMinutes = it }
        stepper("Break", " min", 1, { prefs.breakMinutes }) { prefs.breakMinutes = it }
        note("Focus rounds: focus, a short break, and a longer break after every fourth.", 2)
    }

    private fun weatherSection() {
        heading("Weather and clocks")
        val place = prefs.place
        add(label(place?.let { "Weather for ${it.name}" } ?: "No place chosen: the island shows no weather.", 15f), 6)
        val w = Isle.weather
        if (place != null && w != null) note("${Format.temperature(w.celsius, prefs.fahrenheit)} · ${w.words}", 2)
        row(
            button(if (place == null) "Choose a place…" else "Change place…") { searchPlace() },
            *(if (place != null) arrayOf(button("Remove") { prefs.place = null; Isle.weather = null; build() }) else emptyArray()),
        )
        note("From Open-Meteo, free and without an account. Islet sends it only your place's coordinates.", 2)
        add(label("World clocks", 15f, bold = true), 14)
        for (zone in prefs.worldClocks) {
            row(
                label(zone.substringAfterLast('/').replace('_', ' '), 15f),
                button("Remove") {
                    prefs.worldClocks = prefs.worldClocks - zone
                    build()
                },
                top = 2,
            )
        }
        if (prefs.worldClocks.size < 3) row(button("Add a city…") { addClock() })
    }

    private fun searchPlace() {
        val input = EditText(this).apply {
            hint = "City or town"
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_WORDS
        }
        AlertDialog.Builder(this)
            .setTitle("Weather for…")
            .setView(input)
            .setPositiveButton("Search") { _, _ ->
                val q = input.text.toString()
                if (q.isBlank()) return@setPositiveButton
                Isle.background {
                    val found = Net.get(Forecast.searchUrl(q))?.let { Forecast.places(it) }.orEmpty()
                    runOnUiThread { choosePlace(found) }
                }
            }
            .setNegativeButton("Cancel", null)
            .show()
    }

    private fun choosePlace(found: List<Place>) {
        if (found.isEmpty()) {
            AlertDialog.Builder(this).setMessage("No place by that name (or no connection).").setPositiveButton("OK", null).show()
            return
        }
        AlertDialog.Builder(this)
            .setTitle("Choose")
            .setItems(found.map { it.label }.toTypedArray()) { _, i ->
                prefs.place = found[i]
                Sensors.readWeather(force = true)
                build()
            }
            .show()
    }

    private fun addClock() {
        val zones = java.util.TimeZone.getAvailableIDs()
            .filter { '/' in it && !it.startsWith("Etc") && !it.startsWith("SystemV") }
            .sortedBy { it.substringAfterLast('/') }
        val names = zones.map { "${it.substringAfterLast('/').replace('_', ' ')} (${it.substringBefore('/')})" }
        AlertDialog.Builder(this)
            .setTitle("Add a city")
            .setItems(names.toTypedArray()) { _, i ->
                prefs.worldClocks = (prefs.worldClocks + zones[i]).distinct()
                build()
            }
            .show()
    }

    private fun todoSection() {
        heading("To-do and notes")
        val todos = prefs.todos
        for ((i, t) in todos.withIndex()) {
            val box = CheckBox(this).apply {
                text = t.text
                isChecked = t.done
                setOnCheckedChangeListener { _, on ->
                    prefs.todos = prefs.todos.toMutableList().also { it[i] = Todo(t.text, on) }
                }
            }
            val remove = ImageButton(this).apply {
                setImageResource(android.R.drawable.ic_menu_delete)
                background = null
                contentDescription = "Delete ${t.text}"
                setOnClickListener {
                    prefs.todos = prefs.todos.filterIndexed { j, _ -> j != i }
                    build()
                }
            }
            row(box, remove, top = 0)
        }
        row(
            button("Add a to-do…") {
                ask("New to-do") { s ->
                    prefs.todos = prefs.todos + Todo(s, false)
                    build()
                }
            },
            *(if (todos.any { it.done }) arrayOf(button("Clear done") { prefs.todos = prefs.todos.filter { !it.done }; build() }) else emptyArray()),
        )
        note("The first unfinished one shows on the island's clock card.", 2)
        add(label("Quick notes", 15f, bold = true), 14)
        val notes = prefs.notes
        for (n in notes.takeLast(5).reversed()) note(n, 4)
        row(
            button("Add a note…") {
                ask("Quick note") { s ->
                    val stamp = android.text.format.DateFormat.format("d MMM HH:mm", System.currentTimeMillis())
                    prefs.notes = prefs.notes + "$stamp  $s"
                    build()
                }
            },
            *(
                if (notes.isNotEmpty()) {
                    arrayOf(
                        button("Share all") {
                            startActivity(
                                Intent.createChooser(
                                    Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, notes.joinToString("\n")),
                                    "Share notes",
                                ),
                            )
                        },
                        button("Delete all") { prefs.notes = emptyList(); build() },
                    )
                } else {
                    emptyArray()
                }
                ),
        )
    }

    private fun ask(title: String, done: (String) -> Unit) {
        val input = EditText(this).apply { inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES }
        AlertDialog.Builder(this)
            .setTitle(title)
            .setView(input)
            .setPositiveButton("Add") { _, _ -> input.text.toString().trim().takeIf { it.isNotEmpty() }?.let(done) }
            .setNegativeButton("Cancel", null)
            .show()
    }

    private fun controlsSection() {
        heading("Control centre")
        note(
            "Long-press the island: flashlight, Do Not Disturb, ring or vibrate, volume, brightness, screenshot, lock, " +
                "and shortcuts to your apps. Android doesn't let apps switch Wi-Fi, Bluetooth or airplane mode, so " +
                "those buttons open the phone's own panel.",
            2,
        )
        val names = prefs.shortcuts.map { Isle.appName(it) }
        if (names.isNotEmpty()) note("Shortcuts: ${names.joinToString(", ")}", 6)
        row(button("Choose shortcut apps (up to 4)…") { pickApps("Shortcuts", prefs.shortcuts.toSet(), 4) { prefs.shortcuts = it; build() } })
    }

    private fun musicSection() {
        heading("Music")
        note("Any app that plays music shows on the island, with its cover, controls and a progress bar you can drag.", 2)
        toggle("Synced lyrics on the island", { prefs.lyrics }) {
            prefs.lyrics = it
            Isle.media(Isle.controller)
        }
        note("Lyrics come from LRCLIB, a free service: Islet sends it the song's title, artist and length.", 2)
        if (prefs.lyrics) stepper("Lyrics timing", " ms", 250, { prefs.lyricsOffset.toInt() }) { prefs.lyricsOffset = it.toLong() }
    }

    private fun speedSection() {
        heading("Speed test")
        if (speedResult.isNotEmpty()) add(label(speedResult, 15f, bold = true), 4)
        row(
            button("Test speed") {
                speedResult = "Testing…"
                build()
                Isle.background {
                    val s = Net.speedTest()
                    runOnUiThread {
                        speedResult = s?.let { "${Format.mbps(it.downMbps)} Mbps down · ${Format.mbps(it.upMbps)} Mbps up · ${it.latencyMs.toInt()} ms" }
                            ?: "The test didn't finish. Check the connection."
                        build()
                    }
                }
            },
        )
        note("Uses about 35 MB with Cloudflare's public speed-test servers, only when you press the button.", 2)
    }

    // ---- The computer --------------------------------------------------------------------------

    private fun computerSection() {
        heading("Your computer")
        val pairing = Store.pairing(this)
        val computer = Hub.computer
        add(
            label(
                when {
                    pairing == null -> "Not paired"
                    !hasAccess() -> "Paired with ${pairing.name}"
                    computer != null -> "Connected to $computer"
                    else -> "Looking for ${pairing.name}…"
                },
                16f,
                bold = true,
            ),
            6,
        )
        note(
            when {
                pairing == null ->
                    "Show your phone's notifications, music and battery on Islet on your computer too. On the computer, open " +
                        "Islet Settings → Wi-Fi, Bluetooth & phone → Your Android phone and choose Pair a phone, then scan the code."
                computer != null -> "Your notifications, music and battery level show on the computer's island."
                !hasAccess() -> "Allow notification access above to start."
                else ->
                    "Make sure the computer is on, Islet is running and both are on the same Wi-Fi." +
                        (if (Hub.problem.isNotEmpty()) "\n\nLast try: ${Hub.problem}." else "")
            },
            4,
        )
        row(
            button(if (pairing == null) "Scan the code" else "Scan a new code", primary = pairing == null) {
                scan.launch(
                    ScanOptions()
                        .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                        .setPrompt("Point at the code in Islet Settings on your computer")
                        .setBeepEnabled(false)
                        .setOrientationLocked(false),
                )
            },
            button("Paste a code") { pasteCode() },
        )
        if (pairing != null) {
            toggle("Send notifications to the computer", { Store.sendNotes(this) }) { Store.setSendNotes(this, it) }
            toggle("Send what is playing to the computer", { Store.sendMedia(this) }) {
                Store.setSendMedia(this, it)
                IsletListener.instance?.reload()
            }
            row(
                button("Forget this computer") {
                    Store.forget(this)
                    IsletListener.instance?.reload()
                    build()
                },
            )
            note("Your phone talks straight to your computer over your Wi-Fi, encrypted with a key only the two hold.", 2)
        }
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
            message("That isn't an Islet pairing code. On your computer, open Islet Settings → Wi-Fi, Bluetooth & phone → Your Android phone.")
            return
        }
        Store.pair(this, pairing)
        IsletListener.instance?.reload()
        build()
        if (!hasAccess()) askForAccess()
    }

    private fun askForAccess() {
        AlertDialog.Builder(this)
            .setTitle("Allow notification access")
            .setMessage(
                "Islet needs notification access to show your notifications, music, calls and directions on the island " +
                    "(and on your computer, if you pair one). On the next screen, turn on Islet.\n\n" +
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
}
