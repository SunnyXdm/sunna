package dev.sunna.app

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.os.Build
import android.view.Gravity
import android.view.MotionEvent
import android.view.VelocityTracker
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.WindowInsetsAnimation
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.PopupWindow
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

/** Window insets as the screens use them: the system bars (and cutout), and
 *  the on-screen keyboard. */
data class Insets(val left: Int = 0, val top: Int = 0, val right: Int = 0, val bottom: Int = 0, val ime: Int = 0) {
    companion object {
        fun of(insets: WindowInsets): Insets = if (Build.VERSION.SDK_INT >= 30) {
            val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
            Insets(bars.left, bars.top, bars.right, bars.bottom, insets.getInsets(WindowInsets.Type.ime()).bottom)
        } else {
            @Suppress("DEPRECATION")
            val stable = insets.stableInsetBottom
            @Suppress("DEPRECATION")
            Insets(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, stable, max(0, insets.systemWindowInsetBottom - stable))
        }
    }
}

/**
 * A card that rises from the bottom over a dimmed screen (a narrower card on
 * wide screens), kept above the keyboard as it slides. Drag it down, tap
 * outside or go back to close it.
 */
@SuppressLint("ViewConstructor", "ClickableViewAccessibility")
open class Sheet(context: Context, private val host: FrameLayout) : FrameLayout(context) {
    private val scrim = View(context)
    protected val card = LinearLayout(context)
    private val grabber = FrameLayout(context)
    private var insets = Insets()
    private var imeNow = 0
    private var closing = false
    private val slop = android.view.ViewConfiguration.get(context).scaledTouchSlop
    private var downX = 0f
    private var downY = 0f
    private var dragging = false
    private var tracker: VelocityTracker? = null
    /** Told as it starts closing, and when it's gone. */
    var onClosing: (() -> Unit)? = null
    var onClosed: (() -> Unit)? = null
    /** How much of the screen to keep clear above a tall card, as a
     *  fraction of its height: room to tap outside it. */
    protected open val clearAbove = 0f
    val isOpen: Boolean get() = parent != null && !closing

    companion object {
        /** Told when any sheet opens or closes (Back follows what's open). */
        var changed: (() -> Unit)? = null
    }

