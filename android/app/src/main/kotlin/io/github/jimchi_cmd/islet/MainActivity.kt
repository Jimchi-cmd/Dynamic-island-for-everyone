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
import islet.island.ControlCentre
import islet.island.Forecast
import islet.island.Format
import islet.island.IslandSize
import islet.island.Deadlines
import islet.island.Launcher
import islet.island.Link
import islet.island.Place
import islet.island.Streak
import islet.island.Timetable
import islet.island.Todo
import islet.protocol.Pairing

/**
 * The app's screen, laid out like Islet's Settings on the computer: a list
 * of pages with coloured tiles, and on each page, grouped cards of rows.
 */
class MainActivity : AppCompatActivity() {
    private enum class Page(val title: String, val icon: Icon, val tint: IntArray) {
        Home("Islet", Icon.Gear, Tints.grey),
        Setup(T.t("Set up the island"), Icon.Check, Tints.green),
        Look(T.t("Look"), Icon.Brightness, Tints.grey),
        Shows(T.t("What it shows"), Icon.Bell, Tints.red),
        Timers(T.t("Timer & focus"), Icon.Timer, Tints.purple),
        Alarms(T.t("Alarms"), Icon.Bell, Tints.orange),
        Study(T.t("Classes & deadlines"), Icon.Book, Tints.cyan),
        Weather(T.t("Weather & clocks"), Icon.Sun, Tints.cyan),
        Todo(T.t("To-do & notes"), Icon.Note, Tints.yellow),
        Controls(T.t("Control centre"), Icon.Flashlight, Tints.slate),
        Music(T.t("Music"), Icon.Music, Tints.pink),
        Speed(T.t("Speed test"), Icon.Download, Tints.green),
        Battery(T.t("Battery"), Icon.Battery, Tints.green),
        Launch(T.t("Quick launcher"), Icon.Search, Tints.blue),
        Computer(T.t("Your computer"), Icon.Wifi, Tints.blue),
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

