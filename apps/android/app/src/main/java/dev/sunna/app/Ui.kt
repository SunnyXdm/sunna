package dev.sunna.app

import android.animation.TimeInterpolator
import android.annotation.SuppressLint
import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Outline
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.Typeface
import android.graphics.drawable.Drawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.os.Build
import android.text.InputType
import android.util.TypedValue
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.view.ViewOutlineProvider
import android.view.animation.PathInterpolator
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlin.math.sqrt

/** The quiet chrome around the computers' screens. */
object Palette {
    const val BG = 0xFF0D0E11.toInt()
    const val TEXT = 0xFFF2F2F4.toInt()
    const val TEXT_2 = 0xFFA2A4AC.toInt()
    const val TEXT_3 = 0xFF6D7079.toInt()
    const val LINE = 0x14FFFFFF
    const val LINE_2 = 0x21FFFFFF
    const val READY = 0xFF30D158.toInt()
    const val BUSY = 0xFFFF9F0A.toInt()
    const val DANGER = 0xFFFF453A.toInt()
    const val SHEET = 0xFF1A1B20.toInt()
    const val RAISED = 0xFF24252B.toInt()

    /** The phone's own accent color (Android 12 and later), else blue. */
    var accent = 0xFF0A84FF.toInt()
        private set

    fun load(context: Context) {
        if (Build.VERSION.SDK_INT >= 31) accent = context.getColor(android.R.color.system_accent1_200)
    }
}

object Type {
    val regular: Typeface = Typeface.create(Typeface.SANS_SERIF, 400, false)
    val medium: Typeface = Typeface.create(Typeface.SANS_SERIF, 500, false)
    val semibold: Typeface = Typeface.create(Typeface.SANS_SERIF, 600, false)
    val bold: Typeface = Typeface.create(Typeface.SANS_SERIF, 700, false)
    val mono: Typeface = Typeface.MONOSPACE
}

fun Context.dp(value: Float): Float = value * resources.displayMetrics.density
fun Context.dpi(value: Float): Int = dp(value).roundToInt()
fun View.dp(value: Float): Float = context.dp(value)
fun View.dpi(value: Float): Int = context.dpi(value)

fun white(alpha: Float): Int = ((alpha * 255).roundToInt().coerceIn(0, 255) shl 24) or 0xFFFFFF
fun withAlpha(color: Int, alpha: Float): Int = ((alpha * 255).roundToInt().coerceIn(0, 255) shl 24) or (color and 0xFFFFFF)

/** A spring's step response as an interpolator: `bounce` 0 settles without
 *  overshoot; 0.28 overshoots by about 4%. The animation's duration is the
 *  spring's settling time. */
class Spring(private val bounce: Float = 0f) : TimeInterpolator {
    override fun getInterpolation(input: Float): Float {
        if (input >= 1f) return 1f
        val t = input.toDouble()
        val zeta = (1.0 - bounce).coerceIn(0.1, 1.0)
        if (zeta > 0.999) {
            val w = 9.2
            return (1 - (1 + w * t) * exp(-w * t)).toFloat()
        }
        val w = 6.9 / zeta
        val root = sqrt(1 - zeta * zeta)
        val wd = w * root
        return (1 - exp(-zeta * w * t) * (cos(wd * t) + zeta / root * sin(wd * t))).toFloat()
    }
}

/** The same motion as the desktop app's springs (ui/motion.js). */
object Motion {
    val snappy = Spring(0f)
    const val SNAPPY = 360L
    val bouncy = Spring(0.28f)
    const val BOUNCY = 560L
    val gentle = Spring(0f)
    const val GENTLE = 640L
    val zoom = Spring(0.06f)
    const val ZOOM = 640L
    val outExpo = PathInterpolator(0.16f, 1f, 0.3f, 1f)
    val easeIn = PathInterpolator(0.5f, 0f, 0.75f, 0f)
    val easeInOut = PathInterpolator(0.45f, 0f, 0.55f, 1f)
}

fun Context.text(size: Float, color: Int, face: Typeface = Type.regular): TextView = TextView(this).apply {
    setTextSize(TypedValue.COMPLEX_UNIT_SP, size)
    setTextColor(color)
    typeface = face
    includeFontPadding = false
}

fun Context.icon(resource: Int, color: Int, size: Float = 20f): ImageView = ImageView(this).apply {
    setImageResource(resource)
    imageTintList = ColorStateList.valueOf(color)
    layoutParams = ViewGroup.LayoutParams(dpi(size), dpi(size))
}

fun rounded(color: Int, radius: Float, stroke: Int = 0, strokeWidth: Int = 0): GradientDrawable = GradientDrawable().apply {
    setColor(color)
    cornerRadius = radius
    if (strokeWidth > 0) setStroke(strokeWidth, stroke)
}

