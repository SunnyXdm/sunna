package dev.sunna.app

import android.annotation.SuppressLint
import android.content.Context
import android.view.Gravity
import android.view.View
import android.widget.LinearLayout
import android.widget.Switch
import android.widget.TextView

/** The rows of the session menu and Settings: grouped on a raised card, a
 *  title, sometimes a line below it, and a control. */

fun Context.sectionTitle(title: String): TextView = text(14f, Palette.TEXT_2, Type.semibold).apply {
    text = title
    setPadding(dpi(6f), dpi(22f), 0, dpi(8f))
}

/** Rows on a card, with hairlines between them. */
fun Context.group(vararg rows: View): LinearLayout = LinearLayout(this).apply {
    orientation = LinearLayout.VERTICAL
    background = rounded(white(.05f), dp(16f))
    rows.forEachIndexed { index, row ->
        if (index > 0) {
            addView(View(context).apply { setBackgroundColor(Palette.LINE) }, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 1).apply { marginStart = dpi(16f) })
        }
        addView(row)
    }
}

/** A title and a line below it, for the left of a row. */
private fun Context.label(title: String, detail: String?): LinearLayout = LinearLayout(this).apply {
    orientation = LinearLayout.VERTICAL
    addView(text(15.5f, Palette.TEXT).apply { text = title })
    if (!detail.isNullOrEmpty()) {
        addView(text(13f, Palette.TEXT_3).apply {
            text = detail
            setLineSpacing(0f, 1.15f)
            setPadding(0, dpi(3f), 0, 0)
        })
    }
}

/** A row that does something: an arrow on the right. */
fun Context.linkRow(title: String, detail: String? = null, run: () -> Unit): View = LinearLayout(this).apply {
    orientation = LinearLayout.HORIZONTAL
    gravity = Gravity.CENTER_VERTICAL
    minimumHeight = dpi(52f)
    setPadding(dpi(16f), dpi(8f), dpi(16f), dpi(8f))
    background = ripple(null, dp(16f))
    addView(label(title, detail), LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
    addView(icon(R.drawable.ic_arrow, Palette.TEXT_3, 16f), LinearLayout.LayoutParams(dpi(16f), dpi(16f)))
    setOnClickListener { run() }
}

/** A row with a switch. */
@SuppressLint("ViewConstructor")
class SwitchRow(context: Context, title: String, detail: String?, private val on: () -> Boolean, set: (Boolean) -> Unit) : LinearLayout(context) {
    private val toggle = Switch(context).apply {
        isClickable = false
        isFocusable = false
    }

    init {
        orientation = HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        minimumHeight = dpi(60f)
        setPadding(dpi(16f), dpi(8f), dpi(12f), dpi(8f))
        background = ripple(null, dp(16f))
        addView(context.label(title, detail), LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f).apply { marginEnd = dpi(12f) })
        addView(toggle)
        setOnClickListener {
            set(!on())
            repaint()
        }
        contentDescription = title
        repaint()
    }

    fun repaint() {
        toggle.isChecked = on()
    }
}

/** A row of choices, one of them chosen. */
@SuppressLint("ViewConstructor")
class ChoiceRow<T>(context: Context, title: String, options: List<Pair<String, T>>, private val current: () -> T, choose: (T) -> Unit) : LinearLayout(context) {
    private val pills: List<Pair<TextView, T>>

    init {
        orientation = HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        setPadding(0, dpi(8f), 0, 0)
        addView(context.text(14.5f, Palette.TEXT).apply { text = title }, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
        val group = LinearLayout(context).apply {
            orientation = HORIZONTAL
            background = rounded(0x40000000, dp(10f))
            setPadding(dpi(3f), dpi(3f), dpi(3f), dpi(3f))
        }
        pills = options.map { (label, value) ->
            context.text(13.5f, Palette.TEXT_2, Type.medium).apply {
                text = label
                gravity = Gravity.CENTER
                minWidth = dpi(40f)
                setPadding(dpi(9f), dpi(7f), dpi(9f), dpi(7f))
                contentDescription = "$title: $label"
                setOnClickListener {
                    choose(value)
                    repaint()
                }
            }.also { group.addView(it) } to value
        }
        addView(group)
        repaint()
    }

    fun repaint() {
        for ((pill, value) in pills) {
            val chosen = value == current()
            pill.background = if (chosen) rounded(white(.16f), dp(8f)) else null
            pill.setTextColor(if (chosen) Palette.TEXT else Palette.TEXT_2)
        }
    }
}

/** The video choices, the same in the session menu and Settings. */
fun Context.videoChoices(prefs: Prefs, changed: () -> Unit): List<View> = listOf(
    ChoiceRow(this, "Sharpness", listOf("Full" to 100, "75%" to 75, "50%" to 50), { prefs.scale }) {
        prefs.scale = it
        changed()
    },
    ChoiceRow(this, "Frame rate", listOf("60" to 60, "30" to 30), { prefs.fps }) {
        prefs.fps = it
        changed()
    },
    ChoiceRow(this, "Bitrate", listOf("Auto" to 0, "10" to 10, "20" to 20, "40" to 40, "80" to 80), { prefs.bitrateMbps }) {
        prefs.bitrateMbps = it
        changed()
    },
    ChoiceRow(this, "Codec", listOf("Auto" to "", "HEVC" to "hevc", "H.264" to "h264"), { prefs.codec }) {
        prefs.codec = it
        changed()
    },
)
