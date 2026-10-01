package dev.sunna.app

import android.annotation.SuppressLint
import android.app.Activity
import android.graphics.Color
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import org.json.JSONObject

/**
 * The session's menu: the keyboard, how touch works, sound, the host's own
 * shortcuts, the video, and Disconnect. Opened with ••• or Back.
 */
@SuppressLint("ViewConstructor")
class SessionMenu(
    activity: Activity,
    host: FrameLayout,
    private val app: App,
    private val session: SessionView,
    machine: Machine,
    check: Check,
    mac: Boolean,
) : Sheet(activity, host) {
    private val detail = context.text(13.5f, Palette.TEXT_2)
    private val actions = ArrayList<() -> Unit>()

    init {
        val body = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dpi(16f), 0, dpi(16f), dpi(4f))
        }

        // Who this is.
        body.addView(LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dpi(4f), 0, 0, dpi(16f))
            addView(Badge(context).apply { os = Os.of(check.os) }, LinearLayout.LayoutParams(dpi(30f), dpi(30f)).apply { marginEnd = dpi(12f) })
            addView(LinearLayout(context).apply {
                orientation = LinearLayout.VERTICAL
                addView(context.text(18f, Palette.TEXT, Type.bold).apply {
                    text = machine.name
                    maxLines = 1
                })
                addView(detail.apply { setPadding(0, dpi(3f), 0, 0) })
            }, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        })

        // The four things reached for most.
        val quick = LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL }
        quick.addView(action(R.drawable.ic_keyboard, { "Keyboard" }, { false }) {
            close()
            session.post { session.keyboard(true) }
        })
        quick.addView(action({ if (app.prefs.touchMode == "touch") R.drawable.ic_touch else R.drawable.ic_trackpad },
            { if (app.prefs.touchMode == "touch") "Touch" else "Trackpad" }, { false }) {
            session.setTouchMode(if (app.prefs.touchMode == "touch") "trackpad" else "touch")
            app.notice.show(if (app.prefs.touchMode == "touch") "Touch: tap where you want to click. Touch and hold for a right click." else "Trackpad: drag to move the pointer, tap to click, two fingers to scroll.")
        })
        quick.addView(action({ if (app.prefs.sound) R.drawable.ic_sound else R.drawable.ic_sound_off }, { if (app.prefs.sound) "Sound On" else "Sound Off" }, { app.prefs.sound }) {
            app.prefs.sound = !app.prefs.sound
            session.applyStream()
        })
        quick.addView(action({ R.drawable.ic_stats }, { "Stats" }, { app.prefs.stats }) {
            session.showStats(!app.prefs.stats)
        })
        body.addView(quick)

        // The host's own shortcuts.
        body.addView(section("Shortcuts"))
        val shortcuts = runCatching { Native.shortcuts(check.os) }.getOrDefault(emptyArray())
        val list = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            background = rounded(white(.05f), dp(16f))
        }
        shortcuts.forEachIndexed { index, title ->
            list.addView(row(title) {
                close()
                session.shortcut(index)
            })
            if (index < shortcuts.size - 1) list.addView(View(context).apply { setBackgroundColor(Palette.LINE) }, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 1).apply { marginStart = dpi(16f) })
        }
        body.addView(list)

        // The video.
        body.addView(section("Video"))
        val video = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            background = rounded(white(.05f), dp(16f))
            setPadding(dpi(14f), dpi(6f), dpi(14f), dpi(10f))
        }
        video.addView(choice("Sharpness", listOf("Full" to 100, "75%" to 75, "50%" to 50), { app.prefs.scale }) {
            app.prefs.scale = it
            session.applyStream()
        })
        video.addView(choice("Frame rate", listOf("60" to 60, "30" to 30), { app.prefs.fps }) {
            app.prefs.fps = it
            session.applyStream()
        })
        video.addView(choice("Bitrate", listOf("Auto" to 0, "10" to 10, "20" to 20, "40" to 40, "80" to 80), { app.prefs.bitrateMbps }) {
            app.prefs.bitrateMbps = it
            session.applyStream()
        })
        video.addView(choice("Codec", listOf("Auto" to "", "HEVC" to "hevc", "H.264" to "h264"), { app.prefs.codec }) {
            app.prefs.codec = it
            session.applyStream()
        })
        body.addView(video)

        body.addView(context.button("Disconnect", ButtonStyle.DESTRUCTIVE, R.drawable.ic_power) {
            close()
            session.leave()
        }, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = dpi(20f) })

        val scroll = ScrollView(context).apply {
            isVerticalScrollBarEnabled = false
            addView(body)
        }
        setContent(scroll)
        if (!mac) detail.text = check.os
    }

    fun update(status: JSONObject) {
        val w = status.optInt("width")
        val h = status.optInt("height")
        if (w == 0) return
        val fps = if (status.isNull("shownFps")) status.optInt("fps") else status.optDouble("shownFps").toInt()
        detail.text = "${codecName(status.optString("codec"))}  ·  $w×$h  ·  $fps fps"
    }

    private fun section(title: String) = context.text(13f, Palette.TEXT_3, Type.semibold).apply {
        text = title.uppercase()
        letterSpacing = 0.06f
        setPadding(dpi(6f), dpi(22f), 0, dpi(8f))
    }

    private fun refreshActions() = actions.forEach { it() }

    /** A square button: an icon over a label, lit when on. */
    private fun action(icon: () -> Int, label: () -> String, on: () -> Boolean, run: () -> Unit): View {
        val glyph = ImageView(context)
        val words: TextView = context.text(12.5f, Palette.TEXT, Type.medium).apply {
            gravity = Gravity.CENTER
            maxLines = 1
        }
        val view = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
            minimumHeight = dpi(74f)
            setPadding(dpi(4f), dpi(10f), dpi(4f), dpi(10f))
            addView(glyph, LinearLayout.LayoutParams(dpi(24f), dpi(24f)))
            addView(words, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = dpi(8f) })
            isClickable = true
            setOnClickListener {
                run()
                refreshActions()
            }
            pressable(0.94f)
        }
        val paint = {
            val lit = on()
            glyph.setImageResource(icon())
            glyph.imageTintList = android.content.res.ColorStateList.valueOf(if (lit) Color.BLACK else Palette.TEXT)
            words.text = label()
            words.setTextColor(if (lit) Color.BLACK else Palette.TEXT)
            view.background = ripple(rounded(if (lit) Palette.accent else white(.07f), dp(16f)), dp(16f))
            view.contentDescription = label()
        }
        actions += paint
        paint()
        view.layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f).apply {
            marginStart = dpi(4f)
            marginEnd = dpi(4f)
        }
        return view
    }

    private fun action(icon: Int, label: () -> String, on: () -> Boolean, run: () -> Unit) = action({ icon }, label, on, run)

    private fun row(title: String, run: () -> Unit) = LinearLayout(context).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        minimumHeight = dpi(50f)
        setPadding(dpi(16f), 0, dpi(16f), 0)
        background = ripple(null, dp(16f))
        addView(context.text(15.5f, Palette.TEXT).apply { text = title }, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        addView(context.icon(R.drawable.ic_arrow, Palette.TEXT_3, 16f), LinearLayout.LayoutParams(dpi(16f), dpi(16f)))
        setOnClickListener { run() }
    }

    /** A labeled row of choices, one of them chosen. */
    private fun <T> choice(title: String, options: List<Pair<String, T>>, current: () -> T, choose: (T) -> Unit): View {
        val row = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(0, dpi(8f), 0, 0)
        }
        row.addView(context.text(14.5f, Palette.TEXT).apply { text = title }, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        val group = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            background = rounded(0x40000000, dp(10f))
            setPadding(dpi(3f), dpi(3f), dpi(3f), dpi(3f))
        }
        val pills = options.map { (label, value) ->
            context.text(13.5f, Palette.TEXT_2, Type.medium).apply {
                text = label
                gravity = Gravity.CENTER
                minWidth = dpi(40f)
                setPadding(dpi(9f), dpi(7f), dpi(9f), dpi(7f))
                setOnClickListener {
                    choose(value)
                    refreshActions()
                }
            }.also { group.addView(it) } to value
        }
        val paint = {
            for ((pill, value) in pills) {
                val chosen = value == current()
                pill.background = if (chosen) rounded(white(.16f), dp(8f)) else null
                pill.setTextColor(if (chosen) Palette.TEXT else Palette.TEXT_2)
            }
        }
        actions += paint
        paint()
        row.addView(group)
        return row
    }
}
