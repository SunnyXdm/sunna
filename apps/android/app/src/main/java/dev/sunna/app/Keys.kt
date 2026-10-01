package dev.sunna.app

import android.annotation.SuppressLint
import android.annotation.TargetApi
import android.content.Context
import android.graphics.Color
import android.os.SystemClock
import android.text.Editable
import android.text.InputType
import android.text.Selection
import android.text.SpannableStringBuilder
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import android.view.inputmethod.TextAttribute
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

/** Where typing goes: text (returns how many characters had no key), and
 *  keys by their Mac keycode. */
interface KeySink {
    fun typed(text: String): Int
    /** A key the user pressed, with the modifiers on in the key bar. */
    fun key(code: Int)
    /** A key that's part of retyping an edit (an arrow to it, a backspace
     *  over the old word): the modifiers in the key bar stay for the next. */
    fun plainKey(code: Int)
    /** Ctrl, Alt or ⌘ is on in the key bar: the next key is a shortcut. */
    val shortcut: Boolean
    /** Some typed characters aren't on a US keyboard (é, emoji). */
    fun cantType()
}

/** What a US keyboard types: printable ASCII, tab and newline. */
fun typeable(c: Char) = c in ' '..'~' || c == '\t' || c == '\n'

/** The characters Shift turns these into on a US keyboard. */
fun shifted(text: String): String {
    val plain = "`1234567890-=[]\\;',./"
    val shift = "~!@#$%^&*()_+{}|:\"<>?"
    return text.map { c ->
        val i = plain.indexOf(c)
        when {
            c in 'a'..'z' -> c.uppercaseChar()
            i >= 0 -> shift[i]
            else -> c
        }
    }.joinToString("")
}

/**
 * What the on-screen keyboard types into: nothing shows here; every key goes
 * to the host as key presses.
 *
 * Without suggestions (the default) the keyboard is a password field's: no
 * corrections, each key arrives as it's typed. With them, the keyboard edits
 * a copy of what's been typed here, and whatever an edit changed around the
 * cursor (a corrected word, a swiped one) is retyped on the host: arrows to
 * the change, backspaces, the new text. A click on the host moves its cursor
 * where we can't follow, so it starts the copy over ([reset]).
 */
@SuppressLint("ViewConstructor")
class KeyInput(context: Context, private val sink: KeySink) : View(context) {
    var suggestions = false
        set(value) {
            if (field == value) return
            field = value
            reset()
        }
    private val copy = SpannableStringBuilder()
    /** The keyboard's connection, when it's [Plain]. */
    private var plain: Plain? = null

    init {
        isFocusable = true
        isFocusableInTouchMode = true
    }

    override fun onCheckIsTextEditor() = true

    /** The host's cursor moved where we can't follow: start the copy over
     *  (and forget a word being composed, typed where the cursor was). */
    fun reset() {
        if (copy.isEmpty() && plain?.composing.isNullOrEmpty() && !suggestions) return
        plain?.composing = ""
        copy.clear()
        Selection.setSelection(copy, 0)
        context.getSystemService(InputMethodManager::class.java)?.restartInput(this)
    }

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection {
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI or EditorInfo.IME_FLAG_NO_FULLSCREEN or EditorInfo.IME_ACTION_NONE or EditorInfo.IME_FLAG_NO_ENTER_ACTION
        if (suggestions) {
            outAttrs.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_AUTO_CORRECT
            outAttrs.initialSelStart = Selection.getSelectionStart(copy).coerceAtLeast(0)
            outAttrs.initialSelEnd = Selection.getSelectionEnd(copy).coerceAtLeast(0)
            plain = null
            return Mirror()
        }
        outAttrs.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        return Plain().also { plain = it }
    }

