package io.github.jimchi_cmd.islet

import android.content.Intent
import android.graphics.drawable.Drawable
import android.net.Uri
import android.os.Bundle
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.view.inputmethod.EditorInfo
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import androidx.appcompat.app.AppCompatActivity
import islet.island.Launchable
import islet.island.Launcher

/**
 * The quick launcher, from the island's search button: type a few letters
 * of an app or one of your own links, and tap it (or press Enter for the
 * first). The island's own window can't take typing, so this opens over
 * whatever is on screen.
 */
class LauncherActivity : AppCompatActivity() {
    private lateinit var kit: Kit
    private lateinit var list: LinearLayout
    private var all: List<Launchable> = emptyList()
    private val icons = HashMap<String, Drawable?>()
    private var shown: List<Launchable> = emptyList()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Isle.init(this)
        kit = Kit(this, Theme(this))
        all = Isle.prefs.links.map { Launchable(it.name, it.url, isLink = true) } + apps()

        val box = EditText(this).apply {
            hint = T.t("Search apps and links")
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
            imeOptions = EditorInfo.IME_ACTION_GO
            setSingleLine()
            textSize = 17f
            setTextColor(kit.theme.text)
            setHintTextColor(kit.theme.muted)
            background = kit.rounded(kit.theme.field, 12)
            setPadding(kit.dp(14), kit.dp(10), kit.dp(14), kit.dp(10))
        }
        list = kit.column()
        val panel = kit.column().apply {
            background = kit.rounded(kit.theme.panel, 22)
            setPadding(kit.dp(14), kit.dp(14), kit.dp(14), kit.dp(10))
            elevation = kit.dp(12).toFloat()
            addView(box, kit.lp())
            addView(ScrollView(this@LauncherActivity).apply { addView(list) }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, kit.dp(380)).apply { topMargin = kit.dp(8) })
        }
        // A tap outside the panel closes it.
        val root = FrameLayout(this).apply {
            setBackgroundColor(0x66000000)
            setOnClickListener { finish() }
            addView(panel, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.TOP).apply {
                setMargins(kit.dp(12), kit.dp(56), kit.dp(12), 0)
            })
        }
        panel.setOnClickListener { }
        setContentView(root)

        box.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) = Unit
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) = Unit
            override fun afterTextChanged(s: Editable?) = show(s?.toString().orEmpty())
        })
        box.setOnEditorActionListener { _, id, e ->
            if (id == EditorInfo.IME_ACTION_GO || e?.keyCode == KeyEvent.KEYCODE_ENTER) {
                shown.firstOrNull()?.let { open(it) }
                true
            } else {
                false
            }
        }
        show("")
        window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_STATE_VISIBLE)
        box.requestFocus()
    }

    private fun apps(): List<Launchable> {
        val pm = packageManager
        val main = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        return pm.queryIntentActivities(main, 0)
            .map { Launchable(it.loadLabel(pm).toString(), it.activityInfo.packageName) }
            .filter { it.target != packageName }
            .distinctBy { it.target }
    }

    private fun show(query: String) {
        shown = Launcher.search(query, all)
        list.removeAllViews()
        if (shown.isEmpty()) {
            list.addView(kit.para(T.t("Nothing called that. Add your own links in Islet → Quick launcher.")))
            return
        }
        for (a in shown) {
            val icon: View = if (a.isLink) {
                IconTile(this, Icon.Link, Tints.blue, 30f)
            } else {
                ImageView(this).apply {
                    setImageDrawable(icons.getOrPut(a.target) { Isle.appIcon(a.target) })
                    layoutParams = LinearLayout.LayoutParams(kit.dp(30), kit.dp(30))
                }
            }
            list.addView(kit.row(a.name, if (a.isLink) a.target else null, tile = icon) { open(a) })
        }
    }

    private fun open(a: Launchable) {
        val ok = if (a.isLink) {
            Launcher.safeLink(a.target) && Isle.start(Intent(Intent.ACTION_VIEW, Uri.parse(a.target)))
        } else {
            Isle.launch(a.target)
        }
        if (!ok) Isle.pop(islet.island.Pop.Info(T.f("Couldn't open {}", a.name)))
        finish()
    }
}
