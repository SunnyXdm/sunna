package dev.sunna.app

import android.annotation.SuppressLint
import android.app.Activity
import android.graphics.Color
import android.graphics.RectF
import android.media.MediaCodecList
import android.os.Build
import android.os.SystemClock
import android.provider.Settings
import android.view.Choreographer
import android.view.Gravity
import android.view.HapticFeedbackConstants
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.PointerIcon
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.View
import android.view.WindowInsets
import android.view.WindowInsetsAnimation
import android.view.WindowInsetsController
import android.view.WindowManager
import android.view.inputmethod.InputMethodManager
import android.widget.FrameLayout
import android.widget.TextView
import org.json.JSONObject
import kotlin.math.max
import kotlin.math.roundToInt

/** What this phone's video decoders can do. */
object Decoders {
    private val decoders by lazy {
        runCatching { MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.filter { !it.isEncoder } }.getOrDefault(emptyList())
    }

    private fun of(mime: String) = decoders.filter { info -> info.supportedTypes.any { it.equals(mime, ignoreCase = true) } }

    /** A decoder on the phone's own video hardware. */
    fun hardware(mime: String): Boolean = of(mime).any { it.isHardwareAccelerated }

    /** The largest `width`×`height` with that shape the decoder plays. */
    fun fit(mime: String, width: Int, height: Int): Pair<Int, Int> {
        val caps = of(mime).sortedByDescending { it.isHardwareAccelerated }.firstOrNull()
            ?.getCapabilitiesForType(mime)?.videoCapabilities ?: return width to height
        var w = width
        var h = height
        while (w > 640 && !(caps.isSizeSupported(w, h) || caps.isSizeSupported(h, w))) {
            w = (w * 0.9f).roundToInt() and 1.inv()
            h = (h * 0.9f).roundToInt() and 1.inv()
        }
        return w to h
    }
}

/**
 * A session: the host's screen, full screen, with the pointer drawn where it
 * is, fingers and keys going to the host, and a small button for the menu.
 */
