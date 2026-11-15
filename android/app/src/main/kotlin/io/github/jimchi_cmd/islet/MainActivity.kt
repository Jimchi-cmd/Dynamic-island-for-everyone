package io.github.jimchi_cmd.islet

import android.Manifest
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.service.notification.NotificationListenerService
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import androidx.activity.OnBackPressedCallback
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import islet.island.Forecast
import islet.island.Format
import islet.island.IslandSize
import islet.island.Place
import islet.island.Todo
import islet.protocol.Pairing

/**
 * The app's screen, laid out like Islet's Settings on the computer: a list
 * of pages with coloured tiles, and on each page, grouped cards of rows.
 */
class MainActivity : AppCompatActivity() {
    private enum class Page(val title: String, val icon: Icon, val tint: IntArray) {
        Home("Islet", Icon.Gear, Tints.grey),
        Setup("Set up the island", Icon.Check, Tints.green),
        Look("Look", Icon.Brightness, Tints.grey),
        Shows("What it shows", Icon.Bell, Tints.red),
        Timers("Timer & focus", Icon.Timer, Tints.purple),
        Weather("Weather & clocks", Icon.Sun, Tints.cyan),
        Todo("To-do & notes", Icon.Note, Tints.yellow),
        Controls("Control centre", Icon.Flashlight, Tints.slate),
        Music("Music", Icon.Music, Tints.pink),
        Speed("Speed test", Icon.Download, Tints.green),
        Computer("Your computer", Icon.Wifi, Tints.blue),
    }

    private lateinit var prefs: Prefs
    private lateinit var kit: Kit
    private lateinit var scroll: ScrollView
    private var page = Page.Home
    private var speedResult = ""
    private val scrolls = HashMap<Page, Int>()

    private val scan = registerForActivityResult(ScanContract()) { result ->
        result.contents?.let { pairWith(it) }
    }

    private val askPermission = registerForActivityResult(ActivityResultContracts.RequestPermission()) {
        Sensors.refresh()
        build()
    }

    private val back = object : OnBackPressedCallback(false) {
        override fun handleOnBackPressed() = go(Page.Home)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Isle.init(this)
        prefs = Isle.prefs
        kit = Kit(this, Theme(this))
        WindowCompat.setDecorFitsSystemWindows(window, false)
        WindowCompat.getInsetsController(window, window.decorView).apply {
            isAppearanceLightStatusBars = !kit.theme.dark
            isAppearanceLightNavigationBars = !kit.theme.dark
        }
        window.decorView.setBackgroundColor(kit.theme.bg)
        scroll = ScrollView(this).apply {
            setBackgroundColor(kit.theme.bg)
            isFillViewport = true
            clipToPadding = false
        }
        // Keep clear of the status bar and the gesture bar (Android 15 draws apps edge to edge).
        ViewCompat.setOnApplyWindowInsetsListener(scroll) { v, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            v.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            insets
        }
        setContentView(scroll)
        onBackPressedDispatcher.addCallback(this, back)
        page = savedInstanceState?.getString("page")?.let { p -> Page.entries.firstOrNull { it.name == p } } ?: Page.Home
        handleLink(intent)
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        outState.putString("page", page.name)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleLink(intent)
    }

    /** The island's buttons open a page directly ("Todo", "Speed"). */
    private fun pageFromIntent() {
        val wanted = intent?.getStringExtra("page") ?: return
        intent?.removeExtra("page")
        Page.entries.firstOrNull { it.name == wanted }?.let { page = it }
    }

    override fun onStart() {
        super.onStart()
        Hub.onChange = { runOnUiThread { if (page == Page.Computer || page == Page.Home) build() } }
        if (hasAccess() && IsletListener.instance == null) {
            NotificationListenerService.requestRebind(ComponentName(this, IsletListener::class.java))
        }
        if (!IslandHost.hosted) IslandHost.rebuild()
        Sensors.refresh()
        pageFromIntent()
        build()
    }

    override fun onStop() {
        Hub.onChange = null
        super.onStop()
    }

    private fun go(to: Page) {
        scrolls[page] = scroll.scrollY
        page = to
        build()
        scroll.post { scroll.scrollTo(0, scrolls[to] ?: 0) }
    }

