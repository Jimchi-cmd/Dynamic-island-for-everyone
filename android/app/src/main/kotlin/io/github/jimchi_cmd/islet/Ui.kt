package io.github.jimchi_cmd.islet

import android.animation.ValueAnimator
import android.annotation.SuppressLint
import android.content.Context
import android.content.res.Configuration
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.Shader
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.content.res.ColorStateList
import android.util.TypedValue
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView

/**
 * The look of Islet's Settings on the computer, for the phone: grouped
 * cards on a soft background, rows with a label on the left and a switch
 * or value on the right, coloured icon tiles, light and dark.
 */
class Theme(context: Context) {
    val dark = context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES
    val bg = if (dark) 0xff1b1b1f.toInt() else 0xfff2f2f6.toInt()
    val panel = if (dark) 0xff29292e.toInt() else Color.WHITE
    val text = if (dark) 0xfff5f5f7.toInt() else 0xff1d1d1f.toInt()
    val muted = if (dark) 0xffa1a1a8.toInt() else 0xff6e6e73.toInt()
    val line = if (dark) 0x1affffff else 0x243c3c43
    val field = if (dark) 0x3d767680 else 0x1f767680
    val accent = if (dark) 0xff3b93ff.toInt() else 0xff0a6fe8.toInt()
    val on = if (dark) 0xff32c95f.toInt() else 0xff2fb656.toInt()
    val off = if (dark) 0x6b787880 else 0x4d787880
    val ok = if (dark) 0xff5ad27a.toInt() else 0xff1e8e3e.toInt()
    val warn = if (dark) 0xffffb340.toInt() else 0xffb25e00.toInt()
    val danger = if (dark) 0xffff6961.toInt() else 0xffc5221f.toInt()
}

/** Page tiles' colours, as in the computer's sidebar. */
object Tints {
    val grey = intArrayOf(0xffa2a2a9.toInt(), 0xff77777e.toInt())
    val blue = intArrayOf(0xff46a4ff.toInt(), 0xff0a6fe8.toInt())
    val orange = intArrayOf(0xffffb340.toInt(), 0xffff8a00.toInt())
    val purple = intArrayOf(0xff8a86ff.toInt(), 0xff5a55e0.toInt())
    val green = intArrayOf(0xff4cd97b.toInt(), 0xff1fa64d.toInt())
    val pink = intArrayOf(0xffff7a8a.toInt(), 0xffe0355a.toInt())
    val cyan = intArrayOf(0xff54d3e6.toInt(), 0xff1a9fbd.toInt())
    val red = intArrayOf(0xffff6b5e.toInt(), 0xffe0352b.toInt())
    val yellow = intArrayOf(0xffffd84a.toInt(), 0xffffb000.toInt())
    val slate = intArrayOf(0xff8f97a8.toInt(), 0xff5d6475.toInt())
}

/** A rounded square in a page's colour, with its icon. */
@SuppressLint("ViewConstructor")
class IconTile(context: Context, private val icon: Icon, private val tint: IntArray, private val sizeDp: Float = 30f) : View(context) {
    private val dp = resources.displayMetrics.density
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val box = RectF()

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val s = (sizeDp * dp).toInt()
        setMeasuredDimension(s, s)
    }

    override fun onDraw(c: Canvas) {
        val s = width.toFloat()
        paint.shader = LinearGradient(0f, 0f, s * 0.3f, s, tint[0], tint[1], Shader.TileMode.CLAMP)
        box.set(0f, 0f, s, s)
        c.drawRoundRect(box, s * 0.24f, s * 0.24f, paint)
        Icons.draw(c, icon, s / 2, s / 2, s * 0.6f, Color.WHITE)
    }
}

/** An on/off switch like the computer's: green when on. */
class Toggle(context: Context, private val theme: Theme) : View(context) {
    private val dp = resources.displayMetrics.density
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val box = RectF()
    private var t = 0f
    var onChange: ((Boolean) -> Unit)? = null

    var checked = false
        set(v) {
            if (field == v) return
            field = v
            ValueAnimator.ofFloat(t, if (v) 1f else 0f).apply {
                duration = 180
                addUpdateListener {
                    t = it.animatedValue as Float
                    invalidate()
                }
            }.start()
            contentDescription = if (v) T.t("On") else T.t("Off")
        }

    /** Show a state without animating or reporting a change. */
    fun show(v: Boolean) {
        checked = v
        t = if (v) 1f else 0f
        invalidate()
    }

