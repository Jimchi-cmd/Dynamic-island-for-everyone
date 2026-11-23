package io.github.jimchi_cmd.islet

import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.SystemClock
import android.view.WindowManager
import islet.island.Live
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/** The phone app reads the shared translation table and speaks the language picked. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w411dp-h891dp-xxhdpi")
class LanguageTest {
    private val ctx = RuntimeEnvironment.getApplication()
    private val table = File("../../locales/strings.tsv").readText()

    @After
    fun english() {
        T.use("en", emptyMap())
        islet.island.Words.tr = { it.substringBefore('‖').trimEnd() }
    }

    @Test
    fun the_table_is_packaged_and_read() {
        T.load(ctx, "de")
        assertEquals("de", T.code)
        assertEquals("Kontrollzentrum", T.t("Control Centre"))
        assertEquals("Wecker um 7:30", T.f("Alarm at {}", "7:30"))
        T.load(ctx, "en")
        assertEquals("Control Centre", T.t("Control Centre"))
    }

    @Test
    fun picking_and_filling() {
        assertEquals("es", T.resolve("auto", "es"))
        assertEquals("en", T.resolve("auto", "sw"))
        assertEquals("fr", T.resolve("fr", "de"))
        assertEquals("ja", T.resolve("xx", "ja"))
        assertEquals("b a", T.fill("{1} {0}", arrayOf("a", "b")))
        assertEquals("May", T.english("May‖short month"))
        val ar = T.column(table, "ar")
        assertTrue(ar.size > 800)
        // Every language in the picker has a column, and the other way round.
        assertEquals(T.LANGUAGES.drop(1).map { it.first }, table.lines().first().split('\t'))
    }

    @Test
    fun the_island_module_speaks_too() {
        T.use("fr", T.column(table, "fr"))
        islet.island.Words.tr = { T.t(it) }
        assertEquals("dans 5 min", islet.island.Format.until(5 * 60_000L))
        assertEquals("Pluie", islet.island.Forecast.words(63))
        // Cities and the calendar's weekdays follow the language picked in Islet, not the phone's.
        assertEquals("Londres", T.city("Europe/London"))
        assertEquals("lun.", java.text.DateFormatSymbols.getInstance(T.locale()).shortWeekdays[java.util.Calendar.MONDAY])
    }

    @Test
    fun pictures_in_other_languages() {
        // Set ISLET_SHOTS to a folder to save the control centre and music card in a few languages.
        val dir = System.getenv("ISLET_SHOTS") ?: return
        Isle.init(ctx)
        val dm = ctx.resources.displayMetrics
        val tall = Bitmap.createBitmap(dm.widthPixels, (430 * dm.density).toInt(), Bitmap.Config.ARGB_8888)
        for (code in listOf("es", "zh", "ar", "hi")) {
            T.use(code, T.column(table, code))
            islet.island.Words.tr = { T.t(it) }
            Isle.island.all.forEach { Isle.island.remove(it.id) }
            val v = IslandWindow(ctx, WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY).view
            Isle.onChange = { v.update() }
            Isle.island.put(Live.Music("Midnight Drive", "The Example Band", "Player", true, 215_000, 46_000, SystemClock.elapsedRealtime()))
            Isle.island.open("music")
            Isle.island.showControls()
            v.update()
            v.settle()
            tall.eraseColor(0xff2b2b33.toInt())
            v.draw(Canvas(tall))
            File(dir).mkdirs()
            File(dir, "lang-$code-control-centre.png").outputStream().use { tall.compress(Bitmap.CompressFormat.PNG, 100, it) }
            Isle.island.close(0)
            Isle.island.open("music")
            v.update()
            v.settle()
            tall.eraseColor(0xff2b2b33.toInt())
            v.draw(Canvas(tall))
            File(dir, "lang-$code-music.png").outputStream().use { tall.compress(Bitmap.CompressFormat.PNG, 100, it) }
            Isle.island.close(0)
            Isle.island.remove("music")
            Isle.onChange = null
        }
    }
}