    init {
        scrim.setBackgroundColor(0x8C000000.toInt())
        scrim.alpha = 0f
        scrim.setOnClickListener { close() }
        addView(scrim, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        card.orientation = LinearLayout.VERTICAL
        card.isClickable = true
        card.elevation = dp(16f)
        grabber.addView(View(context).apply { background = rounded(white(.22f), dp(3f)) }, LayoutParams(dpi(38f), dpi(5f), Gravity.CENTER))
        card.addView(grabber, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, dpi(22f)))
        addView(card, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT, Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL))
        if (Build.VERSION.SDK_INT >= 30) {
            setWindowInsetsAnimationCallback(object : WindowInsetsAnimation.Callback(DISPATCH_MODE_STOP) {
                override fun onProgress(insets: WindowInsets, running: MutableList<WindowInsetsAnimation>): WindowInsets {
                    imeNow = insets.getInsets(WindowInsets.Type.ime()).bottom
                    place()
                    return insets
                }

                // Where the keyboard ended up, whatever the last frame said.
                override fun onEnd(animation: WindowInsetsAnimation) {
                    rootWindowInsets?.let {
                        imeNow = it.getInsets(WindowInsets.Type.ime()).bottom
                        place()
                    }
                }
            })
        }
    }

    fun setInsets(insets: Insets) {
        this.insets = insets
        imeNow = insets.ime
        place()
    }

    private fun place() {
        val wide = resources.configuration.screenWidthDp >= 600
        val params = card.layoutParams as LayoutParams
        params.width = if (wide) dpi(560f) else LayoutParams.MATCH_PARENT
        params.bottomMargin = if (wide) dpi(16f) + max(insets.bottom, imeNow) else imeNow
        val radius = dp(26f)
        card.background = if (wide) rounded(Palette.SHEET, radius) else android.graphics.drawable.GradientDrawable().apply {
            setColor(Palette.SHEET)
            cornerRadii = floatArrayOf(radius, radius, radius, radius, 0f, 0f, 0f, 0f)
        }
        // The bars and the keyboard: the content stays clear of both.
        val bottom = if (wide) dpi(20f) else (if (imeNow > 0) dpi(16f) else insets.bottom + dpi(16f))
        // A centered card is clear of the cutouts and bars at the sides.
        val side = if (wide) 0 else 1
        card.setPadding(insets.left * side + dpi(4f), 0, insets.right * side + dpi(4f), bottom)
        card.layoutParams = params
    }

    /** Room for the card: below the status bar, above the keyboard. */
    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        super.onMeasure(widthMeasureSpec, heightMeasureSpec)
        val height = MeasureSpec.getSize(heightMeasureSpec)
        val above = max(insets.top + dpi(12f), (height * clearAbove).toInt())
        val room = height - above - (card.layoutParams as LayoutParams).bottomMargin
        if (room > 0 && card.measuredHeight > room) {
            card.measure(MeasureSpec.makeMeasureSpec(card.measuredWidth, MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(room, MeasureSpec.EXACTLY))
        }
    }

    /** Put `content` in the card. */
    fun setContent(content: View) {
        card.addView(content, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT))
    }

    fun open() {
        host.addView(this, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        host.rootWindowInsets?.let { setInsets(Insets.of(it)) }
        changed?.invoke()
        card.visibility = View.INVISIBLE
        post {
            card.translationY = card.height.toFloat() + dp(40f)
            card.visibility = View.VISIBLE
            card.animate().translationY(0f).setDuration(Motion.ZOOM).setInterpolator(Motion.zoom).start()
            scrim.animate().alpha(1f).setDuration(260).start()
        }
    }

    open fun close() {
        if (closing || parent == null) return
        closing = true
        onClosing?.invoke()
        hideKeyboard()
        card.animate().translationY(card.height.toFloat() + dp(40f)).setDuration(260).setInterpolator(Motion.easeIn).start()
        scrim.animate().alpha(0f).setDuration(260).withEndAction {
            host.removeView(this)
            onClosed?.invoke()
            changed?.invoke()
        }.start()
    }

    protected fun hideKeyboard() {
        val imm = context.getSystemService(android.view.inputmethod.InputMethodManager::class.java)
        imm?.hideSoftInputFromWindow(windowToken, 0)
    }

    /** A downward drag anywhere on the card closes it, unless what's under
     *  the finger can still scroll up: then that scrolls. */
    override fun onInterceptTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                dragging = false
                downX = event.x
                downY = event.y
                tracker?.recycle()
                tracker = if (!closing && inCard(event.x, event.y)) VelocityTracker.obtain().also { it.addMovement(event) } else null
            }
            MotionEvent.ACTION_MOVE -> {
                val tracker = tracker ?: return false
                tracker.addMovement(event)
                val dy = event.y - downY
                if (dy > slop && dy > abs(event.x - downX) &&
                    !scrollsUp(card, downX - card.left - card.translationX, downY - card.top - card.translationY)) {
                    dragging = true
                    downY = event.y
                    card.animate().cancel()
                    scrim.animate().cancel()
                    return true
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                tracker?.recycle()
                tracker = null
            }
        }
        return false
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (!dragging) return super.onTouchEvent(event)
        tracker?.addMovement(event)
        when (event.actionMasked) {
            MotionEvent.ACTION_MOVE -> follow(event)
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                dragging = false
                // Where the finger let go: a quick flick may not have moved
                // it there yet.
                if (event.actionMasked == MotionEvent.ACTION_UP) follow(event)
                val velocity = tracker?.let {
                    it.computeCurrentVelocity(1000)
                    it.yVelocity
                } ?: 0f
                tracker?.recycle()
                tracker = null
                val far = card.translationY > min(card.height * 0.28f, dp(160f))
                if (event.actionMasked == MotionEvent.ACTION_UP && (far || velocity > dp(900f))) close()
                else {
                    card.animate().translationY(0f).setDuration(Motion.BOUNCY).setInterpolator(Motion.bouncy).start()
                    scrim.animate().alpha(1f).setDuration(200).start()
                }
            }
        }
        return true
    }

    private fun follow(event: MotionEvent) {
        val dy = event.y - downY
        card.translationY = if (dy > 0) dy else dy / 4
        scrim.alpha = 1f - (max(0f, dy) / max(1, card.height)).coerceIn(0f, 1f) * 0.8f
    }

    private fun inCard(x: Float, y: Float): Boolean =
        x >= card.left + card.translationX && x < card.right + card.translationX &&
            y >= card.top + card.translationY && y < card.bottom + card.translationY

    /** Whether the view at (x, y), or one inside it there, can scroll up. */
    private fun scrollsUp(view: View, x: Float, y: Float): Boolean {
        if (view.canScrollVertically(-1)) return true
        if (view !is ViewGroup) return false
        for (i in view.childCount - 1 downTo 0) {
            val child = view.getChildAt(i)
            if (child.visibility != View.VISIBLE) continue
            val cx = x + view.scrollX - child.left - child.translationX
            val cy = y + view.scrollY - child.top - child.translationY
            if (cx >= 0 && cy >= 0 && cx < child.width && cy < child.height && scrollsUp(child, cx, cy)) return true
        }
        return false
    }
}