/** A touch ripple over `content`, clipped to its rounded shape. */
fun ripple(content: Drawable?, radius: Float, color: Int = white(.12f)): Drawable =
    RippleDrawable(ColorStateList.valueOf(color), content, rounded(Color.WHITE, radius))

/** Shrinks a little under the finger and springs back, like the desktop
 *  app's buttons. */
@SuppressLint("ClickableViewAccessibility")
fun View.pressable(scale: Float = 0.96f) {
    setOnTouchListener { view, event ->
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> view.animate().scaleX(scale).scaleY(scale).setDuration(120).setInterpolator(Motion.snappy).start()
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL ->
                view.animate().scaleX(1f).scaleY(1f).setDuration(Motion.BOUNCY).setInterpolator(Motion.bouncy).start()
        }
        false
    }
}

enum class ButtonStyle { PRIMARY, SECONDARY, GHOST, DESTRUCTIVE }

fun Context.button(label: String, style: ButtonStyle, iconRes: Int = 0, onClick: () -> Unit): LinearLayout {
    val radius = dp(12f)
    val (background, foreground) = when (style) {
        ButtonStyle.PRIMARY -> Palette.accent to Palette.BG
        ButtonStyle.SECONDARY -> white(.1f) to Palette.TEXT
        ButtonStyle.GHOST -> Color.TRANSPARENT to Palette.TEXT_2
        ButtonStyle.DESTRUCTIVE -> Palette.DANGER to Color.WHITE
    }
    return LinearLayout(this).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER
        minimumHeight = dpi(46f)
        setPadding(dpi(18f), 0, dpi(18f), 0)
        this.background = ripple(rounded(background, radius), radius, if (style == ButtonStyle.PRIMARY) white(.25f) else white(.12f))
        if (iconRes != 0) {
            addView(icon(iconRes, foreground, 18f), LinearLayout.LayoutParams(dpi(18f), dpi(18f)).apply { marginEnd = dpi(8f) })
        }
        addView(text(15f, foreground, Type.semibold).apply { this.text = label })
        isClickable = true
        isFocusable = true
        contentDescription = label
        setOnClickListener { onClick() }
        pressable()
    }
}

/** A text field in the desktop app's style: dark well, hairline edge, the
 *  accent when focused, red when wrong. */
class Field(context: Context, hint: String, mono: Boolean = false, secret: Boolean = false) : LinearLayout(context) {
    val input = EditText(context)
    private val well = rounded(0x47000000, dp(12f), white(.1f), dpi(1f))
    var error = false
        set(value) {
            field = value
            paintEdge()
        }

    init {
        orientation = HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        minimumHeight = dpi(50f)
        setPadding(dpi(14f), 0, dpi(6f), 0)
        background = well
        input.apply {
            background = null
            setPadding(0, dpi(12f), 0, dpi(12f))
            setTextColor(Palette.TEXT)
            setHintTextColor(Palette.TEXT_3)
            this.hint = hint
            setTextSize(TypedValue.COMPLEX_UNIT_SP, if (mono) 15f else 16f)
            typeface = if (mono || secret) Type.mono else Type.regular
            isSingleLine = true
            imeOptions = android.view.inputmethod.EditorInfo.IME_FLAG_NO_EXTRACT_UI or android.view.inputmethod.EditorInfo.IME_ACTION_NEXT
            inputType = if (secret) {
                InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
            } else {
                InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
            }
            if (Build.VERSION.SDK_INT >= 29) {
                textCursorDrawable = rounded(Palette.accent, 0f).apply { setSize(dpi(2f), 0) }
            }
            setOnFocusChangeListener { _, _ -> paintEdge() }
            // Keys read best in monospace; the hint doesn't.
            if (mono || secret) {
                typeface = Type.regular
                addTextChangedListener(object : android.text.TextWatcher {
                    override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
                    override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
                    override fun afterTextChanged(s: android.text.Editable?) {
                        val face = if (s.isNullOrEmpty()) Type.regular else Type.mono
                        if (typeface != face) typeface = face
                    }
                })
            }
        }
        addView(input, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
    }

    private fun paintEdge() {
        val color = when {
            error -> Palette.DANGER
            input.hasFocus() -> Palette.accent
            else -> white(.1f)
        }
        well.setStroke(dpi(if (error || input.hasFocus()) 1.5f else 1f), color)
    }

    /** A shake, as the desktop's fields do when something's wrong. */
    fun shake() {
        animate().cancel()
        translationX = 0f
        val offsets = floatArrayOf(-6f, 5f, -3f, 2f, 0f)
        var step = 0
        fun next() {
            if (step >= offsets.size) return
            animate().translationX(dp(offsets[step++])).setDuration(70).withEndAction { next() }.start()
        }
        next()
    }
}

/** A short message at the bottom of the screen, with an optional action.
 *  Android's own toasts can't carry the action or match the look. */
class Notice(context: Context) : FrameLayout(context) {
    private val pill = LinearLayout(context)
    private val glyph = context.icon(R.drawable.ic_alert, Palette.TEXT_2, 18f)
    private val message = context.text(14f, Palette.TEXT)
    private val action = context.text(14f, Palette.accent, Type.semibold)
    private val hide = Runnable { dismiss() }
    var bottomInset = 0

