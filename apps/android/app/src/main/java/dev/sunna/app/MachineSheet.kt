package dev.sunna.app

import android.annotation.SuppressLint
import android.content.Context
import android.os.Handler
import android.os.Looper
import android.text.Editable
import android.text.InputType
import android.text.TextUtils
import android.text.TextWatcher
import android.view.Gravity
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import org.json.JSONObject

/**
 * Add a computer, or edit one: its address (or the link it printed), its key
 * and a name. The preview lights up as soon as the computer answers, and says
 * what's wrong when it can't let us in.
 */
@SuppressLint("ViewConstructor")
class MachineSheet(
    context: Context,
    host: FrameLayout,
    private val app: App,
    private val machine: Machine?,
    focusKey: Boolean,
    link: String?,
    private val onSaved: (Machine, Boolean) -> Unit,
) : Sheet(context, host) {
    private val screen = ScreenView(context)
    private val previewName = context.text(16f, Palette.TEXT, Type.semibold)
    private val previewDot = Dot(context)
    private val previewStatus = context.text(13.5f, Palette.TEXT_2)
    private val address = Field(context, "100.101.102.103, a name, or a sunna:// link", mono = false)
    private val key = Field(context, "The key it shows when it starts sharing", secret = true)
    private val name = Field(context, "Name (optional)")
    private val message = context.text(14f, Palette.TEXT_2)
    private val handler = Handler(Looper.getMainLooper())
    private var check: Check? = null
    /** The address and key the last check was for. */
    private var checked: Pair<String, String>? = null
    private var sequence = 0
    private var revealed = false
    private var filling = false

    init {
        val body = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            clipChildren = false
        }
        body.addView(header(if (machine == null) "Add a Computer" else "Edit “${machine.name}”"))

        // The preview: the screen it'll have in the grid.
        val preview = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            clipChildren = false
            setPadding(0, dpi(4f), 0, dpi(18f))
        }
        val frame = AspectFrame(context, 16f / 11f)
        frame.clipChildren = false
        frame.addView(screen)
        preview.addView(frame, LinearLayout.LayoutParams(dpi(220f), LinearLayout.LayoutParams.WRAP_CONTENT))
        preview.addView(previewName.apply {
            gravity = Gravity.CENTER
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
            setPadding(dpi(20f), dpi(6f), dpi(20f), 0)
        })
        preview.addView(LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
            setPadding(0, dpi(5f), 0, 0)
            addView(previewDot, LinearLayout.LayoutParams(dpi(7f), dpi(7f)).apply { marginEnd = dpi(7f) })
            addView(previewStatus)
        })
        body.addView(preview)

        // The fields.
        val fields = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dpi(20f), 0, dpi(20f), 0)
        }
        fields.addView(label("Address"))
        fields.addView(address)
        fields.addView(label("Key"))
        val eye = ImageView(context).apply {
            setImageResource(R.drawable.ic_eye)
            imageTintList = android.content.res.ColorStateList.valueOf(Palette.TEXT_2)
            setPadding(dpi(10f), dpi(10f), dpi(10f), dpi(10f))
            background = ripple(null, dp(20f))
            contentDescription = "Show key"
            setOnClickListener {
                revealed = !revealed
                val at = key.input.selectionEnd
                key.input.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS or
                    if (revealed) InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD else InputType.TYPE_TEXT_VARIATION_PASSWORD
                key.input.typeface = if (key.input.text.isEmpty()) Type.regular else Type.mono
                key.input.setSelection(at.coerceIn(0, key.input.text.length))
                setImageResource(if (revealed) R.drawable.ic_eye_off else R.drawable.ic_eye)
                contentDescription = if (revealed) "Hide key" else "Show key"
            }
        }
        key.addView(eye, LinearLayout.LayoutParams(dpi(40f), dpi(40f)))
        fields.addView(key)
        fields.addView(label("Name"))
        fields.addView(name)
        name.input.imeOptions = EditorInfo.IME_ACTION_DONE or EditorInfo.IME_FLAG_NO_EXTRACT_UI
        name.input.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_WORDS
        name.input.setOnEditorActionListener { _, action, _ ->
            if (action == EditorInfo.IME_ACTION_DONE) save()
            action == EditorInfo.IME_ACTION_DONE
        }
        fields.addView(message.apply {
            setLineSpacing(0f, 1.2f)
            setPadding(dpi(2f), dpi(12f), dpi(2f), 0)
            visibility = View.GONE
        })
        body.addView(fields)

        // Buttons.
        val buttons = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dpi(16f), dpi(22f), dpi(16f), dpi(4f))
        }
        if (machine != null) {
            buttons.addView(context.button("Remove", ButtonStyle.GHOST, R.drawable.ic_trash) { remove() }.apply {
                (getChildAt(0) as? ImageView)?.imageTintList = android.content.res.ColorStateList.valueOf(0xFFFF7B72.toInt())
                (getChildAt(1) as? android.widget.TextView)?.setTextColor(0xFFFF7B72.toInt())
            })
        }
        buttons.addView(View(context), LinearLayout.LayoutParams(0, 1, 1f))
        buttons.addView(context.button("Cancel", ButtonStyle.SECONDARY) { close() }, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { marginEnd = dpi(10f) })
        buttons.addView(context.button(if (machine == null) "Add" else "Save", ButtonStyle.PRIMARY) { save() })
        body.addView(buttons)

        val scroll = ScrollView(context).apply {
            isVerticalScrollBarEnabled = false
            addView(body)
        }
        setContent(scroll)

        // What's there already.
        if (machine != null) {
            address.input.setText(machine.address)
            key.input.setText(machine.key)
            name.input.setText(machine.name)
            check = app.checks[machine.id]
            checked = machine.address to machine.key
        }
        link?.let { fill(it) }
        val watcher = object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(s: Editable?) {
                if (filling) return
                address.error = false
                key.error = false
                if (address.input.text.contains("sunna://")) fill(address.input.text.toString())
                scheduleCheck()
            }
        }
        address.input.addTextChangedListener(watcher)
        key.input.addTextChangedListener(watcher)
        name.input.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(s: Editable?) = paintPreview()
        })
        paintPreview(animate = false)
        scheduleCheck(immediately = true)

        val focus = if (focusKey) key.input else if (machine == null && link == null) address.input else null
        if (focus != null) {
            handler.postDelayed({
                focus.requestFocus()
                if (focusKey) focus.selectAll()
                context.getSystemService(InputMethodManager::class.java)?.showSoftInput(focus, InputMethodManager.SHOW_IMPLICIT)
            }, 320)
        }
    }

    private fun label(text: String) = context.text(13f, Palette.TEXT_2, Type.medium).apply {
        this.text = text
        setPadding(dpi(2f), dpi(14f), 0, dpi(7f))
    }

    /** A pasted or opened `sunna://` link: its address and key go where they belong. */
    private fun fill(text: String) {
        val (host, secret) = splitLink(text) ?: return
        filling = true
        address.input.setText(host)
        address.input.setSelection(host.length)
        if (secret.isNotEmpty()) key.input.setText(secret)
        filling = false
        if (secret.isNotEmpty()) say("The key came with the link.", false)
    }

    private fun say(text: String?, problem: Boolean) {
        message.visibility = if (text.isNullOrEmpty()) View.GONE else View.VISIBLE
        message.text = text ?: ""
        message.setTextColor(if (problem) 0xFFFF7B72.toInt() else Palette.TEXT_2)
    }

    private fun scheduleCheck(immediately: Boolean = false) {
        handler.removeCallbacksAndMessages(CHECK)
        val where = address.input.text.toString().trim()
        val secret = key.input.text.toString().trim()
        val run = Runnable {
            val mine = ++sequence
            if (where.isEmpty()) {
                check = Check("idle")
                checked = null
                paintPreview()
                return@Runnable
            }
            // A computer we know the state of keeps it until the answer comes.
            val known = immediately && check != null && checked == where to secret
            if (!known) {
                check = Check.CHECKING
                paintPreview()
            }
            app.background {
                val result = Check.parse(Native.check(where, secret))
                handler.post {
                    if (mine != sequence || !isOpen) return@post
                    check = result
                    checked = where to secret
                    paintPreview()
                }
            }
        }
        if (immediately) run.run() else handler.postAtTime(run, CHECK, android.os.SystemClock.uptimeMillis() + 450)
    }

    private fun paintPreview(animate: Boolean = true) {
        val state = check?.state ?: "idle"
        val os = check?.os?.ifEmpty { null } ?: machine?.os ?: ""
        screen.os = Os.of(os)
        screen.device = check?.device?.ifEmpty { null } ?: machine?.device ?: ""
        screen.bootName = Os.bootName(os)
        val w = check?.width?.takeIf { it > 0 } ?: machine?.width ?: 0
        val h = check?.height?.takeIf { it > 0 } ?: machine?.height ?: 0
        screen.ratio = if (w > 0 && h > 0) w.toFloat() / h else 1.6f
        if (screen.state != state) screen.show(state, 0, animate)
        val typed = name.input.text.toString().trim()
        val suggested = check?.name?.ifEmpty { null } ?: hostOf(address.input.text.toString()).ifEmpty { null }
        name.input.hint = suggested?.let { "$it (from the computer)" } ?: "Name (optional)"
        previewName.text = typed.ifEmpty { suggested ?: "New Computer" }
        // Seen at its old address doesn't say anything about a new one.
        val moved = machine != null && machine.address != address.input.text.toString().trim()
        previewStatus.text = describe(if (moved) machine?.copy(lastSeen = 0) else machine, check ?: Check("idle"))
        previewDot.color = statusColor(state)
        previewDot.pulsing = state == "checking"
        when (state) {
            "ready", "checking", "idle" -> if (message.currentTextColor == 0xFFFF7B72.toInt()) say(null, false)
            "busy" -> say(check?.detail, false)
            else -> say(check?.detail, true)
        }
    }

    private fun save() {
        val where = address.input.text.toString().trim()
        val secret = key.input.text.toString().trim()
        val parsed = runCatching { JSONObject(Native.parseAddress(where)) }.getOrNull()
        val problem = parsed?.optString("error")?.ifEmpty { null }
        if (problem != null || parsed == null) {
            address.error = true
            address.shake()
            say(problem ?: "That doesn't look like an address.", true)
            return
        }
        if (secret.contains('\n')) {
            key.error = true
            key.shake()
            say("The key can't contain line breaks.", true)
            return
        }
        val about = check?.takeIf { checked == where to secret && it.state != "checking" }
        val draft = Draft(name.input.text.toString(), where, secret, about)
        val saved = if (machine == null) app.store.add(draft) else app.store.update(machine.id, draft)
        if (saved == null) {
            say("That computer is no longer in your list.", true)
            return
        }
        about?.let { app.checks[saved.id] = it }
        close()
        onSaved(saved, machine == null)
    }

    private fun remove() {
        val machine = machine ?: return
        hideKeyboard()
        confirm(context, parent as FrameLayout, "Remove “${machine.name}”?", "You can add it again with its address and key.", "Remove") {
            app.store.remove(machine.id)
            app.checks.remove(machine.id)
            close()
            onSaved(machine, false)
        }
    }

    override fun close() {
        handler.removeCallbacksAndMessages(null)
        sequence++
        super.close()
    }

    private companion object {
        val CHECK = Any()
    }
}