    /** Open a page by its name (for tests that draw each page). */
    internal fun show(name: String) = go(Page.valueOf(name))

    private fun granted(p: String) = ContextCompat.checkSelfPermission(this, p) == PackageManager.PERMISSION_GRANTED

    private fun hasAccess() = NotificationManagerCompat.getEnabledListenerPackages(this).contains(packageName)

    private fun changed() = Isle.changed()

    // ---- Building a page ------------------------------------------------------------------------

    private fun build() {
        val y = scroll.scrollY
        back.isEnabled = page != Page.Home
        val col = kit.column().apply { setPadding(kit.dp(16), kit.dp(8), kit.dp(16), kit.dp(36)) }
        if (page == Page.Home) header(col) else topBar(col)
        when (page) {
            Page.Home -> home(col)
            Page.Setup -> setup(col)
            Page.Look -> look(col)
            Page.Shows -> shows(col)
            Page.Timers -> timers(col)
            Page.Weather -> weather(col)
            Page.Todo -> todo(col)
            Page.Controls -> controls(col)
            Page.Music -> music(col)
            Page.Speed -> speed(col)
            Page.Computer -> computer(col)
        }
        scroll.removeAllViews()
        scroll.addView(col)
        scroll.post { scroll.scrollTo(0, y) }
    }

    /** A black pill with a camera dot: the app's mark. */
    private fun mark() = object : View(this) {
        private val p = Paint(Paint.ANTI_ALIAS_FLAG)

        override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(kit.dp(46), kit.dp(18))

        override fun onDraw(c: Canvas) {
            val h = height.toFloat()
            p.color = Color.BLACK
            c.drawRoundRect(RectF(0f, 0f, width.toFloat(), h), h / 2, h / 2, p)
            if (kit.theme.dark) {
                p.style = Paint.Style.STROKE
                p.strokeWidth = kit.dp(1).toFloat()
                p.color = 0x33ffffff
                c.drawRoundRect(RectF(0.5f, 0.5f, width - 0.5f, h - 0.5f), h / 2, h / 2, p)
                p.style = Paint.Style.FILL
            }
            p.color = 0xff3b93ff.toInt()
            c.drawCircle(width - h / 2 - kit.dp(2), h / 2, h * 0.2f, p)
        }
    }