    init {
        isClickable = true
        isFocusable = true
        setOnClickListener {
            checked = !checked
            onChange?.invoke(checked)
        }
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) =
        setMeasuredDimension((51 * dp).toInt(), (31 * dp).toInt())

    override fun onDraw(c: Canvas) {
        val w = width.toFloat()
        val h = height.toFloat()
        paint.color = blend(theme.off, theme.on, t)
        box.set(0f, 0f, w, h)
        c.drawRoundRect(box, h / 2, h / 2, paint)
        paint.color = Color.WHITE
        val r = h / 2 - 2 * dp
        val cx = h / 2 + (w - h) * t
        paint.setShadowLayer(2 * dp, 0f, 1 * dp, 0x33000000)
        c.drawCircle(cx, h / 2, r, paint)
        paint.clearShadowLayer()
    }

    private fun blend(a: Int, b: Int, f: Float): Int {
        fun ch(shift: Int) = (((a shr shift) and 0xff) * (1 - f) + ((b shr shift) and 0xff) * f).toInt() shl shift
        return ch(24) or ch(16) or ch(8) or ch(0)
    }

    override fun onTouchEvent(event: MotionEvent): Boolean = super.onTouchEvent(event)
}

/** Builds the screen's pieces in Islet's look. */
class Kit(private val context: Context, val theme: Theme) {
    private val dm = context.resources.displayMetrics

