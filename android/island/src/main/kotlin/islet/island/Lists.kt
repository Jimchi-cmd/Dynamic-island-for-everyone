package islet.island

/** A to-do item. */
data class Todo(val text: String, val done: Boolean)

/**
 * The to-do list and other small lists the app keeps, one entry per line
 * so the stored form stays readable. Line breaks inside an entry become
 * spaces.
 */
object Lists {
    fun clean(s: String) = s.replace('\r', ' ').replace('\n', ' ').trim()

    fun encodeTodos(items: List<Todo>): String =
        items.joinToString("\n") { (if (it.done) "[x] " else "[ ] ") + clean(it.text) }

    fun decodeTodos(s: String?): List<Todo> =
        s.orEmpty().lineSequence().filter { it.length > 4 }.map {
            Todo(it.substring(4), it.startsWith("[x]"))
        }.toList()

    fun encode(items: List<String>): String = items.map { clean(it) }.filter { it.isNotEmpty() }.joinToString("\n")

    fun decode(s: String?): List<String> = s.orEmpty().lines().map { it.trim() }.filter { it.isNotEmpty() }

    /** The first unfinished to-do, for the clock card. */
    fun nextTodo(items: List<Todo>): String? = items.firstOrNull { !it.done }?.text
}
