package dev.sunna.app

import android.annotation.SuppressLint
import android.content.Context
import android.provider.Settings
import android.text.Editable
import android.text.TextWatcher
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.ArrayAdapter
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.ScrollView
import android.widget.TextView

/** Settings: what this phone is called, how sessions start, and about Sunna.
 *  It slides in over the Computers screen; Back or ‹ closes it. */
@SuppressLint("ViewConstructor")
class SettingsView(context: Context, private val app: App, private val onClosed: () -> Unit) : FrameLayout(context) {
    private val header = LinearLayout(context)
    private val body = LinearLayout(context)
    private val scroll = ScrollView(context)
    private var closing = false

    init {
        setBackgroundColor(Palette.BG)
        isClickable = true
        body.orientation = LinearLayout.VERTICAL
        scroll.isVerticalScrollBarEnabled = false
        scroll.addView(body)
        addView(scroll, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))

        header.apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setBackgroundColor(Palette.BG)
            addView(FrameLayout(context).apply {
                background = ripple(null, dp(22f))
                addView(context.icon(R.drawable.ic_back, Palette.TEXT, 24f), LayoutParams(dpi(24f), dpi(24f), Gravity.CENTER))
                contentDescription = "Back"
                setOnClickListener { close() }
            }, LinearLayout.LayoutParams(dpi(44f), dpi(44f)).apply { marginEnd = dpi(6f) })
            addView(context.text(24f, Palette.TEXT, Type.bold).apply {
                text = "Settings"
                letterSpacing = -0.01f
            })
        }
        addView(header, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT, Gravity.TOP))

        // This phone.
        body.addView(context.sectionTitle("This phone"))
        val ownName = Settings.Global.getString(context.contentResolver, Settings.Global.DEVICE_NAME)?.ifEmpty { null }
            ?: "${android.os.Build.MANUFACTURER.replaceFirstChar { it.uppercase() }} ${android.os.Build.MODEL}"
        val name = Field(context, ownName).apply {
            input.setText(app.prefs.deviceName)
            input.inputType = android.text.InputType.TYPE_CLASS_TEXT or android.text.InputType.TYPE_TEXT_FLAG_CAP_WORDS
            input.imeOptions = android.view.inputmethod.EditorInfo.IME_ACTION_DONE or android.view.inputmethod.EditorInfo.IME_FLAG_NO_EXTRACT_UI
            input.addTextChangedListener(object : TextWatcher {
                override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
                override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
                override fun afterTextChanged(s: Editable?) {
                    app.prefs.deviceName = s?.toString() ?: ""
                }
            })
        }
        body.addView(name)
        body.addView(note("What computers call this phone while it's connected: “$ownName is connected to it.”"))

        // Sessions.
        body.addView(context.sectionTitle("Sessions"))
        body.addView(LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            background = rounded(white(.05f), dp(16f))
            setPadding(dpi(14f), dpi(6f), dpi(14f), dpi(10f))
            addView(ChoiceRow(context, "Touch", listOf("Trackpad" to "trackpad", "Touch" to "touch"), { app.prefs.touchMode }) { app.prefs.touchMode = it })
        })
        body.addView(note("Trackpad: drag to move the pointer, tap to click. Touch: tap where you want to click."))
        body.addView(context.group(
            SwitchRow(context, "Sound", "Play the computer's sound on this phone", { app.prefs.sound }) { app.prefs.sound = it },
            SwitchRow(context, "Share the clipboard", "Copy on one, paste on the other, both ways", { app.prefs.clipboard }) { app.prefs.clipboard = it },
            SwitchRow(context, "Keyboard button", "Next to ••• at the top of sessions, for the keyboard in one tap", { app.prefs.keyboardButton }) { app.prefs.keyboardButton = it },
            SwitchRow(context, "Keyboard suggestions", "Corrections and swipe typing from your keyboard; Sunna retypes what changes", { app.prefs.suggestions }) { app.prefs.suggestions = it },
            SwitchRow(context, "Stats bar", "Latency, frame rate and bitrate during sessions", { app.prefs.stats }) { app.prefs.stats = it },
        ), LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = dpi(12f) })

        // Video.
        body.addView(context.sectionTitle("Video"))
        body.addView(LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            background = rounded(white(.05f), dp(16f))
            setPadding(dpi(14f), dpi(6f), dpi(14f), dpi(10f))
            for (row in context.videoChoices(app.prefs) {}) addView(row)
        })
        body.addView(note("How sessions start; the session menu changes them for the session you're in."))

        // About.
        body.addView(context.sectionTitle("About"))
        val version = runCatching { context.packageManager.getPackageInfo(context.packageName, 0).versionName }.getOrNull() ?: ""
        body.addView(context.group(
            context.linkRow("Open source software", "The software Sunna is made with, and its licenses") { licenses() },
        ))
        body.addView(note("Sunna $version · protocol ${Native.protocol()}"))
    }

    private fun note(text: String) = context.text(13f, Palette.TEXT_3).apply {
        this.text = text
        setLineSpacing(0f, 1.2f)
        setPadding(dpi(6f), dpi(8f), dpi(6f), 0)
    }

    fun setInsets(insets: Insets) {
        val side = dpi(20f)
        header.setPadding(insets.left + dpi(8f), insets.top + dpi(10f), insets.right + side, dpi(12f))
        body.setPadding(insets.left + side, insets.top + dpi(10f) + dpi(52f), insets.right + side, insets.bottom + dpi(40f))
    }

    fun open(host: FrameLayout) {
        host.addView(this, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        host.rootWindowInsets?.let { setInsets(Insets.of(it)) }
        translationX = resources.displayMetrics.widthPixels.toFloat()
        animate().translationX(0f).setDuration(Motion.ZOOM).setInterpolator(Motion.snappy).start()
    }

    fun close() {
        if (closing) return
        closing = true
        val imm = context.getSystemService(android.view.inputmethod.InputMethodManager::class.java)
        imm?.hideSoftInputFromWindow(windowToken, 0)
        animate().translationX(width.toFloat()).setDuration(260).setInterpolator(Motion.easeIn).withEndAction {
            (parent as? ViewGroup)?.removeView(this)
            onClosed()
        }.start()
    }

    /** The open source software in Sunna, with its licenses: a long text, so
     *  it's a list that draws only what's on screen. */
    private fun licenses() {
        val host = parent as? FrameLayout ?: return
        val sheet = Sheet(context, host)
        val text = runCatching { context.assets.open("licenses.txt").bufferedReader().use { it.readText() } }.getOrDefault("")
        val parts = text.split("\n\n").filter { it.isNotBlank() }
        val list = ListView(context).apply {
            divider = null
            isVerticalScrollBarEnabled = true
            adapter = object : ArrayAdapter<String>(context, 0, parts) {
                override fun getView(position: Int, convertView: View?, parent: ViewGroup): View {
                    val view = (convertView as? TextView) ?: context.text(12.5f, Palette.TEXT_2).apply {
                        typeface = Type.mono
                        setPadding(dpi(20f), dpi(6f), dpi(20f), dpi(6f))
                    }
                    view.text = getItem(position)
                    return view
                }
            }
        }
        val body = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            addView(sheet.header("Open Source Software"))
            addView(list, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, (resources.displayMetrics.heightPixels * 0.7f).toInt()))
        }
        sheet.setContent(body)
        sheet.open()
    }
}