    fun dp(v: Number) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v.toFloat(), dm).toInt()

    fun text(s: CharSequence, sp: Float = 16f, color: Int = theme.text, bold: Boolean = false) = TextView(context).apply {
        text = s
        setTextSize(TypedValue.COMPLEX_UNIT_SP, sp)
        setTextColor(color)
        if (bold) typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
        setLineSpacing(0f, 1.1f)
    }

    fun rounded(color: Int, radiusDp: Number, stroke: Int? = null) = GradientDrawable().apply {
        setColor(color)
        cornerRadius = dp(radiusDp).toFloat()
        if (stroke != null) setStroke(dp(1), stroke)
    }

    private fun pressable(view: View, color: Int, radiusDp: Number) {
        view.background = RippleDrawable(ColorStateList.valueOf(theme.field), rounded(color, radiusDp), rounded(Color.WHITE, radiusDp))
    }

    fun column() = LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }

    fun lp(w: Int = ViewGroup.LayoutParams.MATCH_PARENT, h: Int = ViewGroup.LayoutParams.WRAP_CONTENT, top: Int = 0) =
        LinearLayout.LayoutParams(w, h).apply { topMargin = dp(top) }

    /** A white (or dark grey) rounded card holding rows, with hairlines between them. */
    inner class Card(val view: LinearLayout) {
        private var rows = 0

        fun add(row: View) {
            if (rows > 0) {
                view.addView(View(context).apply { setBackgroundColor(theme.line) }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 1).apply { marginStart = dp(16) })
            }
            view.addView(row, lp())
            rows++
        }
    }

    fun card(into: LinearLayout, title: String? = null, footer: String? = null, top: Int = 18): Card {
        if (title != null) into.addView(text(title.uppercase(), 13f, theme.muted).apply { letterSpacing = 0.04f; setPadding(dp(16), 0, dp(16), dp(6)) }, lp(top = top))
        val box = column().apply { background = rounded(theme.panel, 12) }
        into.addView(box, lp(top = if (title == null) top else 0))
        if (footer != null) into.addView(text(footer, 13f, theme.muted).apply { setPadding(dp(16), dp(6), dp(16), 0) }, lp())
        return Card(box)
    }

    /** One line: an optional tile, a title (and a grey note under it), and something on the right. */
    fun row(title: String, note: String? = null, tile: View? = null, right: View? = null, onClick: (() -> Unit)? = null): View {
        val r = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            minimumHeight = dp(48)
            setPadding(dp(16), dp(10), dp(14), dp(10))
        }
        if (tile != null) r.addView(tile, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { marginEnd = dp(12) })
        val words = column()
        words.addView(text(title, 16f))
        if (note != null) words.addView(text(note, 13f, theme.muted), lp(top = 2))
        r.addView(words, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        if (right != null) r.addView(right, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { marginStart = dp(10) })
        if (onClick != null) {
            pressable(r, Color.TRANSPARENT, 12)
            r.setOnClickListener { onClick() }
        }
        return r
    }

    fun toggleRow(title: String, note: String? = null, get: Boolean, set: (Boolean) -> Unit): View {
        val t = Toggle(context, theme)
        t.show(get)
        t.onChange = set
        t.contentDescription = title
        return row(title, note, right = t, onClick = { t.performClick() })
    }

    /** A grey value and a chevron: opens a choice. */
    fun valueRow(title: String, value: String, note: String? = null, tile: View? = null, onClick: () -> Unit): View {
        val v = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(text(value, 15f, theme.muted).apply { maxLines = 1; maxWidth = dp(150); ellipsize = android.text.TextUtils.TruncateAt.END })
            addView(chevron(), LinearLayout.LayoutParams(dp(18), dp(18)).apply { marginStart = dp(4) })
        }
        return row(title, note, tile, v, onClick)
    }

    fun chevron() = object : View(context) {
        private val p = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeWidth = dp(2).toFloat()
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
            color = theme.muted
        }

        override fun onDraw(c: Canvas) {
            val w = width.toFloat()
            val h = height.toFloat()
            c.drawLine(w * 0.38f, h * 0.22f, w * 0.68f, h * 0.5f, p)
            c.drawLine(w * 0.68f, h * 0.5f, w * 0.38f, h * 0.78f, p)
        }
    }

    /** Side-by-side choices, the chosen one raised, like the computer's size buttons. */
    fun segmented(options: List<Pair<String, String>>, chosen: String, pick: (String) -> Unit): View {
        val bar = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            background = rounded(theme.field, 9)
            setPadding(dp(2), dp(2), dp(2), dp(2))
        }
        for ((id, name) in options) {
            val seg = text(name, 14f, theme.text, bold = id == chosen).apply {
                gravity = Gravity.CENTER
                setPadding(dp(6), dp(7), dp(6), dp(7))
                if (id == chosen) background = rounded(if (theme.dark) 0xff636366.toInt() else Color.WHITE, 7)
                setOnClickListener { pick(id) }
                contentDescription = if (id == chosen) T.f("{}, chosen", name) else name
            }
            bar.addView(seg, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        }
        return FrameLayout(context).apply {
            setPadding(dp(16), dp(8), dp(16), dp(10))
            addView(bar)
        }
    }

    /** − value + */
    fun stepperRow(title: String, note: String? = null, value: () -> String, step: (Int) -> Unit): View {
        val shown = text(value(), 15f, theme.muted).apply { gravity = Gravity.CENTER; minWidth = dp(64) }
        fun b(label: String, d: Int) = text(label, 18f, theme.text).apply {
            gravity = Gravity.CENTER
            background = rounded(theme.field, 15)
            contentDescription = if (d < 0) T.t("Less") else T.t("More")
            setOnClickListener {
                step(d)
                shown.text = value()
            }
        }
        val box = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(b("−", -1), LinearLayout.LayoutParams(dp(32), dp(30)))
            addView(shown)
            addView(b("+", 1), LinearLayout.LayoutParams(dp(32), dp(30)))
        }
        return row(title, note, right = box)
    }

    /** A filled blue button, or a grey one. */
    fun button(label: String, primary: Boolean = false, onClick: () -> Unit) = text(label, 15f, if (primary) Color.WHITE else theme.text, bold = true).apply {
        gravity = Gravity.CENTER
        setPadding(dp(16), dp(9), dp(16), dp(9))
        pressable(this, if (primary) theme.accent else theme.field, 9)
        setOnClickListener { onClick() }
    }

    /** Buttons in a row inside a card. */
    fun buttons(vararg bs: View): View {
        val r = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            setPadding(dp(16), dp(10), dp(16), dp(12))
        }
        bs.forEachIndexed { i, b ->
            r.addView(b, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { if (i > 0) marginStart = dp(8) })
        }
        return android.widget.HorizontalScrollView(context).apply {
            isHorizontalScrollBarEnabled = false
            addView(r)
        }
    }

    /** A paragraph inside a card. */
    fun para(s: String, color: Int = theme.muted, sp: Float = 14f) = text(s, sp, color).apply { setPadding(dp(16), dp(12), dp(16), dp(12)) }

    /** A tick or an empty circle. */
    fun status(done: Boolean) = object : View(context) {
        private val p = Paint(Paint.ANTI_ALIAS_FLAG)

        override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(dp(24), dp(24))

        override fun onDraw(c: Canvas) {
            val r = width / 2f
            if (done) {
                p.style = Paint.Style.FILL
                p.color = theme.on
                c.drawCircle(r, r, r, p)
                Icons.draw(c, Icon.Check, r, r, r * 1.3f, Color.WHITE)
            } else {
                p.style = Paint.Style.STROKE
                p.strokeWidth = dp(2).toFloat()
                p.color = theme.off
                c.drawCircle(r, r, r - dp(1), p)
            }
        }
    }
}
