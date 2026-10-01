package dev.sunna.app

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.os.SystemClock
import android.text.InputType
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.widget.FrameLayout
import android.widget.HorizontalScrollView
import android.widget.LinearLayout
import android.widget.TextView

/** Mac virtual keycodes (Carbon `kVK_*`), what the wire carries. */
object MacKey {
    const val RETURN = 0x24
    const val TAB = 0x30
    const val BACKSPACE = 0x33
    const val ESCAPE = 0x35
    const val COMMAND = 0x37
    const val SHIFT = 0x38
    const val OPTION = 0x3A
    const val CONTROL = 0x3B
    const val HOME = 0x73
    const val PAGE_UP = 0x74
    const val FORWARD_DELETE = 0x75
    const val END = 0x77
    const val PAGE_DOWN = 0x79
    const val LEFT = 0x7B
    const val RIGHT = 0x7C
    const val DOWN = 0x7D
    const val UP = 0x7E
    val F = intArrayOf(0x7A, 0x78, 0x63, 0x76, 0x60, 0x61, 0x62, 0x64, 0x65, 0x6D, 0x67, 0x6F)
}

/** Where typing goes: text, and keys by their Mac keycode. */
interface KeySink {
    fun typed(text: String)
    fun key(code: Int)
}

/**
 * What the on-screen keyboard types into: nothing shows here, every
 * character goes to the host as key presses. Suggestions and autocorrect
 * are off (a password field's keyboard), so each key arrives as it's typed;
 * keyboards that compose anyway are followed by retyping the difference.
 */
@SuppressLint("ViewConstructor")
class KeyInput(context: Context, private val sink: KeySink) : View(context) {
    init {
        isFocusable = true
        isFocusableInTouchMode = true
    }

    override fun onCheckIsTextEditor() = true

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection {
        outAttrs.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI or EditorInfo.IME_FLAG_NO_FULLSCREEN or EditorInfo.IME_ACTION_NONE or EditorInfo.IME_FLAG_NO_ENTER_ACTION
        return object : BaseInputConnection(this, false) {
            private var composing = ""

            private fun become(text: String) {
                val common = composing.commonPrefixWith(text)
                repeat(composing.length - common.length) { sink.key(MacKey.BACKSPACE) }
                val added = text.substring(common.length)
                if (added.isNotEmpty()) sink.typed(added)
                composing = text
            }

            override fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {
                become(text.toString())
                composing = ""
                return true
            }

            override fun setComposingText(text: CharSequence, newCursorPosition: Int): Boolean {
                become(text.toString())
                return true
            }

            override fun finishComposingText(): Boolean {
                composing = ""
                return true
            }

            override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
                repeat(beforeLength.coerceAtMost(64)) { sink.key(MacKey.BACKSPACE) }
                repeat(afterLength.coerceAtMost(64)) { sink.key(MacKey.FORWARD_DELETE) }
                return true
            }

            override fun sendKeyEvent(event: KeyEvent): Boolean {
                if (event.action != KeyEvent.ACTION_DOWN) return true
                when (event.keyCode) {
                    KeyEvent.KEYCODE_DEL -> sink.key(MacKey.BACKSPACE)
                    KeyEvent.KEYCODE_FORWARD_DEL -> sink.key(MacKey.FORWARD_DELETE)
                    KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> sink.key(MacKey.RETURN)
                    KeyEvent.KEYCODE_TAB -> sink.key(MacKey.TAB)
                    KeyEvent.KEYCODE_ESCAPE -> sink.key(MacKey.ESCAPE)
                    KeyEvent.KEYCODE_DPAD_LEFT -> sink.key(MacKey.LEFT)
                    KeyEvent.KEYCODE_DPAD_RIGHT -> sink.key(MacKey.RIGHT)
                    KeyEvent.KEYCODE_DPAD_UP -> sink.key(MacKey.UP)
                    KeyEvent.KEYCODE_DPAD_DOWN -> sink.key(MacKey.DOWN)
                    else -> {
                        val char = event.unicodeChar
                        if (char != 0) sink.typed(String(Character.toChars(char)))
                    }
                }
                return true
            }

            // Nothing is kept here, so there's nothing to show the keyboard.
            override fun getTextBeforeCursor(n: Int, flags: Int): CharSequence = ""
            override fun getTextAfterCursor(n: Int, flags: Int): CharSequence = ""
        }
    }
}

/**
 * Keys a phone's keyboard doesn't have, above it: Esc, Tab, the modifiers
 * (named as the host names them), arrows and the rest. A modifier holds for
 * the next key; tap it twice to keep it down.
 */
@SuppressLint("ViewConstructor")
class KeyBar(context: Context, mac: Boolean, private val send: (Int, Boolean) -> Unit, private val typeKey: (Int) -> Unit, onHide: () -> Unit) : LinearLayout(context) {
    private enum class Hold { OFF, NEXT, LOCKED }

    private inner class Modifier(val code: Int, val label: String) {
        var hold = Hold.OFF
        var tappedAt = 0L
        lateinit var view: TextView
    }

