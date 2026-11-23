package io.github.jimchi_cmd.islet

import android.content.Context
import java.util.Locale

/**
 * The words the phone app shows, in the language picked for Islet. Every
 * translation lives in locales/strings.tsv (packaged as an asset), shared
 * with the computer's island and Settings: one line per English sentence,
 * one column per language. A key may carry a note for translators after
 * "‖", which is never shown.
 */
object T {
    /** The languages Islet speaks, by code, each in its own name ("auto" follows the phone). */
    val LANGUAGES = listOf(
        "auto" to "Automatic",
        "en" to "English",
        "es" to "Español",
        "fr" to "Français",
        "de" to "Deutsch",
        "pt" to "Português",
        "ru" to "Русский",
        "zh" to "中文（简体）",
        "ja" to "日本語",
        "ar" to "العربية",
        "hi" to "हिन्दी",
    )

    @Volatile private var words: Map<String, String> = emptyMap()

    /** The language in use. */
    @Volatile var code = "en"
        private set

    /** Use the language set in Islet ("auto" or a code), reading its column of the table once. */
    fun load(context: Context, setting: String) {
        val next = resolve(setting, Locale.getDefault().language)
        if (next == code) return
        words = if (next == "en") emptyMap() else runCatching {
            context.assets.open("strings.tsv").bufferedReader().use { column(it.readText(), next) }
        }.getOrDefault(emptyMap())
        code = next
    }

    /** For tests: use these words. */
    internal fun use(code: String, words: Map<String, String>) {
        this.code = code
        this.words = words
    }

    /** The language for a setting: the one chosen, or with "auto" the phone's when Islet has it; else English. */
    fun resolve(setting: String, system: String): String {
        fun has(c: String) = c != "auto" && LANGUAGES.any { it.first == c }
        if (setting != "auto" && has(setting)) return setting
        val base = system.split('-', '_', '.').first().lowercase()
        return if (has(base)) base else "en"
    }

    /** One language's column of the table, keyed by the English. */
    fun column(table: String, code: String): Map<String, String> {
        val lines = table.lines()
        val col = lines.firstOrNull()?.split('\t')?.indexOf(code) ?: -1
        if (col <= 0) return emptyMap()
        val out = HashMap<String, String>()
        for (line in lines.drop(1)) {
            if (line.isEmpty() || line.startsWith("#")) continue
            val cells = line.split('\t')
            val tr = cells.getOrNull(col)
            if (!tr.isNullOrEmpty()) out[cells[0]] = tr
        }
        return out
    }

    /** The English text of a key: the key without its note. */
    fun english(key: String): String = key.substringBefore('‖').trimEnd()

    /** A sentence in the language in use. */
    fun t(key: String): String = words[key] ?: english(key)

    /** A sentence with values: each "{}" filled in order, or "{0}", "{1}" by position. */
    fun f(key: String, vararg args: Any?): String = fill(t(key), args)

    fun fill(template: String, args: Array<out Any?>): String {
        var next = 0
        return Regex("""\{(\d*)\}""").replace(template) { m ->
            val n = m.groupValues[1]
            val i = if (n.isEmpty()) next++ else n.toInt()
            args.getOrNull(i)?.toString() ?: ""
        }
    }

    /**
     * A date in the language in use, from a skeleton such as "EEEEdMMMM"
     * (each language puts the parts in its own order).
     */
    fun date(skeleton: String, ms: Long): String {
        val loc = locale()
        val pattern = android.text.format.DateFormat.getBestDateTimePattern(loc, skeleton)
        return android.icu.text.SimpleDateFormat(pattern, loc).format(java.util.Date(ms))
    }

    /** A time zone's city ("Europe/Paris") in the language in use, as Android names it. */
    fun city(zone: String): String {
        val fallback = zone.substringAfterLast('/').replace('_', ' ')
        return runCatching { android.icu.text.TimeZoneNames.getInstance(locale()).getExemplarLocationName(zone) }
            .getOrNull()?.takeIf { it.isNotBlank() } ?: fallback
    }

    /** The locale for dates and numbers in the language in use. */
    fun locale(): Locale = when (code) {
        "zh" -> Locale.SIMPLIFIED_CHINESE
        else -> Locale.forLanguageTag(code)
    }
}