    private fun header(col: LinearLayout) {
        val r = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(kit.dp(4), kit.dp(16), 0, kit.dp(4))
        }
        r.addView(mark())
        r.addView(kit.text("Islet", 30f, bold = true), LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { marginStart = kit.dp(12) })
        col.addView(r)
    }

    private fun topBar(col: LinearLayout) {
        val r = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(0, kit.dp(10), 0, kit.dp(2))
        }
        val backButton = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            val c = kit.chevron().apply { rotation = 180f }
            addView(c, LinearLayout.LayoutParams(kit.dp(22), kit.dp(22)))
            addView(kit.text("Islet", 17f, kit.theme.accent))
            setPadding(kit.dp(2), kit.dp(8), kit.dp(12), kit.dp(8))
            contentDescription = "Back"
            setOnClickListener { go(Page.Home) }
        }
        r.addView(backButton)
        col.addView(r)
        val title = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(kit.dp(4), kit.dp(4), 0, 0)
            addView(IconTile(this@MainActivity, page.icon, page.tint, 34f))
            addView(kit.text(page.title, 26f, bold = true), LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { marginStart = kit.dp(12) })
        }
        col.addView(title)
    }

    private fun tile(p: Page) = IconTile(this, p.icon, p.tint)

    private fun pageRow(c: Kit.Card, p: Page, note: String? = null) = c.add(kit.valueRow(p.title, note ?: "", tile = tile(p)) { go(p) })

    // ---- Home ------------------------------------------------------------------------------------

    private val setUp get() = hasAccess() && IslandHost.hosted

    private fun islandStatus(): Pair<String, Int> {
        val t = kit.theme
        return when {
            !prefs.islandOn -> "The island is off" to t.muted
            IslandHost.overAccessibility -> "Showing round the camera" to t.ok
            IslandHost.hosted -> "Showing round the camera. For taps on the small pill, turn on the accessibility switch." to t.warn
            !hasAccess() -> "Not set up yet" to t.warn
            else -> "One more step" to t.warn
        }
    }

    private fun home(col: LinearLayout) {
        val c = kit.card(col, top = 14)
        c.add(kit.toggleRow("Show the island", islandStatus().first, prefs.islandOn) {
            prefs.islandOn = it
            IslandHost.rebuild()
            build()
        })
        if (!setUp || !IslandHost.overAccessibility) {
            c.add(kit.valueRow("Set up", if (setUp) "Round the camera" else "2 steps", tile = tile(Page.Setup)) { go(Page.Setup) })
        }
        val pages = kit.card(col)
        pageRow(pages, Page.Look)
        pageRow(pages, Page.Shows)
        pageRow(pages, Page.Controls)
        val things = kit.card(col)
        pageRow(things, Page.Timers)
        pageRow(things, Page.Weather, prefs.place?.name?.substringBefore(',') ?: "")
        pageRow(things, Page.Todo, prefs.todos.count { !it.done }.takeIf { it > 0 }?.toString() ?: "")
        pageRow(things, Page.Music, if (prefs.lyrics) "Lyrics on" else "")
        pageRow(things, Page.Speed)
        val link = kit.card(col)
        val pairing = Store.pairing(this)
        pageRow(link, Page.Computer, when {
            pairing == null -> "Not paired"
            Hub.computer != null -> "Connected"
            else -> pairing.name
        })
        col.addView(
            kit.text(
                "Tap the island to open it, long-press for the control centre, swipe up to close it, and swipe sideways to " +
                    "switch between two things.\n\nIslet has no account, no ads and no tracking. It only goes online for what " +
                    "you switch on: weather for your place, lyrics, and the speed test.\n\nVersion ${versionName()}",
                13f,
                kit.theme.muted,
            ).apply { setPadding(kit.dp(16), kit.dp(14), kit.dp(16), 0) },
        )
    }

    private fun versionName(): String = try {
        packageManager.getPackageInfo(packageName, 0).versionName ?: ""
    } catch (e: Exception) {
        ""
    }

    // ---- Setup -----------------------------------------------------------------------------------

    private fun setup(col: LinearLayout) {
        val access = hasAccess()
        val accessibility = IslandAccessibility.instance != null
        val overlay = Settings.canDrawOverlays(this)
        val steps = kit.card(col, "Two steps", top = 14)
        steps.add(kit.row("Notification access", "For notifications, music, calls, directions and downloads.", kit.status(access), if (access) null else kit.button("Allow", primary = true) { askForAccess() }))
        steps.add(
            kit.row(
                "Round the camera",
                "Settings → Accessibility → Islet island. Puts the island over the status bar, round the front camera, where taps reach it. Islet reads nothing on your screen.",
                kit.status(accessibility),
                if (accessibility) null else kit.button("Open", primary = true) { startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)) },
            ),
        )
        if (!accessibility) {
            steps.add(
                kit.row(
                    "Or: display over other apps",
                    "Shows the island without the accessibility switch. It still sits round the camera, but on many phones the status bar then takes taps on the small pill.",
                    kit.status(overlay),
                    if (overlay) null else kit.button("Allow") { startActivity(Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, Uri.parse("package:$packageName"))) },
                ),
            )
        }
        col.addView(
            kit.text(
                "A switch greyed out? Open Settings → Apps → Islet, tap ⋮ at the top right, choose \"Allow restricted settings\", and try again. Android asks this of apps installed outside the Play Store.",
                13f,
                kit.theme.muted,
            ).apply { setPadding(kit.dp(16), kit.dp(6), kit.dp(16), 0) },
        )
        val extra = kit.card(col, "Optional")
        fun optional(p: String, title: String, note: String) {
            val ok = granted(p)
            extra.add(kit.row(title, note, kit.status(ok), if (ok) null else kit.button("Allow") { askPermission.launch(p) }))
        }
        optional(Manifest.permission.READ_CALENDAR, "Calendar", "Count down to meetings, with a Join button.")
        optional(
            if (Build.VERSION.SDK_INT >= 33) Manifest.permission.READ_MEDIA_IMAGES else Manifest.permission.READ_EXTERNAL_STORAGE,
            "Photos",
            "Show each new screenshot.",
        )
        if (Build.VERSION.SDK_INT >= 31) optional(Manifest.permission.BLUETOOTH_CONNECT, "Nearby devices", "Name the Bluetooth device that connected.")
        optional(Manifest.permission.ACCESS_FINE_LOCATION, "Location", "Android shares the Wi-Fi network's name only with apps allowed location. Islet uses it for nothing else.")
        val canWrite = Settings.System.canWrite(this)
        extra.add(
            kit.row(
                "Modify system settings",
                "The brightness slider in the control centre.",
                kit.status(canWrite),
                if (canWrite) null else kit.button("Allow") { startActivity(Intent(Settings.ACTION_MANAGE_WRITE_SETTINGS, Uri.parse("package:$packageName"))) },
            ),
        )
    }

    // ---- Look ------------------------------------------------------------------------------------

    private fun look(col: LinearLayout) {
        val style = kit.card(col, "Style", "Glass is frosted with a light rim. Android doesn't let the island blur what's behind it.", top = 14)
        style.add(kit.segmented(listOf("black" to "Black", "glass" to "Glass"), prefs.style) {
            prefs.style = it
            changed()
            build()
        })
        val size = kit.card(col, "Size")
        size.add(kit.segmented(IslandSize.entries.map { it.name to it.name }, prefs.size.name) {
            prefs.size = IslandSize.valueOf(it)
            changed()
            build()
        })
        val place = kit.card(col, "Position", "If the pill doesn't sit exactly round your camera, nudge it. Changes show on the island straight away.")
        place.add(kit.stepperRow("Left and right", value = { "${prefs.offsetX} dp" }) { prefs.offsetX += it * 2; changed() })
        place.add(kit.stepperRow("Up and down", value = { "${prefs.offsetY} dp" }) { prefs.offsetY += it; changed() })
        place.add(kit.stepperRow("Wider", value = { "${prefs.extraWidth} dp" }) { prefs.extraWidth += it * 4; changed() })
        val fonts = listOf("default" to "Default", "light" to "Light", "condensed" to "Condensed", "serif" to "Serif", "mono" to "Mono")
        val look = kit.card(col, "Island")
        look.add(kit.valueRow("Font", fonts.first { it.first == prefs.font || it.first == "default" && fonts.none { f -> f.first == prefs.font } }.second) {
            choose("Font", fonts, prefs.font) { prefs.font = it; changed(); build() }
        })
        look.add(kit.valueRow("Temperature", if (prefs.fahrenheit) "°F" else "°C") {
            choose("Temperature", listOf("c" to "Celsius (°C)", "f" to "Fahrenheit (°F)"), if (prefs.fahrenheit) "f" else "c") {
                prefs.fahrenheit = it == "f"
                changed()
                build()
            }
        })
        look.add(kit.toggleRow("Show the empty island", "The black pill round the camera when nothing is going on.", prefs.showEmpty) { prefs.showEmpty = it; changed() })
        look.add(kit.toggleRow("Glowing rim", "A soft light round the island's edge.", prefs.ring) { prefs.ring = it; changed() })
        look.add(kit.toggleRow("Border in the album's colour while music plays", null, prefs.albumBorder) { prefs.albumBorder = it; changed() })
        look.add(kit.toggleRow("Moving bars beside the song", null, prefs.bars) { prefs.bars = it; changed() })
        look.add(kit.toggleRow("Airplane mode and Do Not Disturb icons", "A plane or moon on the empty island while they're on.", prefs.modeIcons) { prefs.modeIcons = it; changed() })
        look.add(kit.toggleRow("Camera and microphone dots", "Green and orange dots while an app uses them.", prefs.privacyDots) { prefs.privacyDots = it; changed() })
        look.add(kit.toggleRow("Hide in landscape", "For videos and games.", prefs.hideInLandscape) { prefs.hideInLandscape = it; changed() })
    }

    private fun choose(title: String, options: List<Pair<String, String>>, current: String, pick: (String) -> Unit) {
        AlertDialog.Builder(this)
            .setTitle(title)
            .setSingleChoiceItems(options.map { it.second }.toTypedArray(), options.indexOfFirst { it.first == current }) { d, i ->
                pick(options[i].first)
                d.dismiss()
            }
            .show()
    }

    // ---- What it shows ---------------------------------------------------------------------------

    private fun shows(col: LinearLayout) {
        val live = kit.card(col, "On the island", top = 14)
        live.add(kit.toggleRow("Calls", "With the phone app's answer and hang-up buttons.", prefs.calls) { prefs.calls = it; changed() })
        live.add(kit.toggleRow("Directions", "From Google Maps, Waze and other maps apps.", prefs.navigation) { prefs.navigation = it; changed() })
        live.add(kit.toggleRow("Downloads and progress", null, prefs.progress) { prefs.progress = it; changed() })
        live.add(kit.toggleRow("Meetings", "A countdown from 15 minutes before, with a Join button.", prefs.calendar) {
            prefs.calendar = it
            Sensors.refresh()
        })
        val pops = kit.card(col, "Pop-ups")
        pops.add(kit.toggleRow("Notifications", "On the lock screen only the app's name shows.", prefs.popNotes) { prefs.popNotes = it })
        pops.add(kit.toggleRow("Charging and low battery", null, prefs.popPower) { prefs.popPower = it })
        pops.add(kit.toggleRow("Ring, vibrate and silent", "Also airplane mode and Do Not Disturb.", prefs.popModes) { prefs.popModes = it })
        pops.add(kit.toggleRow("Wi-Fi and Bluetooth", null, prefs.popConnections) { prefs.popConnections = it })
        pops.add(kit.toggleRow("Screenshots", null, prefs.popScreenshots) { prefs.popScreenshots = it })
        val quiet = prefs.quietApps
        pops.add(kit.valueRow("Apps that never pop up", if (quiet.isEmpty()) "None" else quiet.size.toString()) { pickQuietApps() })
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
                if (on && checked.count { it } >= limit) {
                    (d as AlertDialog).listView.setItemChecked(i, false)
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

    // ---- Timers ----------------------------------------------------------------------------------

    private fun timers(col: LinearLayout) {
        val start = kit.card(col, "Timer", "Also on the island: open it while it's empty for 1, 5, 10 and 25-minute timers.", top = 14)
        start.add(
            kit.buttons(
                kit.button("1 min") { Isle.startFocus(1.0) },
                kit.button("5 min") { Isle.startFocus(5.0) },
                kit.button("10 min") { Isle.startFocus(10.0) },
                kit.button("25 min") { Isle.startFocus(25.0) },
                kit.button("Stop") { Isle.focus.stop(); Isle.syncTimers() },
            ),
        )
        val focus = kit.card(col, "Focus rounds", "Focus, a short break, and a longer break after every fourth round. A buzz and a sound when each part ends.")
        focus.add(kit.stepperRow("Focus", value = { "${prefs.focusMinutes} min" }) { prefs.focusMinutes += it * 5 })
        focus.add(kit.stepperRow("Break", value = { "${prefs.breakMinutes} min" }) { prefs.breakMinutes += it })
        focus.add(kit.buttons(kit.button("Start focus rounds", primary = true) { Isle.startPomodoro() }))
        val sw = kit.card(col, "Stopwatch")
        sw.add(
            kit.buttons(
                kit.button("Start", primary = true) { Isle.stopwatch.start(Isle.now()); Isle.syncTimers() },
                kit.button("Lap") { Isle.stopwatch.lap(Isle.now()); Isle.syncTimers() },
                kit.button("Pause") { Isle.stopwatch.pause(Isle.now()); Isle.syncTimers() },
                kit.button("Reset") { Isle.stopwatch.reset(); Isle.syncTimers() },
            ),
        )
    }

    // ---- Weather and clocks -----------------------------------------------------------------------

    private fun weather(col: LinearLayout) {
        val place = prefs.place
        val w = Isle.weather
        val c = kit.card(col, "Weather", "From Open-Meteo, free and without an account. Islet sends it only your place's coordinates, at most every half hour.", top = 14)
        c.add(kit.valueRow("Place", place?.name?.substringBefore(',') ?: "Choose…") { searchPlace() })
        if (place != null && w != null) c.add(kit.row("Now", "${w.words}${w.highC?.let { " · high ${Format.temperature(it, prefs.fahrenheit)}" } ?: ""}", right = kit.text(Format.temperature(w.celsius, prefs.fahrenheit), 22f, bold = true)))
        if (place != null) c.add(kit.buttons(kit.button("Remove") { prefs.place = null; Isle.weather = null; build() }))
        val clocks = kit.card(col, "World clocks", "Up to three, on the island's clock card.")
        for (zone in prefs.worldClocks) {
            clocks.add(kit.row(zone.substringAfterLast('/').replace('_', ' '), zone.substringBefore('/'), right = kit.button("Remove") {
                prefs.worldClocks = prefs.worldClocks - zone
                build()
            }))
        }
        if (prefs.worldClocks.size < 3) clocks.add(kit.valueRow("Add a city", "") { addClock() })
    }

    private fun searchPlace() {
        val input = EditText(this).apply {
            hint = "City or town"
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_WORDS
        }
        AlertDialog.Builder(this)
            .setTitle("Weather for…")
            .setView(padded(input))
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

    private fun padded(v: View) = android.widget.FrameLayout(this).apply {
        setPadding(kit.dp(22), kit.dp(8), kit.dp(22), 0)
        addView(v)
    }

    private fun choosePlace(found: List<Place>) {
        if (found.isEmpty()) {
            message("No place by that name (or no connection).")
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

    // ---- To-do and notes ---------------------------------------------------------------------------

    private fun todo(col: LinearLayout) {
        val todos = prefs.todos
        val list = kit.card(col, "To-do", "The first unfinished one shows on the island's clock card.", top = 14)
        for ((i, t) in todos.withIndex()) {
            val tick = kit.status(t.done).apply {
                setOnClickListener {
                    prefs.todos = prefs.todos.toMutableList().also { it[i] = Todo(t.text, !t.done) }
                    build()
                }
                contentDescription = if (t.done) "Done" else "Not done"
            }
            list.add(kit.row(t.text, null, tick, kit.button("Delete") {
                prefs.todos = prefs.todos.filterIndexed { j, _ -> j != i }
                build()
            }))
        }
        list.add(
            kit.buttons(
                kit.button("Add a to-do", primary = true) { ask("New to-do") { s -> prefs.todos = prefs.todos + Todo(s, false); build() } },
                *(if (todos.any { it.done }) arrayOf(kit.button("Clear done") { prefs.todos = prefs.todos.filter { !it.done }; build() }) else emptyArray()),
            ),
        )
        val notes = prefs.notes
        val n = kit.card(col, "Quick notes", "One line at a time, with the date and time.")
        for (line in notes.takeLast(8).reversed()) n.add(kit.para(line, kit.theme.text, 15f))
        n.add(
            kit.buttons(
                kit.button("Add a note", primary = true) {
                    ask("Quick note") { s ->
                        val stamp = android.text.format.DateFormat.format("d MMM HH:mm", System.currentTimeMillis())
                        prefs.notes = prefs.notes + "$stamp  $s"
                        build()
                    }
                },
                *(
                    if (notes.isNotEmpty()) {
                        arrayOf(
                            kit.button("Share all") {
                                startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, notes.joinToString("\n")), "Share notes"))
                            },
                            kit.button("Delete all") { prefs.notes = emptyList(); build() },
                        )
                    } else {
                        emptyArray()
                    }
                    ),
            ),
        )
    }

    private fun ask(title: String, done: (String) -> Unit) {
        val input = EditText(this).apply { inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES }
        AlertDialog.Builder(this)
            .setTitle(title)
            .setView(padded(input))
            .setPositiveButton("Add") { _, _ -> input.text.toString().trim().takeIf { it.isNotEmpty() }?.let(done) }
            .setNegativeButton("Cancel", null)
            .show()
    }

    // ---- Control centre ------------------------------------------------------------------------------

    private fun controls(col: LinearLayout) {
        col.addView(
            kit.text(
                "Long-press the island: flashlight, Do Not Disturb, ring or vibrate, volume, brightness, screenshot, lock and your shortcut apps. Android doesn't let apps switch Wi-Fi, Bluetooth or airplane mode, so those buttons open the phone's own panel.",
                14f,
                kit.theme.muted,
            ).apply { setPadding(kit.dp(16), kit.dp(14), kit.dp(16), 0) },
        )
        val c = kit.card(col, "Shortcuts")
        val names = prefs.shortcuts.map { Isle.appName(it) }
        c.add(kit.valueRow("Apps (up to 4)", if (names.isEmpty()) "None" else names.joinToString(", ")) {
            pickApps("Shortcuts", prefs.shortcuts.toSet(), 4) {
                prefs.shortcuts = it
                build()
            }
        })
    }

    // ---- Music --------------------------------------------------------------------------------------

    private fun music(col: LinearLayout) {
        col.addView(
            kit.text("Any app that plays music shows on the island, with its cover, controls and a progress bar you can drag.", 14f, kit.theme.muted)
                .apply { setPadding(kit.dp(16), kit.dp(14), kit.dp(16), 0) },
        )
        val c = kit.card(col, "Lyrics", "Lyrics come from LRCLIB, a free service: Islet sends it the song's title, artist and length.")
        c.add(kit.toggleRow("Synced lyrics on the island", null, prefs.lyrics) {
            prefs.lyrics = it
            Isle.media(Isle.controller)
            build()
        })
        if (prefs.lyrics) c.add(kit.stepperRow("Timing", "If the words run early or late.", value = { "${prefs.lyricsOffset} ms" }) { prefs.lyricsOffset += it * 250L })
    }

    // ---- Speed test ------------------------------------------------------------------------------------

    private fun speed(col: LinearLayout) {
        val c = kit.card(col, null, "Uses about 35 MB with Cloudflare's public speed-test servers, only when you press the button.", top = 14)
        if (speedResult.isNotEmpty()) c.add(kit.para(speedResult, kit.theme.text, 17f))
        c.add(
            kit.buttons(
                kit.button("Test speed", primary = true) {
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
            ),
        )
    }

    // ---- The computer ----------------------------------------------------------------------------------

    private fun computer(col: LinearLayout) {
        val pairing = Store.pairing(this)
        val computer = Hub.computer
        val c = kit.card(col, null, top = 14)
        c.add(
            kit.row(
                when {
                    pairing == null -> "Not paired"
                    !hasAccess() -> "Paired with ${pairing.name}"
                    computer != null -> "Connected to $computer"
                    else -> "Looking for ${pairing.name}…"
                },
                when {
                    pairing == null ->
                        "Show your phone's notifications, music and battery on Islet on your computer too. On the computer, open " +
                            "Islet Settings → Wi-Fi, Bluetooth & phone → Your Android phone → Pair a phone, then scan the code."
                    computer != null -> "Your notifications, music and battery level show on the computer's island."
                    !hasAccess() -> "Allow notification access first (Set up)."
                    else ->
                        "Make sure the computer is on, Islet is running and both are on the same Wi-Fi." +
                            (if (Hub.problem.isNotEmpty()) "\nLast try: ${Hub.problem}." else "")
                },
                kit.status(computer != null),
            ),
        )
        c.add(
            kit.buttons(
                kit.button(if (pairing == null) "Scan the code" else "Scan a new code", primary = pairing == null) {
                    scan.launch(
                        ScanOptions()
                            .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                            .setPrompt("Point at the code in Islet Settings on your computer")
                            .setBeepEnabled(false)
                            .setOrientationLocked(false),
                    )
                },
                kit.button("Paste a code") { pasteCode() },
            ),
        )
        if (pairing != null) {
            val s = kit.card(col, "Send to the computer", "Your phone talks straight to your computer over your Wi-Fi, encrypted with a key only the two hold.")
            s.add(kit.toggleRow("Notifications", null, Store.sendNotes(this)) { Store.setSendNotes(this, it) })
            s.add(kit.toggleRow("What is playing", null, Store.sendMedia(this)) {
                Store.setSendMedia(this, it)
                IsletListener.instance?.reload()
            })
            s.add(kit.buttons(kit.button("Forget this computer") {
                Store.forget(this)
                IsletListener.instance?.reload()
                build()
            }))
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
        page = Page.Computer
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
            .setView(padded(input))
            .setPositiveButton("Pair") { _, _ -> pairWith(input.text.toString()) }
            .setNegativeButton("Cancel", null)
            .show()
    }
}
