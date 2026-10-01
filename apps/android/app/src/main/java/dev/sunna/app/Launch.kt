package dev.sunna.app

import android.animation.ValueAnimator
import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.os.SystemClock
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import kotlin.math.roundToInt

/** A thin bar with a light moving along it: working on it. */
class Shimmer(context: Context) : View(context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val box = RectF()

    override fun onDraw(canvas: Canvas) {
        val w = width.toFloat()
        val h = height.toFloat()
        box.set(0f, 0f, w, h)
        paint.color = white(.2f)
        canvas.drawRoundRect(box, h / 2, h / 2, paint)
        val t = (SystemClock.uptimeMillis() % 1400L) / 1400f
        val eased = Motion.easeInOut.getInterpolation(if (t < 0.5f) t * 2 else 2 - t * 2)
        val segment = w * 0.36f
        box.set((w - segment) * eased, 0f, (w - segment) * eased + segment, h)
        paint.color = Color.WHITE
        canvas.drawRoundRect(box, h / 2, h / 2, paint)
        if (isShown) postInvalidateOnAnimation()
    }
}

/**
 * Connecting: the computer's screen grows out of its tile to fill the
 * display, with its name and how far along it is; when the picture comes,
 * this fades away to show it. Leaving plays it backwards, into the tile.
 */
@SuppressLint("ViewConstructor")
class LaunchOverlay(context: Context, from: ScreenView, name: String, onCancel: () -> Unit) : FrameLayout(context) {
    private val screen = ScreenView(context)
    private val shade = View(context)
    private val content = LinearLayout(context)
    private val phase: TextView = context.text(15f, white(.72f))
    private val cancel: TextView = context.text(15f, white(.8f), Type.medium)
    private val tile = RectF()
    private val full = RectF()
    private var progress = 0f
    private var animator: ValueAnimator? = null
    var expanded = false
        private set

    init {
        screen.bare = true
        screen.os = from.os
        screen.device = from.device
        screen.show("ready", 0, animate = false)
        addView(screen)
        shade.setBackgroundColor(0x59000000)
        shade.alpha = 0f
        addView(shade, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        content.apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            alpha = 0f
            addView(ImageView(context).apply {
                setImageResource(from.os.mark)
                if (from.os.mark != R.drawable.mark_tux) imageTintList = android.content.res.ColorStateList.valueOf(Color.WHITE)
            }, LinearLayout.LayoutParams(dpi(44f), dpi(44f)))
            addView(context.text(22f, Color.WHITE, Type.semibold).apply {
                text = name
                gravity = Gravity.CENTER
                setPadding(dpi(24f), dpi(16f), dpi(24f), 0)
                setShadowLayer(dp(12f), 0f, 0f, 0x66000000)
            })
            addView(Shimmer(context), LinearLayout.LayoutParams(dpi(150f), dpi(3f)).apply { topMargin = dpi(22f) })
            addView(phase.apply {
                gravity = Gravity.CENTER
                setPadding(dpi(24f), dpi(14f), dpi(24f), 0)
                setShadowLayer(dp(10f), 0f, 0f, 0x66000000)
            })
            addView(cancel.apply {
                text = "Cancel"
                gravity = Gravity.CENTER
                setPadding(dpi(22f), dpi(10f), dpi(22f), dpi(10f))
                background = ripple(rounded(white(.12f), dp(20f)), dp(20f))
                setOnClickListener { onCancel() }
            }, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = dpi(26f) })
        }
        addView(content, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT, Gravity.CENTER))
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        super.onLayout(changed, left, top, right, bottom)
        full.set(0f, 0f, (right - left).toFloat(), (bottom - top).toFloat())
        placeScreen()
    }

    private fun placeScreen() {
        val p = progress
        val l = tile.left + (full.left - tile.left) * p
        val t = tile.top + (full.top - tile.top) * p
        val r = tile.right + (full.right - tile.right) * p
        val b = tile.bottom + (full.bottom - tile.bottom) * p
        screen.cornerRadius = dp(11f) * (1 - p).coerceIn(0f, 1f)
        screen.bezel = dp(4f) * (1 - p).coerceIn(0f, 1f)
        screen.layout(l.roundToInt(), t.roundToInt(), r.roundToInt(), b.roundToInt())
        screen.invalidate()
    }

    private fun animateTo(target: Float, duration: Long, then: () -> Unit) {
        animator?.cancel()
        animator = ValueAnimator.ofFloat(progress, target).apply {
            this.duration = duration
            interpolator = Motion.zoom
            addUpdateListener {
                progress = it.animatedValue as Float
                placeScreen()
            }
            addListener(object : android.animation.AnimatorListenerAdapter() {
                override fun onAnimationEnd(animation: android.animation.Animator) = then()
            })
            start()
        }
    }

    /** Grow out of the tile's screen (`from`, in this view's coordinates). */
    fun open(from: RectF, then: () -> Unit) {
        tile.set(from)
        progress = 0f
        content.alpha = 0f
        animateTo(1f, Motion.ZOOM) {
            expanded = true
            then()
        }
        fadeDesk(0f, 160)
        shade.animate().alpha(1f).setStartDelay(160).setDuration(400).start()
        content.animate().alpha(1f).setStartDelay(220).setDuration(380).start()
    }

    private var deskAnimator: ValueAnimator? = null

    private fun fadeDesk(to: Float, delay: Long) {
        deskAnimator?.cancel()
        deskAnimator = ValueAnimator.ofFloat(screen.deskFade, to).apply {
            startDelay = delay
            duration = 400
            addUpdateListener { screen.deskFade = it.animatedValue as Float }
            start()
        }
    }

    fun say(text: String, canCancel: Boolean = true) {
        if (phase.text == text) return
        phase.animate().cancel()
        phase.alpha = 0f
        phase.translationY = dp(6f)
        phase.text = text
        phase.animate().alpha(1f).translationY(0f).setDuration(320).setInterpolator(Motion.outExpo).start()
        cancel.visibility = if (canCancel) View.VISIBLE else View.INVISIBLE
    }

    /** The picture is there: fade away to show it. */
    fun reveal(then: () -> Unit) {
        animate().alpha(0f).setDuration(320).setInterpolator(Motion.easeInOut).withEndAction {
            visibility = View.GONE
            then()
        }.start()
    }

    /** Back into the tile (or just away, if it's gone), then `then`. */
    fun close(to: RectF?, then: () -> Unit) {
        visibility = View.VISIBLE
        content.animate().alpha(0f).setStartDelay(0).setDuration(160).start()
        shade.animate().alpha(0f).setStartDelay(0).setDuration(300).start()
        animate().alpha(1f).setDuration(180).withEndAction {
            if (to == null) {
                animate().alpha(0f).setDuration(260).withEndAction(then).start()
                return@withEndAction
            }
            tile.set(to)
            fadeDesk(1f, 0)
            animateTo(0f, Motion.ZOOM) { then() }
            // Hand back to the tile's own screen as it lands.
            animate().alpha(0f).setStartDelay((Motion.ZOOM * 0.55f).toLong()).setDuration((Motion.ZOOM * 0.35f).toLong()).start()
        }.start()
    }
}
