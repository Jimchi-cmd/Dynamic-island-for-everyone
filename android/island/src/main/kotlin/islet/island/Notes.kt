package islet.island

/** What the island needs to know about a notification, read off Android's. */
data class NoteFacts(
    val key: String,
    val pkg: String,
    val app: String,
    val category: String? = null,
    val ongoing: Boolean = false,
    val groupSummary: Boolean = false,
    val secret: Boolean = false,
    val title: String = "",
    val text: String = "",
    val progress: Int = 0,
    val progressMax: Int = 0,
    val indeterminate: Boolean = false,
    /** It shows a running clock (`setUsesChronometer`). */
    val chronometer: Boolean = false,
    /** That clock counts down to `whenMs`. */
    val countDown: Boolean = false,
    val whenMs: Long = 0,
    /** Built with MediaStyle (a music player's controls). */
    val media: Boolean = false,
    /** Built with CallStyle (Android 12+). */
    val callStyle: Boolean = false,
    /** The buttons' labels, lower case. */
    val actions: List<String> = emptyList(),
)

/** What the island makes of a notification. */
sealed class NoteKind {
    data class Call(val name: String, val incoming: Boolean, val since: Long) : NoteKind()

    data class Navigation(val title: String, val text: String) : NoteKind()

    data class Countdown(val label: String, val endsAt: Long) : NoteKind()

    data class Progress(val title: String, val percent: Int) : NoteKind()

    /** Worth a pop-up. */
    object Popup : NoteKind()

    /** A wallet app's payment: "Paid", with the amount. */
    data class Payment(val merchant: String, val amount: String) : NoteKind()

    object Ignore : NoteKind()
}

object Notes {
    /** Maps apps whose ongoing notification is the next turn. */
    val MAPS = setOf(
        "com.google.android.apps.maps",
        "com.google.android.apps.mapslite",
        "com.waze",
        "com.here.app.maps",
        "net.osmand",
        "net.osmand.plus",
        "app.organicmaps",
        "com.mapswithme.maps.pro",
        "com.sygic.aura",
        "ru.yandex.yandexnavi",
    )

    private val ANSWER = listOf("answer", "accept", "pick up")

    /** Wallet apps whose notification after a tap-to-pay names the amount. */
    val WALLETS = setOf(
        "com.google.android.apps.walletnfcrel",
        "com.google.android.apps.nbu.paisa.user",
        "com.samsung.android.spay",
        "com.paypal.android.p2pmobile",
        "com.revolut.revolut",
        "com.transferwise.android",
    )

    private const val CODES = "USD|EUR|GBP|UGX|KES|TZS|RWF|NGN|ZAR|INR|CAD|AUD"

    /** An amount of money: $12.50, 12,50 €, UGX 5,000, £3. */
    private val AMOUNT = Regex(
        """(?:[$€£¥₹₦]\s?|\b(?:$CODES)\s?)\d(?:[\d,.\s]*\d)?|\d(?:[\d,.\s]*\d)?\s?(?:[€£₽]|\b(?:$CODES)\b)""",
    )

    /** The amount in a payment's text, if there is one. */
    fun amount(s: String): String? = AMOUNT.find(s)?.value?.trim()

    fun classify(n: NoteFacts, ownPackage: String): NoteKind {
        if (n.pkg == ownPackage) return NoteKind.Ignore
        val title = n.title.trim()
        val text = n.text.trim()
        if (n.category == "call" || n.callStyle) {
            val ringing = n.actions.any { a -> ANSWER.any { a.contains(it) } }
            // A call's clock starts when it is answered.
            val since = if (!ringing && n.chronometer && n.whenMs > 0) n.whenMs else 0L
            return NoteKind.Call(title.ifEmpty { "Call" }, ringing, since)
        }
        if (n.ongoing && (n.category == "navigation" || n.pkg in MAPS) && (title.isNotEmpty() || text.isNotEmpty())) {
            return NoteKind.Navigation(title, text)
        }
        if (n.ongoing && n.chronometer && n.countDown && n.whenMs > 0) {
            return NoteKind.Countdown(title.ifEmpty { n.app }, n.whenMs)
        }
        if (n.progressMax > 0 || n.indeterminate) {
            if (n.media) return NoteKind.Ignore
            val percent = if (n.indeterminate || n.progressMax <= 0) -1 else (n.progress * 100L / n.progressMax).toInt().coerceIn(0, 100)
            return NoteKind.Progress(title.ifEmpty { n.app }, percent)
        }
        if (n.media || n.ongoing || n.groupSummary || n.secret) return NoteKind.Ignore
        if (n.pkg in WALLETS) {
            val inTitle = amount(title)
            val amount = inTitle ?: amount(text)
            if (amount != null) return NoteKind.Payment((if (inTitle != null) text else title).take(40), amount)
        }
        when (n.category) {
            "transport", "service", "status", "sys", "progress", "recommendation" -> return NoteKind.Ignore
        }
        if (title.isEmpty() && text.isEmpty()) return NoteKind.Ignore
        return NoteKind.Popup
    }
}