    private val modifiers = if (mac) {
        listOf(Modifier(MacKey.CONTROL, "⌃"), Modifier(MacKey.OPTION, "⌥"), Modifier(MacKey.COMMAND, "⌘"), Modifier(MacKey.SHIFT, "⇧"))
    } else {
        listOf(Modifier(MacKey.CONTROL, "Ctrl"), Modifier(MacKey.OPTION, "Alt"), Modifier(MacKey.COMMAND, "Super"), Modifier(MacKey.SHIFT, "Shift"))
    }

    init {
        orientation = HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        setBackgroundColor(0xF51A1B20.toInt())
        setPadding(dpi(6f), dpi(6f), dpi(4f), dpi(6f))
        val row = LinearLayout(context).apply {
            orientation = HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        row.addView(key("esc") { typeKey(MacKey.ESCAPE) })
        row.addView(key("tab") { typeKey(MacKey.TAB) })
        for (modifier in modifiers) {
            modifier.view = key(modifier.label, wide = !mac) { toggle(modifier) }
            row.addView(modifier.view)
        }
        row.addView(key("←") { typeKey(MacKey.LEFT) })
        row.addView(key("↑") { typeKey(MacKey.UP) })
        row.addView(key("↓") { typeKey(MacKey.DOWN) })
        row.addView(key("→") { typeKey(MacKey.RIGHT) })
        row.addView(key(if (mac) "⌦" else "Del") { typeKey(MacKey.FORWARD_DELETE) })
        row.addView(key("home", wide = true) { typeKey(MacKey.HOME) })
        row.addView(key("end", wide = true) { typeKey(MacKey.END) })
        row.addView(key("pg↑", wide = true) { typeKey(MacKey.PAGE_UP) })
        row.addView(key("pg↓", wide = true) { typeKey(MacKey.PAGE_DOWN) })
        for ((i, code) in MacKey.F.withIndex()) row.addView(key("F${i + 1}") { typeKey(code) })
        val scroller = HorizontalScrollView(context).apply {
            isHorizontalScrollBarEnabled = false
            overScrollMode = OVER_SCROLL_NEVER
            addView(row)
        }
        addView(scroller, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
        addView(FrameLayout(context).apply {
            background = ripple(null, dp(18f))
            addView(context.icon(R.drawable.ic_keyboard_hide, Palette.TEXT_2, 22f), FrameLayout.LayoutParams(dpi(22f), dpi(22f), Gravity.CENTER))
            contentDescription = "Hide keyboard"
            setOnClickListener { onHide() }
        }, LayoutParams(dpi(44f), dpi(40f)))
    }

    private fun key(label: String, wide: Boolean = false, onTap: () -> Unit): TextView = context.text(15f, Palette.TEXT, Type.medium).apply {
        text = label
        gravity = Gravity.CENTER
        minWidth = dpi(if (wide) 54f else 44f)
        minHeight = dpi(38f)
        setPadding(dpi(8f), 0, dpi(8f), 0)
        background = ripple(rounded(white(.09f), dp(9f)), dp(9f))
        layoutParams = LayoutParams(LayoutParams.WRAP_CONTENT, dpi(38f)).apply { marginEnd = dpi(5f) }
        contentDescription = label
        setOnClickListener {
            performHapticFeedback(android.view.HapticFeedbackConstants.KEYBOARD_TAP)
            onTap()
        }
    }

    private fun toggle(modifier: Modifier) {
        val now = SystemClock.uptimeMillis()
        modifier.hold = when (modifier.hold) {
            Hold.OFF -> Hold.NEXT
            Hold.NEXT -> if (now - modifier.tappedAt < 400) Hold.LOCKED else Hold.OFF
            Hold.LOCKED -> Hold.OFF
        }
        modifier.tappedAt = now
        paint(modifier)
    }

    private fun paint(modifier: Modifier) {
        val view = modifier.view
        when (modifier.hold) {
            Hold.OFF -> {
                view.background = ripple(rounded(white(.09f), dp(9f)), dp(9f))
                view.setTextColor(Palette.TEXT)
            }
            Hold.NEXT -> {
                view.background = ripple(rounded(withAlpha(Palette.accent, .18f), dp(9f), Palette.accent, dpi(1.5f)), dp(9f))
                view.setTextColor(Palette.accent)
            }
            Hold.LOCKED -> {
                view.background = ripple(rounded(Palette.accent, dp(9f)), dp(9f))
                view.setTextColor(Color.BLACK)
            }
        }
    }

    /** Hold the modifiers that are on around `press`, then let go of the
     *  ones that were only for this key. */
    fun withModifiers(press: () -> Unit) {
        val on = modifiers.filter { it.hold != Hold.OFF }
        for (modifier in on) send(modifier.code, true)
        press()
        for (modifier in on.asReversed()) send(modifier.code, false)
        for (modifier in on.filter { it.hold == Hold.NEXT }) {
            modifier.hold = Hold.OFF
            paint(modifier)
        }
    }
}
