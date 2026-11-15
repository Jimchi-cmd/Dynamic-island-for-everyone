package io.github.jimchi_cmd.islet

import android.content.Context
import android.content.SharedPreferences
import islet.island.IslandSize
import islet.island.Lists
import islet.island.Place
import islet.island.Todo

/**
 * The phone island's settings and the small lists it keeps (to-dos,
 * notes, cities, shortcuts). Private to the app and left out of backups.
 */
class Prefs(context: Context) {
    private val p: SharedPreferences = context.applicationContext.getSharedPreferences("island", Context.MODE_PRIVATE)

    private fun flag(key: String, default: Boolean) = p.getBoolean(key, default)

    private fun set(key: String, on: Boolean) = p.edit().putBoolean(key, on).apply()

    var islandOn: Boolean
        get() = flag("on", true)
        set(v) = set("on", v)

    /** The black pill shows round the camera even with nothing in it. */
    var showEmpty: Boolean
        get() = flag("show_empty", true)
        set(v) = set("show_empty", v)

    var size: IslandSize
        get() = IslandSize.entries.firstOrNull { it.name == p.getString("size", null) } ?: IslandSize.Medium
        set(v) = p.edit().putString("size", v.name).apply()

    /** Nudges, in dp, for phones whose camera the pill doesn't sit on exactly. */
    var offsetX: Int
        get() = p.getInt("offset_x", 0)
        set(v) = p.edit().putInt("offset_x", v.coerceIn(-200, 200)).apply()

    var offsetY: Int
        get() = p.getInt("offset_y", 0)
        set(v) = p.edit().putInt("offset_y", v.coerceIn(-40, 80)).apply()

    var extraWidth: Int
        get() = p.getInt("extra_width", 0)
        set(v) = p.edit().putInt("extra_width", v.coerceIn(-40, 120)).apply()

    var albumBorder: Boolean
        get() = flag("album_border", true)
        set(v) = set("album_border", v)

    var fahrenheit: Boolean
        get() = flag("fahrenheit", false)
        set(v) = set("fahrenheit", v)

    /** "default", "light", "condensed", "serif" or "mono". */
    var font: String
        get() = p.getString("font", "default") ?: "default"
        set(v) = p.edit().putString("font", v).apply()

    var hideInLandscape: Boolean
        get() = flag("hide_landscape", true)
        set(v) = set("hide_landscape", v)

    var popNotes: Boolean
        get() = flag("pop_notes", true)
        set(v) = set("pop_notes", v)

    var popPower: Boolean
        get() = flag("pop_power", true)
        set(v) = set("pop_power", v)

    /** Ringer, airplane mode and Do Not Disturb. */
    var popModes: Boolean
        get() = flag("pop_modes", true)
        set(v) = set("pop_modes", v)

    var popConnections: Boolean
        get() = flag("pop_connections", true)
        set(v) = set("pop_connections", v)

    var popScreenshots: Boolean
        get() = flag("pop_screenshots", true)
        set(v) = set("pop_screenshots", v)

    /** Small plane and moon on the empty island while those are on. */
    var modeIcons: Boolean
        get() = flag("mode_icons", true)
        set(v) = set("mode_icons", v)

    /** Green and orange dots while an app uses the camera or microphone. */
    var privacyDots: Boolean
        get() = flag("privacy_dots", false)
        set(v) = set("privacy_dots", v)

    var calls: Boolean
        get() = flag("calls", true)
        set(v) = set("calls", v)

    var navigation: Boolean
        get() = flag("navigation", true)
        set(v) = set("navigation", v)

    var progress: Boolean
        get() = flag("progress", true)
        set(v) = set("progress", v)

    var calendar: Boolean
        get() = flag("calendar", true)
        set(v) = set("calendar", v)

    /** Synced lyrics from LRCLIB (sends the song's title and artist there). Off until switched on. */
    var lyrics: Boolean
        get() = flag("lyrics", false)
        set(v) = set("lyrics", v)

    /** Moving bars beside the song while it plays. */
    var bars: Boolean
        get() = flag("bars", true)
        set(v) = set("bars", v)

    /** Apps whose notifications never pop up. */
    var quietApps: Set<String>
        get() = p.getStringSet("quiet_apps", emptySet()).orEmpty()
        set(v) = p.edit().putStringSet("quiet_apps", v).apply()

    var place: Place?
        get() {
            val name = p.getString("place_name", null) ?: return null
            return Place(name, "", "", p.getFloat("place_lat", 0f).toDouble(), p.getFloat("place_lon", 0f).toDouble())
        }
        set(v) {
            val e = p.edit()
            if (v == null) {
                e.remove("place_name")
            } else {
                e.putString("place_name", v.label).putFloat("place_lat", v.lat.toFloat()).putFloat("place_lon", v.lon.toFloat())
            }
            e.apply()
        }

    /** Time zone ids, up to three. */
    var worldClocks: List<String>
        get() = Lists.decode(p.getString("world_clocks", null))
        set(v) = p.edit().putString("world_clocks", Lists.encode(v.take(3))).apply()

    var todos: List<Todo>
        get() = Lists.decodeTodos(p.getString("todos", null))
        set(v) = p.edit().putString("todos", Lists.encodeTodos(v)).apply()

    var notes: List<String>
        get() = Lists.decode(p.getString("notes", null))
        set(v) = p.edit().putString("notes", Lists.encode(v.takeLast(200))).apply()

    /** Package names of up to four apps for the control centre. */
    var shortcuts: List<String>
        get() = Lists.decode(p.getString("shortcuts", null))
        set(v) = p.edit().putString("shortcuts", Lists.encode(v.take(4))).apply()

    var focusMinutes: Int
        get() = p.getInt("focus_minutes", 25)
        set(v) = p.edit().putInt("focus_minutes", v.coerceIn(1, 180)).apply()

    var breakMinutes: Int
        get() = p.getInt("break_minutes", 5)
        set(v) = p.edit().putInt("break_minutes", v.coerceIn(1, 60)).apply()

    /** Lyrics timing nudge, in milliseconds. */
    var lyricsOffset: Long
        get() = p.getLong("lyrics_offset", 0)
        set(v) = p.edit().putLong("lyrics_offset", v.coerceIn(-10_000, 10_000)).apply()

    fun listen(l: SharedPreferences.OnSharedPreferenceChangeListener) = p.registerOnSharedPreferenceChangeListener(l)

    fun unlisten(l: SharedPreferences.OnSharedPreferenceChangeListener) = p.unregisterOnSharedPreferenceChangeListener(l)
}