    init {
        isClickable = false
        pill.apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dpi(16f), dpi(12f), dpi(16f), dpi(12f))
            background = rounded(Palette.RAISED, dp(16f), white(.08f), dpi(1f))
            elevation = dp(8f)
            addView(glyph, LinearLayout.LayoutParams(dpi(18f), dpi(18f)).apply { marginEnd = dpi(10f) })
            addView(message, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
            addView(action, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { marginStart = dpi(14f) })
            visibility = View.GONE
        }
        message.setLineSpacing(0f, 1.15f)
        addView(pill, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT, Gravity.BOTTOM).apply {
            leftMargin = dpi(14f)
            rightMargin = dpi(14f)
        })
    }

    fun show(text: String, actionLabel: String? = null, onAction: (() -> Unit)? = null, durationMs: Long = 4500) {
        removeCallbacks(hide)
        message.text = text
        action.visibility = if (actionLabel != null) View.VISIBLE else View.GONE
        action.text = actionLabel ?: ""
        action.setOnClickListener {
            dismiss()
            onAction?.invoke()
        }
        (pill.layoutParams as LayoutParams).bottomMargin = bottomInset + dpi(16f)
        pill.requestLayout()
        pill.visibility = View.VISIBLE
        pill.alpha = 0f
        pill.translationY = dp(24f)
        pill.animate().alpha(1f).translationY(0f).setDuration(Motion.BOUNCY).setInterpolator(Motion.bouncy).start()
        postDelayed(hide, durationMs)
    }

    fun dismiss() {
        removeCallbacks(hide)
        if (pill.visibility != View.VISIBLE) return
        pill.animate().alpha(0f).translationY(dp(16f)).setDuration(220).setInterpolator(Motion.easeIn)
            .withEndAction { pill.visibility = View.GONE }.start()
    }
}

/** Clips a view to a rounded rectangle (on the GPU, no extra layer). */
fun View.clipRounded(radius: Float) {
    outlineProvider = object : ViewOutlineProvider() {
        override fun getOutline(view: View, outline: Outline) {
            outline.setRoundRect(0, 0, view.width, view.height, radius)
        }
    }
    clipToOutline = true
}

/** A 7dp status dot. */
class Dot(context: Context) : View(context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    var color: Int = Palette.TEXT_3
        set(value) {
            field = value
            paint.color = value
            invalidate()
        }
    var pulsing = false
        set(value) {
            if (field == value) return
            field = value
            animate().cancel()
            alpha = 1f
            if (value) pulse()
        }

    private fun pulse() {
        if (!pulsing) return
        animate().alpha(if (alpha > 0.7f) 0.35f else 1f).setDuration(600).setInterpolator(Motion.easeInOut).withEndAction { pulse() }.start()
    }

    init {
        paint.color = color
    }

    override fun onDraw(canvas: Canvas) {
        canvas.drawCircle(width / 2f, height / 2f, width / 2f, paint)
    }
}

/** A small rounded plate with an OS's mark, like an app icon. */
class Badge(context: Context) : View(context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val edge = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        color = white(.18f)
    }
    private var mark: Drawable? = null
    private val box = RectF()
    var os: Os = Os.UNKNOWN
        set(value) {
            field = value
            mark = context.getDrawable(value.mark)?.mutate()?.apply {
                if (value != Os.LINUX && value.mark != R.drawable.mark_tux) setTint(Color.WHITE)
            }
            invalidate()
        }

    override fun onDraw(canvas: Canvas) {
        val (top, bottom) = Looks.plate(os).let { it[0] to it[1] }
        val radius = width * 0.28f
        box.set(0f, 0f, width.toFloat(), height.toFloat())
        paint.shader = android.graphics.LinearGradient(0f, 0f, 0f, height.toFloat(), top, bottom, android.graphics.Shader.TileMode.CLAMP)
        canvas.drawRoundRect(box, radius, radius, paint)
        edge.strokeWidth = dp(0.5f)
        canvas.drawRoundRect(box, radius, radius, edge)
        val tux = os.mark == R.drawable.mark_tux
        val inset = if (tux) width * 0.06f else width * 0.17f
        mark?.setBounds(inset.toInt(), (inset + if (tux) width * 0.05f else 0f).toInt(), (width - inset).toInt(), (height - inset + if (tux) width * 0.05f else 0f).toInt())
        mark?.draw(canvas)
    }
}
