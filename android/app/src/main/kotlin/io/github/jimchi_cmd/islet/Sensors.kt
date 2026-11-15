package io.github.jimchi_cmd.islet

import android.Manifest
import android.app.AlarmManager
import android.app.NotificationManager
import android.bluetooth.BluetoothDevice
import android.content.BroadcastReceiver
import android.content.ContentUris
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.database.ContentObserver
import android.hardware.camera2.CameraManager
import android.media.AudioManager
import android.media.AudioRecordingConfiguration
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.os.BatteryManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.CalendarContract
import android.provider.MediaStore
import android.provider.Settings
import androidx.core.content.ContextCompat
import islet.island.CalendarEvent
import islet.island.Forecast
import islet.island.Pop

/**
 * Everything the island watches besides notifications and music: power,
 * sound mode, airplane mode, Do Not Disturb, Wi-Fi, Bluetooth, camera and
 * microphone, the flashlight, screenshots, the calendar, the next alarm
 * and the weather. Started by whichever service hosts the island.
 */
object Sensors {
    private val main = Handler(Looper.getMainLooper())
    private var users = 0
    private lateinit var ctx: Context
    private var startedAt = 0L
    private val busyCameras = HashSet<String>()
    private var lastLowWarning = 101

    private fun has(permission: String) =
        ContextCompat.checkSelfPermission(ctx, permission) == PackageManager.PERMISSION_GRANTED

    /** Settling: the first reports after starting describe how things are, not a change. */
    private fun fresh() = System.currentTimeMillis() - startedAt < 3000

