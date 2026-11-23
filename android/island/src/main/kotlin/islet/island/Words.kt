package islet.island

/**
 * The words this module shows (weather, countdowns, streaks), passed
 * through the app's translations: the app points `tr` at its table, and
 * until then (and in tests) they stay English. Values go in as "{}".
 */
object Words {
    @Volatile var tr: (String) -> String = { it.substringBefore('‖').trimEnd() }

    fun t(key: String): String = tr(key)

    fun f(key: String, vararg args: Any?): String {
        var next = 0
        return Regex("""\{(\d*)\}""").replace(t(key)) { m ->
            val n = m.groupValues[1]
            args.getOrNull(if (n.isEmpty()) next++ else n.toInt())?.toString() ?: ""
        }
    }
}