@SuppressLint("ViewConstructor", "ClickableViewAccessibility")
class SessionView(
    private val activity: Activity,
    private val app: App,
    val machine: Machine,
    private val check: Check,
    private val tile: () -> ScreenView?,
    private val onClosing: () -> Unit,
    private val onFinished: (SessionView, String?) -> Unit,
) : FrameLayout(activity), SurfaceHolder.Callback, Choreographer.FrameCallback, Touch.Remote, KeySink {
    private val surface = SurfaceView(activity)
    private val viewport = Viewport()
    private val pointer = PointerView(activity, viewport)
    private val keyInput = KeyInput(activity, this)
    private val mac = check.os.isEmpty() || check.os.lowercase().contains("mac")
    private val keyBar = KeyBar(activity, mac, { code, down -> if (session != 0L) Native.macKey(session, code, down) }, { key(it) }) { keyboard(false) }
    private val menuButton = FrameLayout(activity)
    private val stats: TextView = activity.text(11.5f, Color.WHITE, Type.medium)
    private val launch = LaunchOverlay(activity, tile() ?: ScreenView(activity).apply { os = Os.of(check.os) }, machine.name) { leave() }
    private val touch = Touch(this, this)
    private var session = 0L
    private var lastChanges = -2L
    private var lastStatusAt = 0L
    private var lastCursor = -1L
    private var streaming = false
    private var revealed = false
    private var ended = false
    private var leaving = false
    private var keyboardUp = false
    private var cutout = Insets()
    private var imeHeight = 0
    @Volatile
    private var codec = ""
    private var status = JSONObject()
    private var skippedNoted = false
    private var menu: SessionMenu? = null

    val active: Boolean get() = session != 0L && !ended

    init {
        isFocusable = true
        isFocusableInTouchMode = true
        // The picture starts as a pixel, so the decoder has a surface from the
        // first keyframe; it takes its place when the screen has grown.
        addView(surface, LayoutParams(1, 1))
        surface.holder.addCallback(this)
        addView(pointer, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        addView(keyInput, LayoutParams(1, 1))
        stats.apply {
            background = rounded(0xB3000000.toInt(), dp(10f))
            setPadding(dpi(10f), dpi(6f), dpi(10f), dpi(6f))
            visibility = View.GONE
        }
        addView(stats, LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT, Gravity.TOP or Gravity.START))
        menuButton.apply {
            background = ripple(rounded(0xB3141518.toInt(), dp(16f), white(.16f), dpi(1f)), dp(16f))
            addView(activity.icon(R.drawable.ic_more, Color.WHITE, 18f), LayoutParams(dpi(18f), dpi(18f), Gravity.CENTER))
            contentDescription = "Session menu"
            setOnClickListener { openMenu() }
            alpha = 0f
            visibility = View.GONE
        }
        addView(menuButton, LayoutParams(dpi(52f), dpi(32f), Gravity.TOP or Gravity.CENTER_HORIZONTAL))
        keyBar.visibility = View.GONE
        addView(keyBar, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT, Gravity.BOTTOM))
        addView(launch, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        pointerIcon = PointerIcon.getSystemIcon(activity, PointerIcon.TYPE_NULL)
        if (Build.VERSION.SDK_INT >= 30) {
            setWindowInsetsAnimationCallback(object : WindowInsetsAnimation.Callback(DISPATCH_MODE_CONTINUE_ON_SUBTREE) {
                override fun onProgress(insets: WindowInsets, running: MutableList<WindowInsetsAnimation>): WindowInsets {
                    placeKeyBar(insets.getInsets(WindowInsets.Type.ime()).bottom)
                    return insets
                }

                override fun onEnd(animation: WindowInsetsAnimation) {
                    rootWindowInsets?.let { placeKeyBar(it.getInsets(WindowInsets.Type.ime()).bottom) }
                }
            })
        }
        setOnApplyWindowInsetsListener { _, insets ->
            val all = Insets.of(insets)
            cutout = if (Build.VERSION.SDK_INT >= 28) {
                insets.displayCutout?.let { Insets(it.safeInsetLeft, it.safeInsetTop, it.safeInsetRight, it.safeInsetBottom) } ?: Insets()
            } else Insets()
            val ime = all.ime
            if (keyboardUp && ime == 0 && imeHeight > 0) keyboard(false, fromSystem = true)
            imeHeight = ime
            placeKeyBar(ime)
            placeChrome()
            placePicture()
            insets
        }
    }

    // ---- Starting and ending ---------------------------------------------------------

    fun start() {
        val from = tile()
        val rect = RectF()
        if (from != null && from.isAttachedToWindow) {
            val at = IntArray(2)
            val mine = IntArray(2)
            from.getLocationInWindow(at)
            getLocationInWindow(mine)
            val (screenRect, scale) = from.screenRect()
            rect.set(at[0] - mine[0] + screenRect.left * scale, at[1] - mine[1] + screenRect.top * scale,
                at[0] - mine[0] + screenRect.right * scale, at[1] - mine[1] + screenRect.bottom * scale)
        } else {
            rect.set(width * 0.3f, height * 0.3f, width * 0.7f, height * 0.7f)
        }
        launch.say("Reaching ${machine.name}…")
        launch.open(rect) {
            setBackgroundColor(Color.BLACK)
            if (streaming) revealPicture()
        }
        immersive(true)
        // Asking the phone about its decoders can take a moment: not on the
        // thread drawing the animation.
        val name = deviceName()
        val sound = app.prefs.sound
        app.background {
            codec = app.prefs.codec.ifEmpty { if (Decoders.hardware("video/hevc")) "hevc" else "h264" }
            val (w, h) = requestedSize()
            val handle = Native.connect(machine.address, machine.key, name, check.os, codec, w, h, sound)
            post {
                if (ended) {
                    Native.close(handle)
                } else {
                    session = handle
                    Choreographer.getInstance().postFrameCallback(this)
                }
            }
        }
    }

    /** The stream to ask for: the host's display (scaled as chosen), within
     *  what this phone's decoder plays. */
    private fun requestedSize(): Pair<Int, Int> {
        val scale = app.prefs.scale / 100f
        val (hostW, hostH) = if (check.width > 0 && check.height > 0) check.width to check.height else 3840 to 2400
        val w = ((hostW * scale).roundToInt()) and 1.inv()
        val h = ((hostH * scale).roundToInt()) and 1.inv()
        return Decoders.fit(if (codec == "hevc") "video/hevc" else "video/avc", w, h)
    }

    private fun deviceName(): String =
        Settings.Global.getString(activity.contentResolver, Settings.Global.DEVICE_NAME)?.ifEmpty { null }
            ?: "${Build.MANUFACTURER.replaceFirstChar { it.uppercase() }} ${Build.MODEL}"

    /** Leave: the host is told, and the screen goes back into its tile. */
    fun leave() {
        if (leaving) return
        leaving = true
        menu?.close()
        if (session != 0L) Native.leave(session)
        finish(null)
    }

    private fun finish(reason: String?) {
        ended = true
        keyboard(false)
        immersive(false)
        menuButton.visibility = View.GONE
        stats.visibility = View.GONE
        pointer.shown = false
        val to = tile()?.let { from ->
            if (!from.isAttachedToWindow) return@let null
            val at = IntArray(2)
            val mine = IntArray(2)
            from.getLocationInWindow(at)
            getLocationInWindow(mine)
            val (screenRect, scale) = from.screenRect()
            RectF(at[0] - mine[0] + screenRect.left * scale, at[1] - mine[1] + screenRect.top * scale,
                at[0] - mine[0] + screenRect.right * scale, at[1] - mine[1] + screenRect.bottom * scale)
        }
        if (reason != null) launch.say(if (streaming) "Disconnected" else reason.ifEmpty { "Disconnected" }, canCancel = false)
        val close: () -> Unit = {
            onClosing()
            launch.close(to) {
                if (session != 0L) Native.close(session)
                session = 0L
                onFinished(this, reason)
            }
            // The home screen shows around the screen as it shrinks.
            postDelayed({
                setBackgroundColor(Color.TRANSPARENT)
                surface.visibility = View.GONE
            }, 180)
        }
        if (reason != null && streaming) postDelayed(close, 500) else close()
    }

    private fun revealPicture() {
        if (revealed || !launch.expanded) return
        revealed = true
        placePicture()
        launch.reveal {
            menuButton.visibility = View.VISIBLE
            menuButton.animate().alpha(1f).setDuration(300).start()
            dimMenuButtonLater()
            stats.visibility = if (app.prefs.stats) View.VISIBLE else View.GONE
            requestFocus()
            if (!activity.getSharedPreferences("sunna", 0).getBoolean("hinted", false)) {
                activity.getSharedPreferences("sunna", 0).edit().putBoolean("hinted", true).apply()
                val how = if (trackpad) "The screen is a trackpad: tap to click, two fingers to scroll." else "Tap to click, two fingers to scroll."
                app.notice.show("$how Tap ••• for the keyboard and menu.", durationMs = 6500)
            }
        }
    }

    private val dim = Runnable { menuButton.animate().alpha(0.55f).setDuration(600).start() }

    private fun dimMenuButtonLater() {
        removeCallbacks(dim)
        menuButton.animate().alpha(1f).setDuration(150).start()
        postDelayed(dim, 3000)
    }

    // ---- The session's status, once a frame -----------------------------------------------

    override fun doFrame(frameTimeNanos: Long) {
        if (session == 0L || ended) return
        val changes = Native.changes(session)
        val now = SystemClock.uptimeMillis()
        if (changes != lastChanges || now - lastStatusAt > 1000) {
            lastChanges = changes
            lastStatusAt = now
            runCatching { JSONObject(Native.status(session)) }.getOrNull()?.let { refresh(it) }
        }
        if (!ended) Choreographer.getInstance().postFrameCallback(this)
    }

    private fun refresh(next: JSONObject) {
        status = next
        val w = next.optInt("width")
        val h = next.optInt("height")
        if (w > 0 && h > 0 && (w != viewport.streamWidth || h != viewport.streamHeight)) {
            viewport.streamWidth = w
            viewport.streamHeight = h
            surface.holder.setFixedSize(w, h)
            viewport.reset()
            placePicture()
            if (!pointer.placed) pointer.moveTo(w / 2f, h / 2f)
        }
        val cursor = next.optLong("cursor")
        if (cursor != lastCursor) {
            lastCursor = cursor
            pointer.setShape(Native.cursor(session))
        }
        when (next.optString("phase")) {
            "reaching" -> launch.say("Reaching ${machine.name}…")
            "starting" -> launch.say("Starting video…")
            "streaming" -> if (!streaming) {
                streaming = true
                app.background { app.store.markConnected(machine.id) }
                revealPicture()
            }
            "ended" -> if (!ended && !leaving) finish(next.optString("reason"))
        }
        if (stats.visibility == View.VISIBLE) stats.text = statsLine()
        menu?.update(next)
    }

    /** The session's numbers, worded as the desktop viewers word them. */
    private fun statsLine(): String = status.optString("stats").ifEmpty { "connecting…" }

    // ---- Where things go -----------------------------------------------------------------

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        viewport.reset()
        placePicture()
        placeChrome()
    }

    private fun placePicture() {
        viewport.viewWidth = width.toFloat()
        viewport.viewHeight = height.toFloat()
        val keyboardRoom = if (keyboardUp) imeHeight + keyBar.height else 0
        viewport.safe.set(cutout.left.toFloat(), cutout.top.toFloat(), (width - cutout.right).toFloat(), (height - max(cutout.bottom, keyboardRoom)).toFloat())
        viewport.update()
        if (!revealed && !(ended && streaming)) return
        applyPicture()
    }

    private fun applyPicture() {
        val picture = viewport.picture
        val params = surface.layoutParams
        val w = max(1, picture.width().roundToInt())
        val h = max(1, picture.height().roundToInt())
        if (params.width != w || params.height != h) {
            params.width = w
            params.height = h
            surface.layoutParams = params
        }
        surface.translationX = picture.left
        surface.translationY = picture.top
        pointer.invalidate()
    }

    private fun placeChrome() {
        val top = max(cutout.top, dpi(8f)) + dpi(6f)
        (menuButton.layoutParams as LayoutParams).topMargin = top
        (stats.layoutParams as LayoutParams).apply {
            topMargin = top
            leftMargin = cutout.left + dpi(10f)
        }
        // Clear of the menu button in the middle.
        stats.maxWidth = (width / 2 - dpi(26f) - dpi(12f) - cutout.left - dpi(10f)).coerceAtLeast(dpi(120f))
        stats.setLineSpacing(0f, 1.15f)
        menuButton.requestLayout()
        stats.requestLayout()
    }

    private fun placeKeyBar(ime: Int) {
        keyBar.translationY = -ime.toFloat()
    }

    // ---- The surface: the decoder draws on it -------------------------------------------

    override fun surfaceCreated(holder: SurfaceHolder) = Native.setSurface(holder.surface)

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {}

    override fun surfaceDestroyed(holder: SurfaceHolder) = Native.setSurface(null)

    // ---- Fingers ---------------------------------------------------------------------------

    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (!active || !revealed) return true
        if (event.isFromSource(InputDevice.SOURCE_MOUSE)) return mouse(event)
        dimMenuButtonLater()
        return touch.onTouchEvent(event)
    }

    override val trackpad: Boolean get() = app.prefs.touchMode != "touch"

    private fun sendPointer() {
        if (session == 0L || viewport.streamWidth == 0) return
        Native.pointer(session, pointer.streamX / viewport.streamWidth, pointer.streamY / viewport.streamHeight)
    }

    override fun moveBy(dx: Float, dy: Float, speed: Float) {
        // Slow is precise; quick crosses the screen. In view pixels, so the
        // pointer keeps pace with the finger however close the picture is.
        val dpPerMs = speed / resources.displayMetrics.density
        val t = ((dpPerMs - 0.15f) / 1.6f).coerceIn(0f, 1f)
        val gain = 1.15f + 1.7f * t * t * (3 - 2 * t)
        pointer.moveTo(pointer.streamX + dx * gain / viewport.scale, pointer.streamY + dy * gain / viewport.scale)
        sendPointer()
        if (viewport.follow(viewport.toViewX(pointer.streamX), viewport.toViewY(pointer.streamY), dp(56f))) applyPicture()
    }

    override fun moveTo(x: Float, y: Float) {
        pointer.moveTo(viewport.toStreamX(x), viewport.toStreamY(y))
        sendPointer()
    }

    override fun button(button: Int, down: Boolean) {
        if (session == 0L) return
        if (down) pointer.pulse()
        Native.button(session, button, down)
    }

    override fun click(button: Int) {
        if (session == 0L) return
        pointer.pulse()
        Native.button(session, button, true)
        Native.button(session, button, false)
    }

    override fun scroll(dx: Float, dy: Float, phase: Int) {
        if (session == 0L) return
        // In the host's units: points on a Mac (two pixels each on Retina).
        val unit = viewport.scale * if (mac) 2f else 1f
        Native.scroll(session, dx / unit, dy / unit, phase)
    }

    override fun pinch(factor: Float, fx: Float, fy: Float, dx: Float, dy: Float) {
        viewport.pinch(factor, fx, fy, dx, dy)
        applyPicture()
    }

    override fun pinchEnded() {
        if (viewport.zoom < 1.06f) {
            viewport.reset()
            applyPicture()
        }
    }

    override fun keyboard() = keyboard(!keyboardUp)

    override fun held() {
        performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
    }

    /** A mouse button pressed with the pointer's touch-down (the rest come as
     *  button presses of their own), or -1. */
    private var mouseHeld = -1

    private fun mouseButton(buttons: Int): Int = when {
        buttons and MotionEvent.BUTTON_PRIMARY != 0 -> 0
        buttons and MotionEvent.BUTTON_SECONDARY != 0 -> 1
        buttons and MotionEvent.BUTTON_TERTIARY != 0 -> 2
        buttons and MotionEvent.BUTTON_BACK != 0 -> 3
        buttons and MotionEvent.BUTTON_FORWARD != 0 -> 4
        else -> 0
    }

    /** A mouse plugged into the phone: its own pointer, at its own place.
     *  A mouse's press comes as a touch-down and then a button press; some
     *  (and adb) send only the touch-down: either way, one click. */
    private fun mouse(event: MotionEvent): Boolean {
        if (session == 0L) return true
        when (event.actionMasked) {
            MotionEvent.ACTION_HOVER_MOVE, MotionEvent.ACTION_MOVE -> moveTo(event.x, event.y)
            MotionEvent.ACTION_DOWN -> {
                moveTo(event.x, event.y)
                mouseHeld = mouseButton(event.buttonState)
                Native.button(session, mouseHeld, true)
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                moveTo(event.x, event.y)
                if (mouseHeld >= 0) Native.button(session, mouseHeld, false)
                mouseHeld = -1
            }
            MotionEvent.ACTION_BUTTON_PRESS, MotionEvent.ACTION_BUTTON_RELEASE -> {
                val button = mouseButton(event.actionButton)
                if (button != mouseHeld) Native.button(session, button, event.actionMasked == MotionEvent.ACTION_BUTTON_PRESS)
            }
            MotionEvent.ACTION_SCROLL -> {
                // The wheel scrolls what's under the mouse.
                moveTo(event.x, event.y)
                Native.scroll(session, -event.getAxisValue(MotionEvent.AXIS_HSCROLL) * 40f, event.getAxisValue(MotionEvent.AXIS_VSCROLL) * 40f, -1)
            }
        }
        return true
    }

    override fun onGenericMotionEvent(event: MotionEvent): Boolean {
        if (active && revealed && event.isFromSource(InputDevice.SOURCE_MOUSE)) return mouse(event)
        return super.onGenericMotionEvent(event)
    }

    // ---- Keys --------------------------------------------------------------------------------

    fun keyboard(show: Boolean, fromSystem: Boolean = false) {
        val imm = activity.getSystemService(InputMethodManager::class.java)
        keyboardUp = show
        if (show) {
            keyInput.requestFocus()
            // Once the keyboard is connected to the newly focused view.
            post {
                if (!keyboardUp) return@post
                if (Build.VERSION.SDK_INT >= 30) windowInsetsController?.show(WindowInsets.Type.ime())
                imm?.showSoftInput(keyInput, 0)
            }
            keyBar.visibility = View.VISIBLE
            keyBar.alpha = 0f
            keyBar.animate().alpha(1f).setDuration(200).start()
        } else {
            if (!fromSystem) {
                if (Build.VERSION.SDK_INT >= 30) windowInsetsController?.hide(WindowInsets.Type.ime())
                else imm?.hideSoftInputFromWindow(windowToken, 0)
            }
            keyBar.visibility = View.GONE
        }
        post { placePicture() }
    }

    override fun typed(text: String) {
        if (session == 0L) return
        keyBar.withModifiers {
            val skipped = Native.type(session, text)
            if (skipped > 0 && !skippedNoted) {
                skippedNoted = true
                app.notice.show("Some characters can't be typed: Sunna types on a US keyboard layout.")
            }
        }
    }

    override fun key(code: Int) {
        if (session == 0L) return
        keyBar.withModifiers {
            Native.macKey(session, code, true)
            Native.macKey(session, code, false)
        }
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        // Keys from a keyboard (or anything else that sends them, adb and
        // scrcpy included); the on-screen keyboard's arrive through KeyInput.
        val system = event.keyCode in setOf(KeyEvent.KEYCODE_BACK, KeyEvent.KEYCODE_HOME, KeyEvent.KEYCODE_VOLUME_UP, KeyEvent.KEYCODE_VOLUME_DOWN,
            KeyEvent.KEYCODE_VOLUME_MUTE, KeyEvent.KEYCODE_POWER, KeyEvent.KEYCODE_APP_SWITCH)
        val action = event.action == KeyEvent.ACTION_DOWN || event.action == KeyEvent.ACTION_UP
        if (active && revealed && !system && action) {
            if (Native.key(session, event.keyCode, event.action == KeyEvent.ACTION_DOWN, event.repeatCount > 0)) return true
        }
        return super.dispatchKeyEvent(event)
    }

    // ---- The menu ----------------------------------------------------------------------------

    fun openMenu() {
        if (menu?.isOpen == true || !revealed || ended) return
        if (keyboardUp) keyboard(false)
        val host = parent as? FrameLayout ?: return
        menu = SessionMenu(activity, host, app, this, machine, check, mac).also {
            it.onClosed = { menu = null }
            it.update(status)
            it.open()
        }
    }

    /** Back: the menu (where Disconnect is), or closes it. */
    fun back(): Boolean {
        if (menu?.isOpen == true) {
            menu?.close()
            return true
        }
        if (keyboardUp) {
            keyboard(false)
            return true
        }
        if (!revealed) {
            leave()
            return true
        }
        openMenu()
        return true
    }

    fun showStats(show: Boolean) {
        app.prefs.stats = show
        stats.visibility = if (show) View.VISIBLE else View.GONE
        stats.text = statsLine()
    }

    fun setTouchMode(mode: String) {
        app.prefs.touchMode = mode
    }

    fun shortcut(index: Int) {
        if (session != 0L) Native.shortcut(session, index)
    }

    fun applyStream() {
        if (session == 0L) return
        codec = app.prefs.codec.ifEmpty { if (Decoders.hardware("video/hevc")) "hevc" else "h264" }
        val (w, h) = requestedSize()
        Native.setStream(session, codec, w, h, app.prefs.bitrateMbps, app.prefs.fps, app.prefs.sound)
    }

    fun pause() {}

    /** The app is going away: leave at once, no animation. */
    fun destroy() {
        ended = true
        if (session != 0L) Native.close(session)
        session = 0L
    }

    fun resume() {
        if (active) immersive(true)
    }

    private fun immersive(on: Boolean) {
        val window = activity.window
        if (Build.VERSION.SDK_INT >= 30) {
            val controller = window.insetsController ?: return
            if (on) {
                controller.systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                controller.hide(WindowInsets.Type.systemBars())
            } else {
                controller.show(WindowInsets.Type.systemBars())
            }
        } else {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_LAYOUT_STABLE or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
                View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or if (on) {
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or View.SYSTEM_UI_FLAG_FULLSCREEN or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                } else 0
        }
        if (on) window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) else window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    }

}

fun codecName(codec: String): String = when (codec) {
    "hevc" -> "HEVC"
    "h264" -> "H.264"
    "" -> "—"
    else -> codec.uppercase()
}

/** Where a tile's screen is drawn within it, and the tile's scale (it may be
 *  shrunk under a finger). */
fun ScreenView.screenRect(): Pair<RectF, Float> {
    val w = width.toFloat()
    val h = height.toFloat()
    var sw = w
    var sh = w * 11f / 16f
    if (sh > h) {
        sh = h
        sw = h * 16f / 11f
    }
    val left = (w - sw) / 2
    val area = sw * 10f / 16f
    val (screenW, screenH) = if (ratio >= 1.6f) sw to sw / ratio else area * ratio to area
    val rect = RectF(left + sw / 2 - screenW / 2, area - screenH, left + sw / 2 + screenW / 2, area)
    var scale = 1f
    var view: View? = this
    while (view != null) {
        scale *= view.scaleX
        view = view.parent as? View
    }
    return rect to scale
}