    /** A photo for behind Control Centre: shrunk and kept in the app's own files. */
    private val pickPhoto = registerForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri == null) return@registerForActivityResult
        val saved = runCatching { savePhoto(uri) }.getOrDefault(false)
        if (saved) {
            prefs.ccPhoto = true
            changed()
        } else {
            android.widget.Toast.makeText(this, T.t("That isn't a picture Islet can read."), android.widget.Toast.LENGTH_SHORT).show()
        }
        build()
    }

    private fun savePhoto(uri: android.net.Uri): Boolean {
        val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
        contentResolver.openInputStream(uri)?.use { android.graphics.BitmapFactory.decodeStream(it, null, bounds) }
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return false
        // About twice the card's size is plenty: decode it smaller, then shrink.
        var sample = 1
        while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= 1200) sample *= 2
        val opts = android.graphics.BitmapFactory.Options().apply { inSampleSize = sample }
        val raw = contentResolver.openInputStream(uri)?.use { android.graphics.BitmapFactory.decodeStream(it, null, opts) } ?: return false
        val k = minOf(1f, 1200f / maxOf(raw.width, raw.height))
        val bmp = if (k < 1f) android.graphics.Bitmap.createScaledBitmap(raw, (raw.width * k).toInt().coerceAtLeast(1), (raw.height * k).toInt().coerceAtLeast(1), true) else raw
        java.io.File(filesDir, IslandView.CC_PHOTO).outputStream().use { bmp.compress(android.graphics.Bitmap.CompressFormat.JPEG, 88, it) }
        return true
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
            Page.Alarms -> alarms(col)
            Page.Study -> study(col)
            Page.Battery -> battery(col)
            Page.Launch -> launch(col)
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
            contentDescription = T.t("Back")
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
            !prefs.islandOn -> T.t("The island is off") to t.muted
            IslandHost.overAccessibility -> T.t("Showing round the camera") to t.ok
            IslandAccessibility.switchedOn(this) -> T.t("Your phone stopped the accessibility switch: open Set up to fix it") to t.warn
            IslandHost.hosted -> T.t("Showing round the camera. For taps on the small pill, turn on the accessibility switch.") to t.warn
            !hasAccess() -> T.t("Not set up yet") to t.warn
            else -> T.t("One more step") to t.warn
        }
    }

    private fun home(col: LinearLayout) {
        val c = kit.card(col, top = 14)
        c.add(kit.toggleRow(T.t("Show the island"), islandStatus().first, prefs.islandOn) {
            prefs.islandOn = it
            IslandHost.rebuild()
            build()
        })
        if (!setUp || !IslandHost.overAccessibility) {
            c.add(kit.valueRow(T.t("Set up"), if (setUp) T.t("Round the camera") else "2 steps", tile = tile(Page.Setup)) { go(Page.Setup) })
        }
        val pages = kit.card(col)
        pageRow(pages, Page.Look)
        pageRow(pages, Page.Shows)
        pageRow(pages, Page.Controls)
        val things = kit.card(col)
        pageRow(things, Page.Timers, prefs.streak.label(Streak.dayOf(System.currentTimeMillis())) ?: "")
        pageRow(things, Page.Alarms, Isle.nextAlarm?.let { android.text.format.DateFormat.getTimeFormat(this).format(java.util.Date(it)) } ?: "")
        pageRow(things, Page.Study, prefs.timetable.lessons.size.takeIf { it > 0 }?.let { T.f("{} lessons", it) } ?: "")
        pageRow(things, Page.Weather, prefs.place?.name?.substringBefore(',') ?: "")
        pageRow(things, Page.Todo, prefs.todos.count { !it.done }.takeIf { it > 0 }?.toString() ?: "")
        pageRow(things, Page.Music, if (prefs.lyrics) T.t("Lyrics on") else "")
        pageRow(things, Page.Speed)
        pageRow(things, Page.Battery, if (Isle.battery >= 0) "${Isle.battery}%" else "")
        pageRow(things, Page.Launch, prefs.links.size.takeIf { it > 0 }?.let { T.f("{} links", it) } ?: "")
        val link = kit.card(col)
        val pairing = Store.pairing(this)
        pageRow(link, Page.Computer, when {
            pairing == null -> T.t("Not paired")
            Hub.computer != null -> T.t("Connected")
            else -> pairing.name
        })
        col.addView(
            kit.text(
                T.t("Tap the island to open it, long-press for the control centre, swipe up to close it, and swipe sideways to switch between two things.") + "\n\n" + T.t("Islet has no account, no ads and no tracking. It only goes online for what you switch on: weather for your place, lyrics, and the speed test.") + "\n\n" + T.f("Version {}", versionName()),
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
        val steps = kit.card(col, T.t("Two steps"), top = 14)
        steps.add(kit.row(T.t("Notification access"), T.t("For notifications, music, calls, directions and downloads."), kit.status(access), if (access) null else kit.button(T.t("Allow"), primary = true) { askForAccess() }))
        // On in the settings but not running: the phone stopped it.
        val stopped = !accessibility && IslandAccessibility.switchedOn(this)
        steps.add(
            kit.row(
                if (stopped) T.t("Round the camera: stopped by your phone") else T.t("Round the camera"),
                if (stopped) {
                    T.t("It's switched on, but Android isn't running it, so taps on the top of the island, the lock and screenshot buttons and the smoothest motion don't work. Turn Islet island off and on again, and set Islet's battery use to Unrestricted so the phone doesn't stop it again.")
                } else {
                    T.t("Settings → Accessibility → Islet island. Puts the island over the status bar, round the front camera, where taps reach it, and makes the lock and screenshot buttons work. Islet reads nothing on your screen.")
                },
                kit.status(accessibility),
                if (accessibility) null else kit.button(T.t("Open"), primary = true) { startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)) },
            ),
        )
        if (stopped) {
            steps.add(kit.row(T.t("Battery use"), T.t("Settings → Apps → Islet → Battery → Unrestricted."), right = kit.button(T.t("Open")) {
                startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName")))
            }))
        }
        if (!accessibility) {
            steps.add(
                kit.row(
                    T.t("Or: display over other apps"),
                    T.t("Shows the island without the accessibility switch. It still sits round the camera, but on many phones the status bar then takes taps on the small pill."),
                    kit.status(overlay),
                    if (overlay) null else kit.button(T.t("Allow")) { startActivity(Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, Uri.parse("package:$packageName"))) },
                ),
            )
        }
        col.addView(
            kit.text(
                T.t("A switch greyed out? Open Settings → Apps → Islet, tap ⋮ at the top right, choose \"Allow restricted settings\", and try again. Android asks this of apps installed outside the Play Store."),
                13f,
                kit.theme.muted,
            ).apply { setPadding(kit.dp(16), kit.dp(6), kit.dp(16), 0) },
        )
        val extra = kit.card(col, T.t("Optional"))
        fun optional(p: String, title: String, note: String) {
            val ok = granted(p)
            extra.add(kit.row(title, note, kit.status(ok), if (ok) null else kit.button(T.t("Allow")) { askPermission.launch(p) }))
        }
        optional(Manifest.permission.READ_CALENDAR, T.t("Calendar"), T.t("Count down to meetings, with a Join button."))
        optional(
            if (Build.VERSION.SDK_INT >= 33) Manifest.permission.READ_MEDIA_IMAGES else Manifest.permission.READ_EXTERNAL_STORAGE,
            T.t("Photos"),
            T.t("Show each new screenshot."),
        )
        if (Build.VERSION.SDK_INT >= 31) optional(Manifest.permission.BLUETOOTH_CONNECT, T.t("Nearby devices"), T.t("Name the Bluetooth device that connected."))
        optional(Manifest.permission.ACCESS_FINE_LOCATION, T.t("Location"), T.t("Android shares the Wi-Fi network's name only with apps allowed location. Islet uses it for nothing else."))
        val canWrite = Settings.System.canWrite(this)
        extra.add(
            kit.row(
                T.t("Modify system settings"),
                T.t("The brightness slider in the control centre."),
                kit.status(canWrite),
                if (canWrite) null else kit.button(T.t("Allow")) { startActivity(Intent(Settings.ACTION_MANAGE_WRITE_SETTINGS, Uri.parse("package:$packageName"))) },
            ),
        )
    }

    // ---- Look ------------------------------------------------------------------------------------

    private fun look(col: LinearLayout) {
        val lang = kit.card(col, T.t("Language"), T.t("Islet, the island and these settings, in your language."), top = 14)
        val langs = T.LANGUAGES.map { (code, name) -> code to if (code == "auto") T.t(name) else name }
        lang.add(kit.valueRow(T.t("Language"), langs.firstOrNull { it.first == prefs.language }?.second ?: T.t("Automatic")) {
            choose(T.t("Language"), langs, prefs.language) {
                prefs.language = it
                T.load(this, it)
                changed()
                recreate()
            }
        })
        val style = kit.card(col, T.t("Style"), T.t("Tinted is a dark shade of the accent colour. Glass is frosted with a light rim; Android doesn't let the island blur what's behind it."), top = 14)
        style.add(kit.segmented(listOf("black" to T.t("Black"), "tinted" to T.t("Tinted"), "glass" to T.t("Glass")), prefs.style) {
            prefs.style = it
            changed()
            build()
        })
        if (prefs.style == "tinted") {
            val tints = listOf(
                "" to T.t("Same as accent"),
                "#d6336c" to T.t("Pink"),
                "#e03131" to T.t("Red"),
                "#f76707" to T.t("Orange"),
                "#f59f00" to T.t("Yellow"),
                "#2f9e44" to T.t("Green"),
                "#0c8599" to T.t("Teal"),
                "#1971c2" to T.t("Blue"),
                "#4263eb" to T.t("Indigo"),
                "#7048e8" to T.t("Purple"),
                "#868e96" to T.t("Graphite"),
                "custom" to T.t("Your own…"),
            )
            val now = tints.firstOrNull { it.first.equals(prefs.tint, ignoreCase = true) }?.second ?: prefs.tint
            style.add(kit.valueRow(T.t("Tint colour"), now) {
                choose(T.t("Tint colour"), tints, prefs.tint) { pick ->
                    if (pick == "custom") {
                        askTint()
                    } else {
                        prefs.tint = pick
                        changed()
                        build()
                    }
                }
            })
        }
        val accents = listOf("purple" to T.t("Purple"), "blue" to T.t("Blue"), "pink" to T.t("Pink"), "green" to T.t("Green"), "orange" to T.t("Orange"), "white" to T.t("White"))
        style.add(kit.valueRow(T.t("Accent colour"), accents.firstOrNull { it.first == prefs.accent }?.second ?: T.t("Purple")) {
            choose(T.t("Accent colour"), accents, prefs.accent) { prefs.accent = it; changed(); build() }
        })
        val faces = listOf("time" to "14:05", "day" to T.t("Wed 14:05"), "seconds" to "14:05:09")
        style.add(kit.valueRow(T.t("Clock on the empty island"), faces.firstOrNull { it.first == prefs.clockFace }?.second ?: "14:05") {
            choose(T.t("Clock on the empty island"), faces, prefs.clockFace) { prefs.clockFace = it; changed(); build() }
        })
        style.add(kit.toggleRow(T.t("Notch shape"), T.t("Square top corners joined to the top of the screen, so the island grows out of the edge."), prefs.notch) { prefs.notch = it; changed() })
        controlCentre(col)
        val size = kit.card(col, T.t("Size"))
        size.add(kit.segmented(IslandSize.entries.map { it.name to it.name }, prefs.size.name) {
            prefs.size = IslandSize.valueOf(it)
            changed()
            build()
        })
        val place = kit.card(col, T.t("Position"), T.t("If the pill doesn't sit exactly round your camera, nudge it. Changes show on the island straight away."))
        place.add(kit.stepperRow(T.t("Left and right"), value = { "${prefs.offsetX} dp" }) { prefs.offsetX += it * 2; changed() })
        place.add(kit.stepperRow(T.t("Up and down"), value = { "${prefs.offsetY} dp" }) { prefs.offsetY += it; changed() })
        place.add(kit.stepperRow(T.t("Wider"), value = { "${prefs.extraWidth} dp" }) { prefs.extraWidth += it * 4; changed() })
        val fonts = listOf("inter" to T.t("Inter (as on the computer)"), "default" to T.t("The phone's own"), "light" to T.t("Light"), "condensed" to T.t("Condensed"), "serif" to T.t("Serif"), "mono" to T.t("Mono"))
        val look = kit.card(col, T.t("Island"))
        look.add(kit.valueRow(T.t("Font"), fonts.first { it.first == prefs.font || it.first == "inter" && fonts.none { f -> f.first == prefs.font } }.second) {
            choose(T.t("Font"), fonts, prefs.font) { prefs.font = it; changed(); build() }
        })
        look.add(kit.valueRow(T.t("Temperature"), if (prefs.fahrenheit) "°F" else "°C") {
            choose(T.t("Temperature"), listOf("c" to T.t("Celsius (°C)"), "f" to T.t("Fahrenheit (°F)")), if (prefs.fahrenheit) "f" else "c") {
                prefs.fahrenheit = it == "f"
                changed()
                build()
            }
        })
        look.add(kit.toggleRow(T.t("Show the empty island"), T.t("The black pill round the camera when nothing is going on."), prefs.showEmpty) { prefs.showEmpty = it; changed() })
        look.add(kit.toggleRow(T.t("Time on the empty island"), T.t("The time left of the camera and the battery right of it, when nothing is playing."), prefs.idleClock) { prefs.idleClock = it; changed() })
        look.add(kit.toggleRow(T.t("Network speed in the strip"), T.t("The strip from a tap on the empty island shows download speed instead of memory use."), prefs.stripNetwork) { prefs.stripNetwork = it; changed() })
        look.add(kit.toggleRow(T.t("Glowing rim"), T.t("A soft light round the island's edge."), prefs.ring) { prefs.ring = it; changed() })
        look.add(kit.toggleRow(T.t("Border in the album's colour while music plays"), null, prefs.albumBorder) { prefs.albumBorder = it; changed() })
        look.add(kit.toggleRow(T.t("Moving bars beside the song"), null, prefs.bars) { prefs.bars = it; changed() })
        look.add(kit.toggleRow(T.t("Airplane mode and Do Not Disturb icons"), T.t("A plane or moon on the empty island while they're on."), prefs.modeIcons) { prefs.modeIcons = it; changed() })
        look.add(kit.toggleRow(T.t("Camera and microphone dots"), T.t("Green and orange dots while an app uses them."), prefs.privacyDots) { prefs.privacyDots = it; changed() })
        look.add(kit.toggleRow(T.t("Hide in landscape"), T.t("For videos and games."), prefs.hideInLandscape) { prefs.hideInLandscape = it; changed() })
    }

    /** A tint colour typed as #rrggbb. */
    private fun askTint() {
        val field = android.widget.EditText(this).apply {
            hint = "#3a7bd5"
            setText(prefs.tint)
            setSingleLine()
        }
        AlertDialog.Builder(this)
            .setTitle(T.t("Tint colour"))
            .setView(field)
            .setPositiveButton(T.t("Save")) { _, _ ->
                val rgb = IslandView.tintOf(field.text.toString())
                if (rgb != null) {
                    prefs.tint = String.format("#%06x", rgb)
                    changed()
                    build()
                }
            }
            .setNegativeButton(T.t("Cancel"), null)
            .show()
    }

    /** Control Centre: its layout, a photo behind it, and the order of its buttons. */
    private fun controlCentre(col: LinearLayout) {
        val cc = kit.card(col, T.t("Control Centre"), T.t("The card that opens when you tap the island."), top = 14)
        val names = listOf("classic" to T.t("Classic"), "modules" to T.t("Modules"), "sliders" to T.t("Sliders"), "dock" to T.t("Dock"))
        cc.add(kit.segmented(names, prefs.ccStyle) {
            prefs.ccStyle = it
            changed()
            build()
        })
        val hints = mapOf(
            "classic" to T.t("The time, Wi-Fi and Bluetooth with their names, the tools in a row"),
            "modules" to T.t("A block of round buttons, with search beside it"),
            "sliders" to T.t("Volume and brightness as tall sliders beside the buttons"),
            "dock" to T.t("A big clock, with every button in one labelled row"),
        )
        hints[prefs.ccStyle]?.let { cc.add(kit.para(it)) }
        if (prefs.ccPhoto) {
            cc.add(kit.row(T.t("Background photo"), T.t("Darkened so the buttons stay easy to read. It stays on this phone."), right = kit.buttons(
                kit.button(T.t("Change…")) { pickPhoto.launch("image/*") },
                kit.button(T.t("Remove")) {
                    prefs.ccPhoto = false
                    java.io.File(filesDir, IslandView.CC_PHOTO).delete()
                    changed()
                    build()
                },
            )))
        } else {
            cc.add(kit.valueRow(T.t("Background photo"), T.t("None"), T.t("Darkened so the buttons stay easy to read. It stays on this phone.")) {
                pickPhoto.launch("image/*")
            })
        }
        val order = kit.card(col, T.t("Button order"), T.t("Every layout places Control Centre's buttons in this order; when a layout has no room for them all, the ones at the end are left out."), top = 14)
        val ids = ControlCentre.order(prefs.ccOrder)
        val labels = mapOf(
            "wifi" to T.t("Wi-Fi"),
            "bluetooth" to T.t("Bluetooth"),
            "search" to T.t("Search"),
            "todo" to T.t("To-do list"),
            "speed" to T.t("Speed test"),
            "controls" to T.t("Control centre"),
            "calendar" to T.t("Calendar"),
            "screenshot" to T.t("Screenshot"),
            "lock" to T.t("Lock"),
        )
        ids.forEachIndexed { i, id ->
            val name = labels[id] ?: id
            fun arrow(label: String, by: Int, enabled: Boolean) = kit.text(label, 18f, if (enabled) kit.theme.text else kit.theme.muted, bold = true).apply {
                gravity = android.view.Gravity.CENTER
                minWidth = kit.dp(40)
                minHeight = kit.dp(36)
                contentDescription = if (by < 0) T.f("Move {} up", name) else T.f("Move {} down", name)
                isEnabled = enabled
                setOnClickListener {
                    prefs.ccOrder = ControlCentre.moved(ids, id, by)
                    changed()
                    build()
                }
            }
            val arrows = LinearLayout(this).apply {
                orientation = LinearLayout.HORIZONTAL
                addView(arrow("↑", -1, i > 0))
                addView(arrow("↓", 1, i < ids.size - 1))
            }
            order.add(kit.row("${i + 1}.  $name", right = arrows))
        }
        if (prefs.ccOrder.isNotEmpty()) {
            order.add(kit.buttons(kit.button(T.t("Usual order")) {
                prefs.ccOrder = emptyList()
                changed()
                build()
            }))
        }
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
        val live = kit.card(col, T.t("On the island"), top = 14)
        live.add(kit.toggleRow(T.t("Calls"), T.t("With the phone app's answer and hang-up buttons."), prefs.calls) { prefs.calls = it; changed() })
        live.add(kit.toggleRow(T.t("Directions"), T.t("From Google Maps, Waze and other maps apps."), prefs.navigation) { prefs.navigation = it; changed() })
        live.add(kit.toggleRow(T.t("Downloads and progress"), null, prefs.progress) { prefs.progress = it; changed() })
        live.add(kit.toggleRow(
            T.t("Activities from other apps"),
            T.t("Automation apps such as Tasker can put a title and a progress ring on the island: send the broadcast io.github.jimchi_cmd.islet.ACTIVITY with command (start, update or end), id, title and progress (0–100). Up to three at once."),
            prefs.automation,
        ) { prefs.automation = it })
        live.add(kit.toggleRow(T.t("Meetings"), T.t("A countdown from 15 minutes before, with a Join button."), prefs.calendar) {
            prefs.calendar = it
            Sensors.refresh()
        })
        val pops = kit.card(col, T.t("Pop-ups"))
        pops.add(kit.toggleRow(T.t("Grow downward"), T.t("Pop-ups drop down from the camera instead of spreading sideways, and notifications open under the status bar, so its clock and battery stay in view."), prefs.popsDown) { prefs.popsDown = it; changed() })
        pops.add(kit.toggleRow(T.t("Notifications"), T.t("On the lock screen only the app's name shows."), prefs.popNotes) { prefs.popNotes = it })
        pops.add(kit.toggleRow(T.t("Charging and low battery"), null, prefs.popPower) { prefs.popPower = it })
        pops.add(kit.toggleRow(T.t("Ring, vibrate and silent"), T.t("Also airplane mode and Do Not Disturb."), prefs.popModes) { prefs.popModes = it })
        pops.add(kit.toggleRow(T.t("Wi-Fi and Bluetooth"), T.t("With the device's battery, when it says."), prefs.popConnections) { prefs.popConnections = it })
        pops.add(kit.toggleRow(T.t("Volume and brightness"), T.t("Shown on the island as well as Android's own slider. Brightness only when set by hand."), prefs.popLevels) { prefs.popLevels = it })
        pops.add(kit.toggleRow(T.t("Screenshots"), null, prefs.popScreenshots) { prefs.popScreenshots = it })
        val quiet = prefs.quietApps
        pops.add(kit.valueRow(T.t("Apps that never pop up"), if (quiet.isEmpty()) T.t("None") else quiet.size.toString()) { pickQuietApps() })
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
            .setPositiveButton(T.t("Done")) { _, _ -> done(apps.filterIndexed { i, _ -> checked[i] }.map { it.first }) }
            .setNegativeButton(T.t("Cancel"), null)
            .show()
    }

    private fun pickQuietApps() = pickApps(T.t("Never pop up"), prefs.quietApps, 500) {
        prefs.quietApps = it.toSet()
        build()
    }

    // ---- Timers ----------------------------------------------------------------------------------

    private fun timers(col: LinearLayout) {
        val start = kit.card(col, T.t("Timer"), T.t("Also on the island: open it while it's empty for 1, 5, 10 and 25-minute timers."), top = 14)
        start.add(
            kit.buttons(
                kit.button("1 min") { Isle.startFocus(1.0) },
                kit.button("5 min") { Isle.startFocus(5.0) },
                kit.button("10 min") { Isle.startFocus(10.0) },
                kit.button("25 min") { Isle.startFocus(25.0) },
                kit.button(T.t("Stop")) { Isle.focus.stop(); Isle.syncTimers() },
            ),
        )
        val focus = kit.card(col, T.t("Focus rounds"), T.t("Focus, a short break, and a longer break after every fourth round. A buzz and a sound when each part ends."))
        focus.add(kit.stepperRow(T.t("Focus"), value = { T.f("{} min", prefs.focusMinutes) }) { prefs.focusMinutes += it * 5 })
        focus.add(kit.stepperRow(T.t("Break"), value = { T.f("{} min", prefs.breakMinutes) }) { prefs.breakMinutes += it })
        focus.add(kit.buttons(kit.button(T.t("Start focus rounds"), primary = true) { Isle.startPomodoro() }))
        val day = Streak.dayOf(System.currentTimeMillis())
        val s = prefs.streak
        val streak = kit.card(col, T.t("Focus streak"), T.t("Days in a row with at least one finished focus round. It shows on the island's timer."))
        streak.add(kit.row(
            when (s.current(day)) {
                0 -> if (s.rounds == 0) T.t("No focus rounds yet") else T.t("No streak right now")
                1 -> "1 day so far"
                else -> T.f("{} days in a row", s.current(day))
            },
            T.f("Today {} · best {} · {} in all", s.doneOn(day), if (s.best == 1) T.t("1 day") else T.f("{} days", s.best), s.rounds),
            right = kit.text(s.current(day).toString(), 26f, bold = true),
        ))
        val sw = kit.card(col, T.t("Stopwatch"))
        sw.add(
            kit.buttons(
                kit.button(T.t("Start"), primary = true) { Isle.stopwatch.start(Isle.now()); Isle.syncTimers() },
                kit.button(T.t("Lap")) { Isle.stopwatch.lap(Isle.now()); Isle.syncTimers() },
                kit.button(T.t("Pause")) { Isle.stopwatch.pause(Isle.now()); Isle.syncTimers() },
                kit.button(T.t("Reset")) { Isle.stopwatch.reset(); Isle.syncTimers() },
            ),
        )
    }

    // ---- Weather and clocks -----------------------------------------------------------------------

    private fun weather(col: LinearLayout) {
        val place = prefs.place
        val w = Isle.weather
        val c = kit.card(col, T.t("Weather"), T.t("From Open-Meteo, free and without an account. Islet sends it only your place's coordinates, at most every half hour."), top = 14)
        c.add(kit.valueRow(T.t("Place"), place?.name?.substringBefore(',') ?: T.t("Choose…")) { searchPlace() })
        if (place != null && w != null) c.add(kit.row(T.t("Now"), "${w.words}${w.highC?.let { " · " + T.f("high {}", Format.temperature(it, prefs.fahrenheit)) } ?: ""}", right = kit.text(Format.temperature(w.celsius, prefs.fahrenheit), 22f, bold = true)))
        if (place != null) c.add(kit.buttons(kit.button(T.t("Remove")) { prefs.place = null; Isle.weather = null; build() }))
        val clocks = kit.card(col, T.t("World clocks"), T.t("Up to three, on the island's clock card."))
        for (zone in prefs.worldClocks) {
            clocks.add(kit.row(zone.substringAfterLast('/').replace('_', ' '), zone.substringBefore('/'), right = kit.button(T.t("Remove")) {
                prefs.worldClocks = prefs.worldClocks - zone
                build()
            }))
        }
        if (prefs.worldClocks.size < 3) clocks.add(kit.valueRow(T.t("Add a city"), "") { addClock() })
    }

    private fun searchPlace() {
        val input = EditText(this).apply {
            hint = T.t("City or town")
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_WORDS
        }
        AlertDialog.Builder(this)
            .setTitle(T.t("Weather for…"))
            .setView(padded(input))
            .setPositiveButton(T.t("Search")) { _, _ ->
                val q = input.text.toString()
                if (q.isBlank()) return@setPositiveButton
                Isle.background {
                    val found = Net.get(Forecast.searchUrl(q))?.let { Forecast.places(it) }.orEmpty()
                    runOnUiThread { choosePlace(found) }
                }
            }
            .setNegativeButton(T.t("Cancel"), null)
            .show()
    }

    private fun padded(v: View) = android.widget.FrameLayout(this).apply {
        setPadding(kit.dp(22), kit.dp(8), kit.dp(22), 0)
        addView(v)
    }

    private fun choosePlace(found: List<Place>) {
        if (found.isEmpty()) {
            message(T.t("No place by that name (or no connection)."))
            return
        }
        AlertDialog.Builder(this)
            .setTitle(T.t("Choose"))
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
            .setTitle(T.t("Add a city"))
            .setItems(names.toTypedArray()) { _, i ->
                prefs.worldClocks = (prefs.worldClocks + zones[i]).distinct()
                build()
            }
            .show()
    }

    // ---- To-do and notes ---------------------------------------------------------------------------

    private fun todo(col: LinearLayout) {
        val todos = prefs.todos
        val list = kit.card(col, T.t("To-do"), T.t("The first unfinished one shows on the island's clock card."), top = 14)
        for ((i, t) in todos.withIndex()) {
            val tick = kit.status(t.done).apply {
                setOnClickListener {
                    prefs.todos = prefs.todos.toMutableList().also { it[i] = Todo(t.text, !t.done) }
                    build()
                }
                contentDescription = if (t.done) T.t("Done") else T.t("Not done")
            }
            list.add(kit.row(t.text, null, tick, kit.button(T.t("Delete")) {
                prefs.todos = prefs.todos.filterIndexed { j, _ -> j != i }
                build()
            }))
        }
        list.add(
            kit.buttons(
                kit.button(T.t("Add a to-do"), primary = true) { ask(T.t("New to-do")) { s -> prefs.todos = prefs.todos + Todo(s, false); build() } },
                *(if (todos.any { it.done }) arrayOf(kit.button("Clear done") { prefs.todos = prefs.todos.filter { !it.done }; build() }) else emptyArray()),
            ),
        )
        val notes = prefs.notes
        val n = kit.card(col, T.t("Quick notes"), T.t("One line at a time, with the date and time."))
        for (line in notes.takeLast(8).reversed()) n.add(kit.para(line, kit.theme.text, 15f))
        n.add(
            kit.buttons(
                kit.button(T.t("Add a note"), primary = true) {
                    ask(T.t("Quick note")) { s ->
                        val stamp = android.text.format.DateFormat.format("d MMM HH:mm", System.currentTimeMillis())
                        prefs.notes = prefs.notes + "$stamp  $s"
                        build()
                    }
                },
                *(
                    if (notes.isNotEmpty()) {
                        arrayOf(
                            kit.button(T.t("Share all")) {
                                startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, notes.joinToString("\n")), T.t("Share notes")))
                            },
                            kit.button(T.t("Delete all")) { prefs.notes = emptyList(); build() },
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
            .setPositiveButton(T.t("Add")) { _, _ -> input.text.toString().trim().takeIf { it.isNotEmpty() }?.let(done) }
            .setNegativeButton(T.t("Cancel"), null)
            .show()
    }

    // ---- Control centre ------------------------------------------------------------------------------

    private fun controls(col: LinearLayout) {
        col.addView(
            kit.text(
                T.t("Long-press the island: flashlight, Do Not Disturb, ring or vibrate, volume, brightness, screenshot, lock and your shortcut apps. Android doesn't let apps switch Wi-Fi, Bluetooth or airplane mode, so those buttons open the phone's own panel."),
                14f,
                kit.theme.muted,
            ).apply { setPadding(kit.dp(16), kit.dp(14), kit.dp(16), 0) },
        )
        val c = kit.card(col, T.t("Shortcuts"))
        val names = prefs.shortcuts.map { Isle.appName(it) }
        c.add(kit.valueRow(T.t("Apps (up to 4)"), if (names.isEmpty()) T.t("None") else names.joinToString(", ")) {
            pickApps(T.t("Shortcuts"), prefs.shortcuts.toSet(), 4) {
                prefs.shortcuts = it
                build()
            }
        })
    }

    // ---- Music --------------------------------------------------------------------------------------

    private fun music(col: LinearLayout) {
        col.addView(
            kit.text(T.t("Any app that plays music shows on the island, with its cover, controls and a progress bar you can drag."), 14f, kit.theme.muted)
                .apply { setPadding(kit.dp(16), kit.dp(14), kit.dp(16), 0) },
        )
        val sleep = kit.card(col, T.t("Sleep timer"), T.t("Pauses the music when the time is up. Also on the island: the moon button on the music card."), top = 14)
        val left = Isle.sleepMinutesLeft()
        sleep.add(kit.row(if (left == null) T.t("Off") else T.f("Pauses in {} min", left), null))
        sleep.add(kit.buttons(
            kit.button("15 min") { setSleep(15) },
            kit.button("30 min") { setSleep(30) },
            kit.button("45 min") { setSleep(45) },
            kit.button("60 min") { setSleep(60) },
            kit.button(T.t("Off")) { setSleep(null) },
        ))
        val bars = kit.card(col, T.t("Bars"), T.t("Android lets apps read the loudness of what's playing only with the microphone permission. Islet doesn't use the microphone and records nothing: it reads four levels while music plays."))
        bars.add(kit.toggleRow(T.t("Bars follow the sound"), if (LiveBars.failed) T.t("This phone doesn't allow it; the bars move on their own.") else null, prefs.liveBars && LiveBars.allowed()) { on ->
            if (on && !LiveBars.allowed()) {
                askPermission.launch(Manifest.permission.RECORD_AUDIO)
                prefs.liveBars = true
            } else {
                prefs.liveBars = on
                if (on) LiveBars.retry() else LiveBars.stop()
            }
        })
        val c = kit.card(col, T.t("Lyrics"), T.t("Lyrics come from LRCLIB, a free service: Islet sends it the song's title, artist and length."))
        c.add(kit.toggleRow(T.t("Synced lyrics on the island"), null, prefs.lyrics) {
            prefs.lyrics = it
            Isle.media(Isle.controller)
            build()
        })
        if (prefs.lyrics) c.add(kit.stepperRow(T.t("Timing"), T.t("If the words run early or late."), value = { "${prefs.lyricsOffset} ms" }) { prefs.lyricsOffset += it * 250L })
    }

    private fun setSleep(minutes: Int?) {
        Isle.setSleep(minutes)
        build()
    }

    // ---- Speed test ------------------------------------------------------------------------------------

    private fun speed(col: LinearLayout) {
        val now = kit.card(col, T.t("Right now"), T.t("Everything the phone sends and receives, every second. Nothing goes online for this."), top = 14)
        val rate = kit.text("–", 17f, bold = true)
        now.add(kit.row(T.t("Network use"), null, right = rate))
        val tick = object : Runnable {
            override fun run() {
                if (page != Page.Speed || !rate.isAttachedToWindow) return
                Isle.sampleNet()
                rate.text = Isle.netRate?.let { "↓ ${Format.rate(it.first)}  ↑ ${Format.rate(it.second)}" } ?: "–"
                rate.postDelayed(this, 1000)
            }
        }
        rate.post(tick)
        val c = kit.card(col, T.t("Speed test"), T.t("Uses about 35 MB with Cloudflare's public speed-test servers, only when you press the button."))
        if (speedResult.isNotEmpty()) c.add(kit.para(speedResult, kit.theme.text, 17f))
        c.add(
            kit.buttons(
                kit.button(T.t("Test speed"), primary = true) {
                    speedResult = T.t("Testing…")
                    build()
                    Isle.background {
                        val s = Net.speedTest()
                        runOnUiThread {
                            speedResult = s?.let { T.f("{} Mbps down · {} Mbps up · {} ms", Format.mbps(it.downMbps), Format.mbps(it.upMbps), it.latencyMs.toInt()) }
                                ?: T.t("The test didn't finish. Check the connection.")
                            build()
                        }
                    }
                },
            ),
        )
    }

    // ---- Alarms ----------------------------------------------------------------------------------------

    private fun alarms(col: LinearLayout) {
        val tf = android.text.format.DateFormat.getTimeFormat(this)
        val next = Isle.nextAlarm
        val c = kit.card(col, null, T.t("Alarms go into your phone's Clock app, so they ring even if Islet is closed. The next one shows on the island's calendar card."), top = 14)
        c.add(kit.row(T.t("Next alarm"), next?.let { T.date("EEEE", it) + " " + tf.format(java.util.Date(it)) } ?: T.t("None set"), right = kit.text(next?.let { tf.format(java.util.Date(it)) } ?: "–", 26f, bold = true)))
        c.add(kit.buttons(
            kit.button(T.t("Set an alarm"), primary = true) { pickAlarm() },
            kit.button(T.t("In 20 min")) { alarmIn(20) },
            kit.button(T.t("In 1 hour")) { alarmIn(60) },
            kit.button(T.t("Open Clock")) { Isle.start(Intent(android.provider.AlarmClock.ACTION_SHOW_ALARMS)) },
        ))
    }

    private fun pickAlarm() {
        val now = java.util.Calendar.getInstance()
        android.app.TimePickerDialog(this, { _, h, m ->
            val label = EditText(this).apply {
                hint = T.t("Label (optional)")
                inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
            }
            AlertDialog.Builder(this)
                .setTitle(T.f("Alarm at {}", Timetable.hm(h * 60 + m, android.text.format.DateFormat.is24HourFormat(this))))
                .setView(padded(label))
                .setPositiveButton(T.t("Set")) { _, _ -> setAlarm(h, m, label.text.toString().trim()) }
                .setNegativeButton(T.t("Cancel"), null)
                .show()
        }, now.get(java.util.Calendar.HOUR_OF_DAY), now.get(java.util.Calendar.MINUTE), android.text.format.DateFormat.is24HourFormat(this)).show()
    }

    private fun alarmIn(minutes: Int) {
        val at = java.util.Calendar.getInstance().apply { add(java.util.Calendar.MINUTE, minutes) }
        setAlarm(at.get(java.util.Calendar.HOUR_OF_DAY), at.get(java.util.Calendar.MINUTE), "")
    }

    private fun setAlarm(hour: Int, minute: Int, label: String) {
        val i = Intent(android.provider.AlarmClock.ACTION_SET_ALARM)
            .putExtra(android.provider.AlarmClock.EXTRA_HOUR, hour)
            .putExtra(android.provider.AlarmClock.EXTRA_MINUTES, minute)
            .putExtra(android.provider.AlarmClock.EXTRA_SKIP_UI, true)
        if (label.isNotEmpty()) i.putExtra(android.provider.AlarmClock.EXTRA_MESSAGE, label)
        if (!Isle.start(i)) {
            message(T.t("This phone has no Clock app that takes alarms from other apps."))
            return
        }
        Isle.pop(islet.island.Pop.Info(T.f("Alarm set for {}", Timetable.hm(hour * 60 + minute, android.text.format.DateFormat.is24HourFormat(this)))))
        window.decorView.postDelayed({ Sensors.refresh(); build() }, 800)
    }

    // ---- Classes and deadlines ---------------------------------------------------------------------------

    private fun study(col: LinearLayout) {
        val t = prefs.timetable
        val now = System.currentTimeMillis()
        val h24 = android.text.format.DateFormat.is24HourFormat(this)
        val today = t.line(Timetable.weekday(now), Timetable.minuteOfDay(now), h24)
        val week = kit.card(col, T.t("Timetable"), T.t("Your week's lessons. The island counts down from 15 minutes before each one, then shows the time left in it."), top = 14)
        if (today != null) week.add(kit.row(today, T.t("Today"), tile = IconTile(this, Icon.Book, Tints.cyan)))
        for ((i, l) in t.lessons.withIndex()) {
            week.add(kit.row(
                "${T.t(Timetable.DAYS[l.day].take(3))} ${Timetable.hm(l.start, h24)}–${Timetable.hm(l.end, h24)}  ${l.name}",
                l.room.ifEmpty { null },
                right = kit.button(T.t("Remove")) { prefs.timetable = prefs.timetable.remove(i); changed(); build() },
            ))
        }
        week.add(kit.buttons(kit.button(T.t("Add a lesson"), primary = true) { addLesson() }))
        val opts = kit.card(col, T.t("On the island"))
        opts.add(kit.toggleRow(T.t("Count down to lessons"), null, prefs.lessons) { prefs.lessons = it; changed() })
        opts.add(kit.toggleRow(T.t("Do Not Disturb during lessons"), T.t("Turns it on when a lesson starts and off when it ends (if Islet turned it on). Needs notification access."), prefs.lessonQuiet) { prefs.lessonQuiet = it; changed() })
        val due = kit.card(col, T.t("Deadlines"), T.t("From your calendar: events called exam, test, essay, assignment, homework, project, due and the like, in the next two weeks. They show on the island's calendar card."))
        due.add(kit.toggleRow(T.t("Count down to deadlines"), null, prefs.deadlines) { prefs.deadlines = it; Sensors.refresh(); build() })
        if (prefs.deadlines && !granted(Manifest.permission.READ_CALENDAR)) {
            due.add(kit.row(T.t("Calendar access"), T.t("Needed to find deadlines."), right = kit.button(T.t("Allow")) { askPermission.launch(Manifest.permission.READ_CALENDAR) }))
        }
        if (prefs.deadlines) {
            for (e in Isle.deadlines.filter { it.end > now }.take(8)) {
                due.add(kit.row(e.title, Deadlines.countdown(e, now), tile = IconTile(this, Icon.Flag, Tints.orange)))
            }
        }
    }

    private fun addLesson() {
        AlertDialog.Builder(this)
            .setTitle(T.t("Which day?"))
            .setItems(Timetable.DAYS.toTypedArray()) { _, day ->
                val h24 = android.text.format.DateFormat.is24HourFormat(this)
                android.app.TimePickerDialog(this, { _, sh, sm ->
                    android.app.TimePickerDialog(this, { _, eh, em ->
                        val name = EditText(this).apply { hint = T.t("Lesson"); inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_WORDS }
                        val room = EditText(this).apply { hint = T.t("Room (optional)"); inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_CHARACTERS }
                        val form = kit.column().apply { addView(name); addView(room) }
                        AlertDialog.Builder(this)
                            .setTitle("${T.t(Timetable.DAYS[day])} ${Timetable.hm(sh * 60 + sm, h24)}–${Timetable.hm(eh * 60 + em, h24)}")
                            .setView(padded(form))
                            .setPositiveButton(T.t("Add")) { _, _ ->
                                prefs.timetable.add(day, sh * 60 + sm, eh * 60 + em, name.text.toString(), room.text.toString())
                                    .onSuccess { prefs.timetable = it; changed(); build() }
                                    .onFailure { message(it.message ?: T.t("That lesson can't be added.")) }
                            }
                            .setNegativeButton(T.t("Cancel"), null)
                            .show()
                    }, (sh + 1) % 24, sm, h24).apply { setTitle(T.t("Ends at")) }.show()
                }, 9, 0, h24).apply { setTitle(T.t("Starts at")) }.show()
            }
            .show()
    }

    // ---- Battery -------------------------------------------------------------------------------------------

    private fun battery(col: LinearLayout) {
        val b = registerReceiver(null, android.content.IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        val bm = getSystemService(android.os.BatteryManager::class.java)
        val level = Isle.battery
        val c = kit.card(col, null, T.t("Android doesn't tell apps how much the battery has worn, but it shares its health, temperature and, on Android 14 and later, how many times it has been charged."), top = 14)
        c.add(kit.row(T.t("Charge"), when (b?.getIntExtra(android.os.BatteryManager.EXTRA_PLUGGED, 0)) {
            android.os.BatteryManager.BATTERY_PLUGGED_AC -> T.t("Charging from the wall")
            android.os.BatteryManager.BATTERY_PLUGGED_USB -> T.t("Charging over USB")
            android.os.BatteryManager.BATTERY_PLUGGED_WIRELESS -> T.t("Charging wirelessly")
            else -> T.t("On battery")
        }, right = kit.text(if (level >= 0) "$level%" else "–", 26f, bold = true)))
        val health = when (b?.getIntExtra(android.os.BatteryManager.EXTRA_HEALTH, 0)) {
            android.os.BatteryManager.BATTERY_HEALTH_GOOD -> T.t("Good")
            android.os.BatteryManager.BATTERY_HEALTH_OVERHEAT -> T.t("Too hot")
            android.os.BatteryManager.BATTERY_HEALTH_COLD -> T.t("Too cold")
            android.os.BatteryManager.BATTERY_HEALTH_DEAD -> T.t("Worn out: time for a new battery")
            android.os.BatteryManager.BATTERY_HEALTH_OVER_VOLTAGE -> T.t("Over voltage")
            android.os.BatteryManager.BATTERY_HEALTH_UNSPECIFIED_FAILURE -> T.t("Failing")
            else -> T.t("Not reported")
        }
        c.add(kit.row(T.t("Health"), null, right = kit.text(health, 16f, if (health == T.t("Good")) kit.theme.ok else kit.theme.text, bold = true)))
        b?.getIntExtra(android.os.BatteryManager.EXTRA_TEMPERATURE, Int.MIN_VALUE)?.takeIf { it != Int.MIN_VALUE }?.let {
            c.add(kit.row(T.t("Temperature"), null, right = kit.text(Format.temperature(it / 10.0, prefs.fahrenheit), 16f, bold = true)))
        }
        b?.getIntExtra(android.os.BatteryManager.EXTRA_VOLTAGE, 0)?.takeIf { it > 0 }?.let {
            c.add(kit.row(T.t("Voltage"), null, right = kit.text("%.2f V".format(java.util.Locale.ROOT, it / 1000.0), 16f, bold = true)))
        }
        if (Build.VERSION.SDK_INT >= 34) {
            b?.getIntExtra(android.os.BatteryManager.EXTRA_CYCLE_COUNT, -1)?.takeIf { it >= 0 }?.let {
                c.add(kit.row(T.t("Charge cycles"), T.t("Most batteries keep about 80% of their charge for 500 to 800 cycles."), right = kit.text(it.toString(), 16f, bold = true)))
            }
        }
        b?.getStringExtra(android.os.BatteryManager.EXTRA_TECHNOLOGY)?.takeIf { it.isNotBlank() }?.let {
            c.add(kit.row(T.t("Type"), null, right = kit.text(it, 16f, bold = true)))
        }
        bm?.getIntProperty(android.os.BatteryManager.BATTERY_PROPERTY_CHARGE_COUNTER)?.takeIf { it > 0 && level > 0 }?.let { uah ->
            // Charge left now, and roughly what a full charge holds today.
            c.add(kit.row(T.t("Holds now"), T.f("About {} mAh when full", uah / 10 / level), right = kit.text("${uah / 1000} mAh", 16f, bold = true)))
        }
        // As the computer's system panel: memory and storage too.
        val s = kit.card(col, T.t("Memory and storage"), null)
        getSystemService(android.app.ActivityManager::class.java)?.let { am ->
            val info = android.app.ActivityManager.MemoryInfo()
            am.getMemoryInfo(info)
            if (info.totalMem > 0) {
                val used = info.totalMem - info.availMem
                s.add(kit.row(T.t("Memory"), T.f("{} of {} in use", gb(used), gb(info.totalMem)), right = kit.text("${used * 100 / info.totalMem}%", 16f, bold = true)))
            }
        }
        try {
            val st = android.os.StatFs(android.os.Environment.getDataDirectory().path)
            val total = st.totalBytes
            val free = st.availableBytes
            if (total > 0) s.add(kit.row(T.t("Storage"), T.f("{} free of {}", gb(free), gb(total)), right = kit.text("${(total - free) * 100 / total}%", 16f, bold = true)))
        } catch (e: Exception) {
            // Not readable here.
        }
    }

    private fun gb(bytes: Long) = "%.1f GB".format(java.util.Locale.getDefault(), bytes / 1e9)

    // ---- Quick launcher ----------------------------------------------------------------------------------------

    private fun launch(col: LinearLayout) {
        val c = kit.card(col, null, T.t("On the island: tap the empty island twice for the card, then the search button. Type a few letters of an app or a link and tap it."), top = 14)
        c.add(kit.buttons(kit.button(T.t("Open the launcher"), primary = true) { startActivity(Intent(this, LauncherActivity::class.java)) }))
        val links = prefs.links
        val l = kit.card(col, T.t("Your links"), T.t("Websites, phone numbers (tel:), email (mailto:) or an app's own links. Up to 30."))
        for ((i, link) in links.withIndex()) {
            l.add(kit.row(link.name, link.url, tile = IconTile(this, Icon.Link, Tints.blue), right = kit.button(T.t("Remove")) {
                prefs.links = prefs.links.filterIndexed { j, _ -> j != i }
                build()
            }))
        }
        l.add(kit.buttons(kit.button(T.t("Add a link"), primary = true) { addLink() }))
    }

    private fun addLink() {
        val name = EditText(this).apply { hint = T.t("Name"); inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_WORDS }
        val url = EditText(this).apply { hint = "example.com or tel:…"; inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI }
        AlertDialog.Builder(this)
            .setTitle(T.t("Add a link"))
            .setView(padded(kit.column().apply { addView(name); addView(url) }))
            .setPositiveButton(T.t("Add")) { _, _ ->
                val u = Launcher.tidyLink(url.text.toString())
                val n = name.text.toString().trim()
                when {
                    n.isEmpty() -> message(T.t("Give the link a name."))
                    !Launcher.safeLink(u) -> message(T.t("That doesn't look like a link Islet can open."))
                    else -> {
                        prefs.links = prefs.links + Link(n, u)
                        build()
                    }
                }
            }
            .setNegativeButton(T.t("Cancel"), null)
            .show()
    }

    // ---- The computer ----------------------------------------------------------------------------------

    private fun computer(col: LinearLayout) {
        val pairing = Store.pairing(this)
        val computer = Hub.computer
        val c = kit.card(col, null, top = 14)
        c.add(
            kit.row(
                when {
                    pairing == null -> T.t("Not paired")
                    !hasAccess() -> T.f("Paired with {}", pairing.name)
                    computer != null -> T.f("Connected to {}", computer)
                    else -> T.f("Looking for {}…", pairing.name)
                },
                when {
                    pairing == null ->
                        T.t("Show your phone's notifications, music and battery on Islet on your computer too. On the computer, open Islet Settings → Wi-Fi, Bluetooth & phone → Your Android phone → Pair a phone, then scan the code.")
                    computer != null -> T.t("Your notifications, music and battery level show on the computer's island.")
                    !hasAccess() -> T.t("Allow notification access first (Set up).")
                    else ->
                        T.t("Make sure the computer is on, Islet is running and both are on the same Wi-Fi.") +
                            (if (Hub.problem.isNotEmpty()) "\n" + T.f("Last try: {}.", Hub.problem) else "")
                },
                kit.status(computer != null),
            ),
        )
        c.add(
            kit.buttons(
                kit.button(if (pairing == null) T.t("Scan the code") else T.t("Scan a new code"), primary = pairing == null) {
                    scan.launch(
                        ScanOptions()
                            .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                            .setPrompt(T.t("Point at the code in Islet Settings on your computer"))
                            .setBeepEnabled(false)
                            .setOrientationLocked(false),
                    )
                },
                kit.button(T.t("Paste a code")) { pasteCode() },
            ),
        )
        if (pairing != null) {
            val s = kit.card(col, T.t("Send to the computer"), T.t("Your phone talks straight to your computer over your Wi-Fi, encrypted with a key only the two hold."))
            s.add(kit.toggleRow(T.t("Notifications"), null, Store.sendNotes(this)) { Store.setSendNotes(this, it) })
            s.add(kit.toggleRow(T.t("What is playing"), null, Store.sendMedia(this)) {
                Store.setSendMedia(this, it)
                IsletListener.instance?.reload()
            })
            s.add(kit.buttons(kit.button(T.t("Forget this computer")) {
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
            message(T.t("That isn't an Islet pairing code. On your computer, open Islet Settings → Wi-Fi, Bluetooth & phone → Your Android phone."))
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
            .setTitle(T.t("Allow notification access"))
            .setMessage(
                T.t("Islet needs notification access to show your notifications, music, calls and directions on the island (and on your computer, if you pair one). On the next screen, turn on Islet.") + "\n\n" + T.t("If the switch is greyed out: open this app's App info, tap the three dots at the top right, choose \"Allow restricted settings\", then come back."),
            )
            .setPositiveButton(T.t("Open settings")) { _, _ -> startActivity(Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS)) }
            .setNegativeButton(T.t("Not now"), null)
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
            .setTitle(T.t("Paste the pairing code"))
            .setView(padded(input))
            .setPositiveButton(T.t("Pair")) { _, _ -> pairWith(input.text.toString()) }
            .setNegativeButton(T.t("Cancel"), null)
            .show()
    }
}