    private val receiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            val p = Isle.prefs
            when (intent.action) {
                Intent.ACTION_BATTERY_CHANGED -> battery(intent)
                AudioManager.RINGER_MODE_CHANGED_ACTION -> if (p.popModes && !fresh()) {
                    val mode = when (intent.getIntExtra(AudioManager.EXTRA_RINGER_MODE, AudioManager.RINGER_MODE_NORMAL)) {
                        AudioManager.RINGER_MODE_SILENT -> Pop.RingerMode.Silent
                        AudioManager.RINGER_MODE_VIBRATE -> Pop.RingerMode.Vibrate
                        else -> Pop.RingerMode.Ring
                    }
                    Isle.pop(Pop.Ringer(mode))
                }
                Intent.ACTION_AIRPLANE_MODE_CHANGED -> {
                    Isle.airplane = intent.getBooleanExtra("state", false)
                    if (p.popModes) Isle.pop(Pop.Airplane(Isle.airplane))
                    Isle.changed()
                }
                NotificationManager.ACTION_INTERRUPTION_FILTER_CHANGED -> {
                    val was = Isle.dnd
                    readDnd()
                    if (p.popModes && was != Isle.dnd) Isle.pop(Pop.DoNotDisturb(Isle.dnd))
                    Isle.changed()
                }
                BluetoothDevice.ACTION_ACL_CONNECTED, BluetoothDevice.ACTION_ACL_DISCONNECTED -> if (p.popConnections && !fresh()) {
                    val device: BluetoothDevice? = if (Build.VERSION.SDK_INT >= 33) {
                        intent.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE, BluetoothDevice::class.java)
                    } else {
                        @Suppress("DEPRECATION")
                        intent.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE)
                    }
                    val name = try {
                        device?.name
                    } catch (e: SecurityException) {
                        null
                    } ?: "Bluetooth device"
                    val connected = intent.action == BluetoothDevice.ACTION_ACL_CONNECTED
                    Isle.btName = if (connected) name else null
                    Isle.pop(Pop.Bluetooth(name, connected))
                }
                AlarmManager.ACTION_NEXT_ALARM_CLOCK_CHANGED -> readAlarm()
                android.net.wifi.WifiManager.WIFI_STATE_CHANGED_ACTION, android.bluetooth.BluetoothAdapter.ACTION_STATE_CHANGED -> {
                    readRadios()
                    Isle.changed()
                }
                Intent.ACTION_SCREEN_OFF, Intent.ACTION_SCREEN_ON -> {
                    Isle.screenOn = intent.action == Intent.ACTION_SCREEN_ON
                    Isle.changed()
                }
            }
        }
    }

    private fun battery(intent: Intent) {
        val level = intent.getIntExtra(BatteryManager.EXTRA_LEVEL, -1)
        val scale = intent.getIntExtra(BatteryManager.EXTRA_SCALE, 100)
        if (level < 0 || scale <= 0) return
        val percent = level * 100 / scale
        val status = intent.getIntExtra(BatteryManager.EXTRA_STATUS, -1)
        val charging = status == BatteryManager.BATTERY_STATUS_CHARGING || status == BatteryManager.BATTERY_STATUS_FULL ||
            intent.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) != 0
        val first = Isle.battery < 0
        val pluggedChanged = charging != Isle.charging
        Isle.battery = percent
        Isle.charging = charging
        if (first) return
        if (Isle.prefs.popPower) {
            if (pluggedChanged) Isle.pop(Pop.Charging(percent, charging))
            if (charging) {
                lastLowWarning = 101
            } else {
                val warn = listOf(20, 10, 5).firstOrNull { percent <= it && it < lastLowWarning }
                if (warn != null) {
                    lastLowWarning = warn
                    Isle.pop(Pop.LowBattery(percent))
                }
            }
        }
    }

    private fun readDnd() {
        val nm = ctx.getSystemService(NotificationManager::class.java)
        Isle.dnd = nm != null && nm.currentInterruptionFilter > NotificationManager.INTERRUPTION_FILTER_ALL
    }

    private fun readAlarm() {
        Isle.nextAlarm = ctx.getSystemService(AlarmManager::class.java)?.nextAlarmClock?.triggerTime
        Isle.changed()
    }

    /** Wi-Fi coming and going, and its name. */
    private object WifiEvents {
        fun available() {
            if (fresh() || !Isle.prefs.popConnections) return
            // Wait a moment for the network's name, then say so.
            Handler(Looper.getMainLooper()).postDelayed({ Isle.pop(Pop.Wifi(Isle.wifiName.orEmpty())) }, 800)
        }

        fun capabilities(caps: NetworkCapabilities) {
            val name = wifiName(caps)
            Isle.post {
                Isle.wifiName = name
                Isle.changed()
            }
        }

        fun lost() = Isle.post {
            Isle.wifiName = null
            Isle.changed()
        }
    }

    /** Android 12 and later share the network's name only when asked for it. */
    @androidx.annotation.RequiresApi(31)
    private class WifiWithName : ConnectivityManager.NetworkCallback(FLAG_INCLUDE_LOCATION_INFO) {
        override fun onAvailable(network: Network) = WifiEvents.available()

        override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) = WifiEvents.capabilities(caps)

        override fun onLost(network: Network) = WifiEvents.lost()
    }

    private class WifiPlain : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) = WifiEvents.available()

        override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) = WifiEvents.capabilities(caps)

        override fun onLost(network: Network) = WifiEvents.lost()
    }

    private val wifi: ConnectivityManager.NetworkCallback by lazy {
        if (Build.VERSION.SDK_INT >= 31) WifiWithName() else WifiPlain()
    }

    /** The network's name, if Android shares it (location access on), without the quotes. */
    @Suppress("DEPRECATION")
    private fun wifiName(caps: NetworkCapabilities): String? {
        val info = if (Build.VERSION.SDK_INT >= 31) {
            caps.transportInfo as? android.net.wifi.WifiInfo
        } else {
            ctx.getSystemService(android.net.wifi.WifiManager::class.java)?.connectionInfo
        }
        val ssid = info?.ssid?.removeSurrounding("\"") ?: return null
        return ssid.takeIf { it.isNotBlank() && it != "<unknown ssid>" && it != "0x" }
    }

    private fun readRadios() {
        Isle.wifiOn = try {
            ctx.getSystemService(android.net.wifi.WifiManager::class.java)?.isWifiEnabled == true
        } catch (e: Exception) {
            false
        }
        Isle.btOn = try {
            ctx.getSystemService(android.bluetooth.BluetoothManager::class.java)?.adapter?.isEnabled == true
        } catch (e: SecurityException) {
            false
        }
        if (!Isle.btOn) Isle.btName = null
        if (!Isle.wifiOn) Isle.wifiName = null
    }

    private val cameras = object : CameraManager.AvailabilityCallback() {
        override fun onCameraUnavailable(cameraId: String) {
            busyCameras.add(cameraId)
            updateCamera()
        }

        override fun onCameraAvailable(cameraId: String) {
            busyCameras.remove(cameraId)
            updateCamera()
        }
    }

    private fun updateCamera() {
        Isle.cameraOn = busyCameras.isNotEmpty() && !Isle.torchOn
        Isle.changed()
    }

    private val torch = object : CameraManager.TorchCallback() {
        override fun onTorchModeChanged(cameraId: String, enabled: Boolean) {
            Isle.torchOn = enabled
            updateCamera()
        }
    }

    private val recording = object : AudioManager.AudioRecordingCallback() {
        override fun onRecordingConfigChanged(configs: MutableList<AudioRecordingConfiguration>?) {
            Isle.micOn = !configs.isNullOrEmpty()
            Isle.changed()
        }
    }

    private val screenshots = object : ContentObserver(main) {
        override fun onChange(selfChange: Boolean) = checkScreenshot()
    }

    private val calendarChanged = object : ContentObserver(main) {
        override fun onChange(selfChange: Boolean) = readCalendar()
    }

    private val periodic = object : Runnable {
        override fun run() {
            readCalendar()
            readWeather()
            main.postDelayed(this, 15 * 60_000L)
        }
    }

    fun start(context: Context) {
        Isle.init(context)
        users++
        if (users > 1) return
        ctx = context.applicationContext
        startedAt = System.currentTimeMillis()
        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_BATTERY_CHANGED)
            addAction(AudioManager.RINGER_MODE_CHANGED_ACTION)
            addAction(Intent.ACTION_AIRPLANE_MODE_CHANGED)
            addAction(NotificationManager.ACTION_INTERRUPTION_FILTER_CHANGED)
            addAction(AlarmManager.ACTION_NEXT_ALARM_CLOCK_CHANGED)
            addAction(Intent.ACTION_SCREEN_OFF)
            addAction(android.net.wifi.WifiManager.WIFI_STATE_CHANGED_ACTION)
            addAction(android.bluetooth.BluetoothAdapter.ACTION_STATE_CHANGED)
            addAction(Intent.ACTION_SCREEN_ON)
            if (Build.VERSION.SDK_INT < 31 || has(Manifest.permission.BLUETOOTH_CONNECT)) {
                addAction(BluetoothDevice.ACTION_ACL_CONNECTED)
                addAction(BluetoothDevice.ACTION_ACL_DISCONNECTED)
            }
        }
        ContextCompat.registerReceiver(ctx, receiver, filter, ContextCompat.RECEIVER_EXPORTED)
        Isle.airplane = Settings.Global.getInt(ctx.contentResolver, Settings.Global.AIRPLANE_MODE_ON, 0) != 0
        readDnd()
        readAlarm()
        readRadios()
        try {
            ctx.getSystemService(ConnectivityManager::class.java)?.registerNetworkCallback(
                NetworkRequest.Builder().addTransportType(NetworkCapabilities.TRANSPORT_WIFI).build(),
                wifi,
            )
        } catch (e: RuntimeException) {
            // Too many callbacks on this phone: no Wi-Fi pop-ups.
        }
        ctx.getSystemService(CameraManager::class.java)?.let {
            it.registerAvailabilityCallback(cameras, main)
            it.registerTorchCallback(torch, main)
        }
        ctx.getSystemService(AudioManager::class.java)?.registerAudioRecordingCallback(recording, main)
        watchScreenshots()
        watchCalendar()
        main.post(periodic)
        Isle.syncTimers()
    }

    fun stop() {
        users = (users - 1).coerceAtLeast(0)
        if (users > 0 || !::ctx.isInitialized) return
        try {
            ctx.unregisterReceiver(receiver)
        } catch (e: IllegalArgumentException) {
            // Not registered.
        }
        try {
            ctx.getSystemService(ConnectivityManager::class.java)?.unregisterNetworkCallback(wifi)
        } catch (e: RuntimeException) {
            // Not registered.
        }
        ctx.getSystemService(CameraManager::class.java)?.let {
            it.unregisterAvailabilityCallback(cameras)
            it.unregisterTorchCallback(torch)
        }
        ctx.getSystemService(AudioManager::class.java)?.unregisterAudioRecordingCallback(recording)
        ctx.contentResolver.unregisterContentObserver(screenshots)
        ctx.contentResolver.unregisterContentObserver(calendarChanged)
        main.removeCallbacks(periodic)
    }

    /** Permissions may have been granted since: watch what they now allow. */
    fun refresh() {
        if (users == 0) return
        watchScreenshots()
        watchCalendar()
        readCalendar()
        readWeather()
    }

    private fun canReadPhotos() =
        if (Build.VERSION.SDK_INT >= 33) has(Manifest.permission.READ_MEDIA_IMAGES) else has(Manifest.permission.READ_EXTERNAL_STORAGE)

    private fun watchScreenshots() {
        ctx.contentResolver.unregisterContentObserver(screenshots)
        if (canReadPhotos()) {
            ctx.contentResolver.registerContentObserver(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, true, screenshots)
        }
    }

    private var lastShot = 0L

    private fun checkScreenshot() {
        if (!Isle.prefs.popScreenshots || !canReadPhotos()) return
        val since = System.currentTimeMillis() / 1000 - 15
        @Suppress("DEPRECATION")
        val folder = if (Build.VERSION.SDK_INT >= 29) MediaStore.Images.Media.RELATIVE_PATH else MediaStore.Images.Media.DATA
        val cols = arrayOf(MediaStore.Images.Media._ID, MediaStore.Images.Media.DISPLAY_NAME, folder)
        try {
            ctx.contentResolver.query(
                MediaStore.Images.Media.EXTERNAL_CONTENT_URI,
                cols,
                "${MediaStore.Images.Media.DATE_ADDED} >= ?",
                arrayOf(since.toString()),
                "${MediaStore.Images.Media.DATE_ADDED} DESC",
            )?.use { c ->
                if (!c.moveToFirst()) return
                val id = c.getLong(0)
                val name = c.getString(1).orEmpty()
                val path = c.getString(2).orEmpty()
                if (id == lastShot) return
                if (!name.contains("screenshot", true) && !path.contains("screenshot", true)) return
                lastShot = id
                Isle.pop(Pop.Screenshot(ContentUris.withAppendedId(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, id).toString()))
            }
        } catch (e: Exception) {
            // The photo isn't readable yet (still being written) or permission was withdrawn.
        }
    }

    private fun watchCalendar() {
        ctx.contentResolver.unregisterContentObserver(calendarChanged)
        if (has(Manifest.permission.READ_CALENDAR)) {
            ctx.contentResolver.registerContentObserver(CalendarContract.Events.CONTENT_URI, true, calendarChanged)
        }
    }

    private fun readCalendar() {
        val p = Isle.prefs
        if (!(p.calendar || p.deadlines) || !has(Manifest.permission.READ_CALENDAR)) {
            Isle.events = emptyList()
            Isle.deadlines = emptyList()
            Isle.changed()
            return
        }
        Isle.background {
            val now = System.currentTimeMillis()
            val uri = CalendarContract.Instances.CONTENT_URI.buildUpon().also {
                ContentUris.appendId(it, now - 60 * 60_000L)
                // Two weeks ahead, for deadlines; meetings use the next day and a half.
                ContentUris.appendId(it, now + 14 * 24 * 60 * 60_000L)
            }.build()
            val cols = arrayOf(
                CalendarContract.Instances.EVENT_ID,
                CalendarContract.Instances.TITLE,
                CalendarContract.Instances.BEGIN,
                CalendarContract.Instances.END,
                CalendarContract.Instances.ALL_DAY,
                CalendarContract.Instances.EVENT_LOCATION,
                CalendarContract.Instances.DESCRIPTION,
            )
            val found = ArrayList<CalendarEvent>()
            try {
                ctx.contentResolver.query(uri, cols, null, null, "${CalendarContract.Instances.BEGIN} ASC")?.use { c ->
                    while (c.moveToNext() && found.size < 500) {
                        found.add(
                            CalendarEvent(
                                c.getLong(0),
                                c.getString(1).orEmpty().ifEmpty { "Event" },
                                c.getLong(2),
                                c.getLong(3),
                                c.getInt(4) != 0,
                                c.getString(5).orEmpty(),
                                c.getString(6).orEmpty().take(4000),
                            ),
                        )
                    }
                }
            } catch (e: SecurityException) {
                // Permission withdrawn.
            }
            val soon = found.filter { it.begin < now + 36 * 60 * 60_000L }.take(50)
            val due = found.filter { islet.island.Deadlines.isDeadline(it.title) }.take(20)
            Isle.post {
                Isle.events = if (p.calendar) soon else emptyList()
                Isle.deadlines = if (p.deadlines) due else emptyList()
                Isle.changed()
            }
        }
    }

    private var weatherAt = 0L

    /** At most every half hour, and only with a place chosen. */
    fun readWeather(force: Boolean = false) {
        val place = Isle.prefs.place ?: run {
            Isle.weather = null
            return
        }
        val now = System.currentTimeMillis()
        if (!force && now - weatherAt < 29 * 60_000L) return
        weatherAt = now
        Isle.background {
            val w = Net.get(Forecast.url(place.lat, place.lon))?.let { Forecast.parse(it) }
            Isle.post {
                if (w != null) Isle.weather = w
                Isle.changed()
            }
        }
    }
}