/** A heading for a sheet: a title, and an × to close. */
fun Sheet.header(title: String): LinearLayout = LinearLayout(context).apply {
    orientation = LinearLayout.HORIZONTAL
    gravity = Gravity.CENTER_VERTICAL
    setPadding(dpi(20f), 0, dpi(8f), dpi(6f))
    addView(context.text(20f, Palette.TEXT, Type.bold).apply {
        text = title
        maxLines = 2
    }, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
    addView(FrameLayout(context).apply {
        background = ripple(rounded(white(.08f), dp(16f)), dp(16f))
        addView(context.icon(R.drawable.ic_close, Palette.TEXT_2, 16f), FrameLayout.LayoutParams(dpi(16f), dpi(16f), Gravity.CENTER))
        contentDescription = "Close"
        setOnClickListener { close() }
    }, LinearLayout.LayoutParams(dpi(32f), dpi(32f)).apply { marginEnd = dpi(8f) })
}

/** Ask before something that can't be undone. */
fun confirm(context: Context, host: FrameLayout, title: String, message: String, action: String, onConfirm: () -> Unit): Sheet {
    val sheet = Sheet(context, host)
    val body = LinearLayout(context).apply {
        orientation = LinearLayout.VERTICAL
        addView(sheet.header(title))
        addView(context.text(15f, Palette.TEXT_2).apply {
            text = message
            setLineSpacing(0f, 1.25f)
            setPadding(context.dpi(20f), 0, context.dpi(20f), context.dpi(20f))
        })
        val buttons = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            setPadding(context.dpi(16f), 0, context.dpi(16f), 0)
            addView(context.button("Cancel", ButtonStyle.SECONDARY) { sheet.close() }, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f).apply { marginEnd = context.dpi(10f) })
            addView(context.button(action, ButtonStyle.DESTRUCTIVE) {
                sheet.close()
                onConfirm()
            }, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        }
        addView(buttons)
    }
    sheet.setContent(body)
    sheet.open()
    return sheet
}

/** A small menu next to what opened it, like the desktop app's. */
class MenuPopup(private val context: Context) {
    class Item(val title: String, val icon: Int, val danger: Boolean = false, val run: () -> Unit)

    fun show(anchor: View, items: List<Item>) {
        val list = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(context.dpi(6f), context.dpi(6f), context.dpi(6f), context.dpi(6f))
            background = rounded(0xF2222329.toInt(), context.dp(16f), white(.1f), context.dpi(1f))
            elevation = context.dp(12f)
        }
        val popup = PopupWindow(list, context.dpi(230f), ViewGroup.LayoutParams.WRAP_CONTENT, true)
        for (item in items) {
            val color = if (item.danger) 0xFFFF7B72.toInt() else Palette.TEXT
            list.addView(LinearLayout(context).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                minimumHeight = context.dpi(46f)
                setPadding(context.dpi(12f), 0, context.dpi(12f), 0)
                background = ripple(null, context.dp(10f))
                addView(context.icon(item.icon, color, 18f), LinearLayout.LayoutParams(context.dpi(18f), context.dpi(18f)).apply { marginEnd = context.dpi(12f) })
                addView(context.text(15f, color).apply { text = item.title })
                setOnClickListener {
                    popup.dismiss()
                    item.run()
                }
            })
        }
        popup.setBackgroundDrawable(ColorDrawable(Color.TRANSPARENT))
        popup.elevation = context.dp(12f)
        popup.isOutsideTouchable = true
        list.alpha = 0f
        list.scaleX = 0.9f
        list.scaleY = 0.9f
        list.pivotX = context.dp(230f)
        list.pivotY = 0f
        popup.showAsDropDown(anchor, 0, context.dpi(6f), Gravity.END)
        list.animate().alpha(1f).scaleX(1f).scaleY(1f).setDuration(Motion.BOUNCY).setInterpolator(Motion.bouncy).start()
    }
}