    /** Keys the keyboard sends as key events: special keys, or a character. */
    private fun keyEvent(event: KeyEvent, backspace: () -> Unit): Boolean {
        if (event.action != KeyEvent.ACTION_DOWN) return true
        when (event.keyCode) {
            KeyEvent.KEYCODE_DEL -> backspace()
            KeyEvent.KEYCODE_FORWARD_DEL -> sink.key(MacKey.FORWARD_DELETE)
            KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> {
                sink.key(MacKey.RETURN)
                reset()
            }
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

    /** No suggestions: nothing kept, every key straight over. Keyboards that
     *  compose anyway are followed by retyping the difference. */
    private inner class Plain : BaseInputConnection(this@KeyInput, false) {
        /** What of the text being composed has been typed on the host: only
         *  characters a US keyboard has, so a correction backspaces over
         *  exactly those. */
        var composing = ""

        private fun become(text: String) {
            val typeable = text.filter(::typeable)
            if (typeable.length != text.length) sink.cantType()
            val common = composing.commonPrefixWith(typeable)
            repeat(composing.length - common.length) { sink.plainKey(MacKey.BACKSPACE) }
            val added = typeable.substring(common.length)
            if (added.isNotEmpty()) sink.typed(added)
            composing = typeable
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

        override fun sendKeyEvent(event: KeyEvent) = keyEvent(event) { sink.key(MacKey.BACKSPACE) }

        // Nothing is kept here, so there's nothing to show the keyboard.
        override fun getTextBeforeCursor(n: Int, flags: Int): CharSequence = ""
        override fun getTextAfterCursor(n: Int, flags: Int): CharSequence = ""
    }

    /** Suggestions: the keyboard edits [copy]; each edit is replayed. */
    private inner class Mirror : BaseInputConnection(this@KeyInput, true) {
        /** Inside an edit being replayed: what it does through other calls is
         *  part of it, replayed once at the end. */
        private var replaying = false

        override fun getEditable(): Editable = copy

        private fun <T> replayed(edit: () -> T): T {
            if (replaying) return edit()
            val before = copy.toString()
            val cursor = Selection.getSelectionEnd(copy).coerceIn(0, before.length)
            replaying = true
            val result = try {
                edit()
            } finally {
                replaying = false
            }
            val after = copy.toString()
            val cursorAfter = Selection.getSelectionEnd(copy).coerceIn(0, after.length)
            if (before != after) replay(before, cursor, after, cursorAfter) else arrows(cursorAfter - cursor)
            return result
        }

        /** The host's text and cursor were `before` and `cursor`; make them `after`. */
        private fun replay(before: String, cursor: Int, after: String, cursorAfter: Int) {
            var prefix = before.commonPrefixWith(after).length
            var suffix = before.commonSuffixWith(after).length
            // A change shorter than its surroundings can match on both sides.
            suffix = suffix.coerceAtMost(minOf(before.length, after.length) - prefix)
            prefix = prefix.coerceAtMost(before.length - suffix)
            val oldEnd = before.length - suffix
            val inserted = after.substring(prefix, after.length - suffix)
            if (sink.shortcut) {
                // A key with Ctrl, Alt or ⌘ is a shortcut on the host, not
                // an edit of its text: send the key pressed (the last
                // character, not a correction the keyboard made with it),
                // and start the copy over.
                if (inserted.isNotEmpty()) sink.typed(inserted.substring(inserted.offsetByCodePoints(inserted.length, -1)))
                else if (oldEnd > prefix) sink.key(MacKey.BACKSPACE)
                post { reset() }
                return
            }
            arrows(oldEnd - cursor)
            repeat(oldEnd - prefix) { sink.plainKey(MacKey.BACKSPACE) }
            if (inserted.isNotEmpty() && sink.typed(inserted) > 0) {
                // Something the host's keyboard can't type: our copy no longer
                // matches, so start over from here.
                post { reset() }
                return
            }
            arrows(cursorAfter - (prefix + inserted.length))
        }

        private fun arrows(by: Int) = repeat(kotlin.math.abs(by)) { sink.plainKey(if (by < 0) MacKey.LEFT else MacKey.RIGHT) }

        override fun commitText(text: CharSequence, newCursorPosition: Int) = replayed { super.commitText(text, newCursorPosition) }
        override fun setComposingText(text: CharSequence, newCursorPosition: Int) = replayed { super.setComposingText(text, newCursorPosition) }
        override fun commitCorrection(correctionInfo: android.view.inputmethod.CorrectionInfo?) = replayed { super.commitCorrection(correctionInfo) }

        // A picked suggestion, on Android 14 and later.
        @TargetApi(34)
        override fun replaceText(start: Int, end: Int, text: CharSequence, newCursorPosition: Int, textAttribute: TextAttribute?) =
            replayed { super.replaceText(start, end, text, newCursorPosition, textAttribute) }

        // The keyboard moved the cursor (its space bar slides): so do arrows.
        override fun setSelection(start: Int, end: Int) = replayed { super.setSelection(start, end) }

        override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
            // Past either end of what we've typed here: the host has text
            // there we don't know of.
            val cursor = Selection.getSelectionStart(copy).coerceAtLeast(0)
            val known = copy.length - Selection.getSelectionEnd(copy).coerceAtLeast(cursor)
            if (!replaying) {
                beyond(beforeLength - cursor, MacKey.BACKSPACE)
                beyond(afterLength - known, MacKey.FORWARD_DELETE)
            }
            return replayed { super.deleteSurroundingText(beforeLength.coerceAtMost(cursor), afterLength.coerceAtMost(known)) }
        }

        /** One press is the user's (with the key bar's modifiers); more are a
         *  word or so the keyboard deletes. */
        private fun beyond(count: Int, code: Int) {
            if (count == 1) sink.key(code) else repeat(count.coerceIn(0, 64)) { sink.plainKey(code) }
        }

        override fun deleteSurroundingTextInCodePoints(beforeLength: Int, afterLength: Int): Boolean {
            // The same in characters: those the copy has, as its UTF-16 units,
            // and any beyond it one each.
            val start = Selection.getSelectionStart(copy).coerceAtLeast(0)
            val end = Selection.getSelectionEnd(copy).coerceAtLeast(start)
            val had = Character.codePointCount(copy, 0, start)
            val before = beforeLength.coerceAtLeast(0)
            val units = start - Character.offsetByCodePoints(copy, start, -minOf(before, had))
            val hadAfter = Character.codePointCount(copy, end, copy.length)
            val after = afterLength.coerceAtLeast(0)
            val unitsAfter = Character.offsetByCodePoints(copy, end, minOf(after, hadAfter)) - end
            return deleteSurroundingText(units + (before - had).coerceAtLeast(0), unitsAfter + (after - hadAfter).coerceAtLeast(0))
        }

        /** Keys the keyboard sends as key events change the copy as they
         *  change the host's text; where the copy can't follow the host's
         *  cursor (off its ends, up, down, a shortcut), it starts over. */
        override fun sendKeyEvent(event: KeyEvent): Boolean {
            if (event.action != KeyEvent.ACTION_DOWN) return true
            val start = Selection.getSelectionStart(copy).coerceAtLeast(0)
            val end = Selection.getSelectionEnd(copy).coerceAtLeast(start)
            val char = event.unicodeChar
            val code = event.keyCode
            val enter = code == KeyEvent.KEYCODE_ENTER || code == KeyEvent.KEYCODE_NUMPAD_ENTER
            when {
                sink.shortcut -> {
                    keyEvent(event) { sink.key(MacKey.BACKSPACE) }
                    if (!enter) post { reset() }
                }
                code == KeyEvent.KEYCODE_DEL && end > 0 -> replayed { remove(if (start < end) start else end - 1, end) }
                code == KeyEvent.KEYCODE_FORWARD_DEL && end < copy.length -> replayed { remove(start, if (start < end) end else end + 1) }
                code == KeyEvent.KEYCODE_DPAD_LEFT && end > 0 -> replayed { Selection.setSelection(copy, end - 1) }
                code == KeyEvent.KEYCODE_DPAD_RIGHT && end < copy.length -> replayed { Selection.setSelection(copy, end + 1) }
                code == KeyEvent.KEYCODE_DEL -> sink.key(MacKey.BACKSPACE)
                code == KeyEvent.KEYCODE_FORWARD_DEL -> sink.key(MacKey.FORWARD_DELETE)
                char != 0 && !enter && code != KeyEvent.KEYCODE_TAB -> {
                    val text = String(Character.toChars(char))
                    replayed {
                        copy.replace(start, end, text)
                        Selection.setSelection(copy, start + text.length)
                    }
                }
                else -> {
                    keyEvent(event) { sink.key(MacKey.BACKSPACE) }
                    if (!enter) post { reset() }
                }
            }
            return true
        }

        private fun remove(from: Int, to: Int) {
            copy.delete(from, to)
            Selection.setSelection(copy, from)
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

    /** Ctrl, Alt or ⌘ (Super) is on for the next key. */
    val shortcut: Boolean get() = modifiers.any { it.hold != Hold.OFF && it.code != MacKey.SHIFT }

    /** Hold the modifiers that are on around `press`, then let go of the
     *  ones that were only for this key. */
    fun withModifiers(press: () -> Unit) = around(modifiers.filter { it.hold != Hold.OFF }, press)

    /** Type `text` with the modifiers that are on. Shift isn't held for
     *  text it changes: typing presses and lets go of Shift for capitals
     *  itself, which would let go of ours mid-text; the shifted characters
     *  are typed instead. Text it doesn't change (a space, a tab, a return)
     *  is typed with Shift held, for shortcuts like Ctrl+Shift+Space. */
    fun typing(text: String, type: (String) -> Int): Int {
        val on = modifiers.filter { it.hold != Hold.OFF }
        val shift = on.any { it.code == MacKey.SHIFT } && shifted(text) != text
        var skipped = 0
        around(on.filter { !shift || it.code != MacKey.SHIFT }) { skipped = type(if (shift) shifted(text) else text) }
        if (shift) letGo(on.filter { it.code == MacKey.SHIFT })
        return skipped
    }

    private fun around(on: List<Modifier>, press: () -> Unit) {
        for (modifier in on) send(modifier.code, true)
        press()
        for (modifier in on.asReversed()) send(modifier.code, false)
        letGo(on)
    }

    /** The modifiers that held only for one key turn off. */
    private fun letGo(used: List<Modifier>) {
        for (modifier in used.filter { it.hold == Hold.NEXT }) {
            modifier.hold = Hold.OFF
            paint(modifier)
        }
    }
}
