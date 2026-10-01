package dev.sunna.app

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.ColorMatrix
import android.graphics.ColorMatrixColorFilter
import android.graphics.LinearGradient
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RadialGradient
import android.graphics.RectF
import android.graphics.Shader
import android.graphics.drawable.Drawable
import android.os.SystemClock
import android.view.View
import android.view.animation.PathInterpolator
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin

/**
 * A computer's screen, drawn as the desktop app draws it: dark glass while
 * it's away, its OS's wallpaper and desktop when it answers, a boot screen
 * as it comes up. Tiles, the add sheet's preview and the connecting
 * animation all use it.
 */
class ScreenView(context: Context) : View(context) {
    private enum class Transition { NONE, BOOT, WAKE, SLEEP }

    var os: Os = Os.UNKNOWN
        set(value) {
            if (field == value) return
            field = value
            mark = null
            invalidate()
        }

    /** "laptop" sits on a keyboard deck; "desktop" on a stand. */
    var device: String = ""
        set(value) {
            field = value
            invalidate()
        }

    /** Under the penguin on a Linux boot screen. */
    var bootName: String = ""

    /** The display's shape, width over height. */
    var ratio: Float = 1.6f
        set(value) {
            field = if (value.isFinite() && value > 0) value.coerceIn(1.25f, 2.4f) else 1.6f
            invalidate()
        }

    /** Just the screen, filling the view: no stand, no light below (the
     *  connecting animation, which also sets the corners and bezel). */
    var bare = false
    var cornerRadius = 0f

    /** How much of the desktop's details show (the connecting animation
     *  fades them out once the screen fills the display, as on the desktop). */
    var deskFade = 1f
        set(value) {
            field = value
            invalidate()
        }
    var bezel = 0f

    /** Under a finger: the light below brightens. */
    var lifted = false
        set(value) {
            if (field == value) return
            field = value
            liftedAt = SystemClock.uptimeMillis()
            invalidate()
        }

    /** Whether to keep animating (off while the app is in the background or
     *  a session covers the screen). */
    var running = true
        set(value) {
            field = value
            if (value) invalidate()
        }

    var state = "checking"
        private set
    private var lit = false
    private var transition = Transition.NONE
    private var transitionAt = 0L
    private var liftedAt = 0L
    private val driftSeed = (Math.random() * 60_000).toLong()
    private var mark: Drawable? = null

    /** Show a computer's state; screens that let us in boot first. */
    fun show(state: String, wakeDelay: Long = 0, animate: Boolean = true) {
        val nowLit = state in Check.LIT
        val now = SystemClock.uptimeMillis()
        if (nowLit && !lit) {
            transition = when {
                !animate -> Transition.NONE
                state == "ready" || state == "busy" -> Transition.BOOT
                else -> Transition.WAKE
            }
            transitionAt = now + wakeDelay
        } else if (!nowLit && lit) {
            transition = if (animate) Transition.SLEEP else Transition.NONE
            transitionAt = now
        }
        lit = nowLit
        this.state = state
        invalidate()
    }

    // ---- Paints and shapes, reused frame to frame ------------------------------------

    private val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    private val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val shaded = Paint(Paint.ANTI_ALIAS_FLAG)
    private val layer = Paint()
    private val gray = Paint().apply {
        colorFilter = ColorMatrixColorFilter(ColorMatrix().apply {
            setSaturation(0.3f)
            postConcat(ColorMatrix().apply { setScale(0.5f, 0.5f, 0.5f, 1f) })
        })
    }
    private val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = Type.semibold
        textAlign = Paint.Align.CENTER
        letterSpacing = 0.22f
    }
    private val stage = RectF()
    private val screen = RectF()
    private val inner = RectF()
    private val box = RectF()
    private val clip = Path()
    private val shape = Path()
    private val lock: Drawable? = context.getDrawable(R.drawable.ic_lock)?.mutate()?.apply { setTint(white(.9f)) }
    private val up: Drawable? = context.getDrawable(R.drawable.ic_up)?.mutate()?.apply { setTint(white(.9f)) }
    private val bootCurve = PathInterpolator(0.35f, 0.6f, 0.35f, 1f)

    private fun phase(elapsed: Float, start: Float, duration: Float): Float = ((elapsed - start) / duration).coerceIn(0f, 1f)

    override fun onDraw(canvas: Canvas) {
        val w = width.toFloat()
        val h = height.toFloat()
        if (w <= 0 || h <= 0) return
        val now = SystemClock.uptimeMillis()
        val elapsed = (now - transitionAt).toFloat()

        // Where the transition is: how lit the wallpaper is, the boot screen.
        var wall = if (lit) 1f else 0f
        var wallScale = 1f
        var desk = wall
        var boot = 0f
        var progress = 0f
        var bloom = if (lit) 1f else 0f
        var dusk = 0f
        when (transition) {
            Transition.BOOT -> {
                val u = elapsed / 1350f
                boot = when {
                    u < 0f -> 0f
                    u < 0.08f -> u / 0.08f
                    u < 0.72f -> 1f
                    u < 1f -> 1f - (u - 0.72f) / 0.28f
                    else -> 0f
                }
                progress = bootCurve.getInterpolation(phase(elapsed, 200f, 850f))
                val wake = Motion.outExpo.getInterpolation(phase(elapsed, 950f, 900f))
                wall = wake
                wallScale = 1.06f - 0.06f * wake
                desk = phase(elapsed, 1150f, 600f)
                bloom = Motion.outExpo.getInterpolation(phase(elapsed, 900f, 1400f))
                if (elapsed > 2350f) transition = Transition.NONE
            }
            Transition.WAKE -> {
                val wake = Motion.outExpo.getInterpolation(phase(elapsed, 0f, 900f))
                wall = wake
                wallScale = 1.06f - 0.06f * wake
                desk = phase(elapsed, 200f, 600f)
                bloom = Motion.outExpo.getInterpolation(phase(elapsed, 0f, 1400f))
                if (elapsed > 1450f) transition = Transition.NONE
            }
            Transition.SLEEP -> {
                val sleep = Motion.easeIn.getInterpolation(phase(elapsed, 0f, 700f))
                wall = 1f - sleep
                wallScale = 1f - 0.03f * sleep
                desk = 1f - sleep
                bloom = 1f - sleep
                dusk = sleep * 0.6f
                if (elapsed > 720f) transition = Transition.NONE
            }
            Transition.NONE -> {}
        }

        if (bare) {
            screen.set(0f, 0f, w, h)
            drawScreen(canvas, now, cornerRadius, bezel, wall, wallScale, desk, boot, progress, dusk)
        } else {
            var sw = w
            var sh = w * 11f / 16f
            if (sh > h) {
                sh = h
                sw = h * 16f / 11f
            }
            stage.set((w - sw) / 2, 0f, (w + sw) / 2, sh)
            val area = sw * 10f / 16f
            val (screenW, screenH) = if (ratio >= 1.6f) sw to sw / ratio else area * ratio to area
            screen.set(stage.centerX() - screenW / 2, stage.top + area - screenH, stage.centerX() + screenW / 2, stage.top + area)
            drawGlow(canvas, now, sw, sh, bloom)
            drawBase(canvas, sw, sh, screenW)
            drawScreen(canvas, now, dp(11f), dp(4f), wall, wallScale, desk, boot, progress, dusk)
        }

        // Keep moving while something moves: smoothly through transitions and
        // the sweep; the wallpaper's slow drift needs far fewer frames.
        if (!running || !isAttachedToWindow) return
        when {
            transition != Transition.NONE || state == "checking" || state == "busy" || now - liftedAt < 700 -> postInvalidateOnAnimation()
            lit -> postInvalidateDelayed(66)
        }
    }

    /** The screen's light: around its edges, and pooled on the surface below. */
    private fun drawGlow(canvas: Canvas, now: Long, sw: Float, sh: Float, bloom: Float) {
        if (bloom <= 0f) return
        val dim = state == "busy" || state == "wrong-key" || state == "update-needed"
        val liftedBy = Motion.gentle.getInterpolation(((now - liftedAt) / Motion.GENTLE.toFloat()).coerceIn(0f, 1f))
        val lift = if (lifted) liftedBy else 1f - liftedBy
        val opacity = (if (dim) 0.14f else 0.5f + 0.35f * lift * (if (state == "ready") 1f else 0f)) * bloom
        val grow = (0.6f + 0.4f * bloom) * (1f + 0.06f * lift)
        val cx = stage.centerX()
        val cy = screen.centerY() + screen.height() * (0.1f + 0.04f * lift)
        val rx = max(screen.width() * 0.62f, sw * 0.5f) * grow
        val ry = screen.height() * 0.66f * grow
        val colors = Looks.glow(os)
        for ((i, color) in colors.withIndex()) {
            val x = cx + (if (colors.size > 1) (if (i == 0) -0.16f else 0.16f) else 0f) * screen.width()
            val gradient = RadialGradient(x, cy, rx,
                intArrayOf(withAlpha(color, opacity), withAlpha(color, opacity * 0.8f), withAlpha(color, opacity * 0.26f), withAlpha(color, 0f)),
                floatArrayOf(0f, 0.6f, 0.82f, 1f), Shader.TileMode.CLAMP)
            gradient.setLocalMatrix(Matrix().apply { setScale(1f, ry / rx, x, cy) })
            shaded.shader = gradient
            canvas.drawRect(x - rx, cy - ry, x + rx, cy + ry, shaded)
        }
        shaded.shader = null
    }

    /** Laptops sit on a keyboard deck; desktops on a stand. */
    private fun drawBase(canvas: Canvas, sw: Float, sh: Float, screenW: Float) {
        val top = stage.top + sh * 0.909f
        when (device) {
            "laptop" -> {
                val deckW = screenW + sw * 0.07f
                val deckH = max(sh * 0.034f, dp(3f))
                box.set(stage.centerX() - deckW / 2, top, stage.centerX() + deckW / 2, top + deckH)
                val corner = min(dp(14f), deckW * 0.08f)
                shape.reset()
                shape.addRoundRect(box, floatArrayOf(1f, 1f, 1f, 1f, corner, deckH * 0.9f, corner, deckH * 0.9f), Path.Direction.CW)
                shaded.shader = LinearGradient(0f, box.top, 0f, box.bottom, intArrayOf(0xFF555A66.toInt(), 0xFF2D3038.toInt(), 0xFF17191E.toInt()), floatArrayOf(0f, 0.38f, 1f), Shader.TileMode.CLAMP)
                canvas.drawPath(shape, shaded)
                shaded.shader = null
                fill.color = white(.3f)
                canvas.drawRect(box.left + corner * 0.3f, box.top, box.right - corner * 0.3f, box.top + max(1f, dp(0.5f)), fill)
                // The notch that opens the lid.
                val notchW = deckW * 0.15f
                box.set(stage.centerX() - notchW / 2, top, stage.centerX() + notchW / 2, top + deckH * 0.42f)
                fill.color = 0x66000000
                canvas.drawRoundRect(box, deckH * 0.3f, deckH * 0.3f, fill)
            }
            "desktop" -> {
                val neckW = sw * 0.10f
                val neckH = sh * 0.07f
                val left = stage.centerX() - neckW / 2
                shape.reset()
                shape.moveTo(left + neckW * 0.24f, top)
                shape.lineTo(left + neckW * 0.76f, top)
                shape.lineTo(left + neckW * 0.88f, top + neckH)
                shape.lineTo(left + neckW * 0.12f, top + neckH)
                shape.close()
                shaded.shader = LinearGradient(left, 0f, left + neckW, 0f, intArrayOf(0xFF202228.toInt(), 0xFF3D414B.toInt(), 0xFF202228.toInt()), null, Shader.TileMode.CLAMP)
                canvas.drawPath(shape, shaded)
                val footW = sw * 0.26f
                val footH = max(sh * 0.022f, dp(2.5f))
                val footTop = stage.top + sh * 0.974f
                box.set(stage.centerX() - footW / 2, footTop, stage.centerX() + footW / 2, footTop + footH)
                shape.reset()
                shape.addRoundRect(box, floatArrayOf(footH, footH, footH, footH, footH * 0.5f, footH * 0.5f, footH * 0.5f, footH * 0.5f), Path.Direction.CW)
                shaded.shader = LinearGradient(0f, box.top, 0f, box.bottom, 0xFF4A4E57.toInt(), 0xFF1A1C21.toInt(), Shader.TileMode.CLAMP)
                canvas.drawPath(shape, shaded)
                shaded.shader = null
            }
        }
    }

    private fun drawScreen(canvas: Canvas, now: Long, radius: Float, bezelWidth: Float, wall: Float, wallScale: Float, desk: Float, boot: Float, progress: Float, dusk: Float) {
        // The body, and a hairline around it.
        fill.color = 0xFF060709.toInt()
        canvas.drawRoundRect(screen, radius, radius, fill)
        inner.set(screen.left + bezelWidth, screen.top + bezelWidth, screen.right - bezelWidth, screen.bottom - bezelWidth)
        val innerRadius = max(radius - bezelWidth, if (radius > 0) dp(2f) else 0f)
        canvas.save()
        clip.reset()
        clip.addRoundRect(inner, innerRadius, innerRadius, Path.Direction.CW)
        canvas.clipPath(clip)

        // An asleep screen: dark glass with a reflection.
        fill.color = 0xFF08090C.toInt()
        canvas.drawRect(inner, fill)
        shaded.shader = css(128f, inner, intArrayOf(white(.05f), white(.012f), 0x00FFFFFF), floatArrayOf(0f, 0.34f, 0.35f))
        canvas.drawRect(inner, shaded)
        shaded.shader = null

        val locked = state == "wrong-key" || state == "update-needed"
        if (wall > 0f) {
            val save = if (state == "update-needed") canvas.saveLayer(inner, gray) else canvas.save()
            if (wall < 1f) canvas.saveLayerAlpha(inner, (wall * 255).toInt())
            canvas.scale(wallScale, wallScale, inner.centerX(), inner.centerY())
            val drift = ((now + driftSeed) % 60_000L) / 30_000f
            Looks.paintWall(canvas, inner, os, if (drift < 1f) drift else 2f - drift)
            canvas.restoreToCount(save)
            val shade = when (state) {
                "busy" -> 0.3f
                "wrong-key" -> 0.45f
                else -> 0f
            } + dusk
            if (shade > 0f) {
                fill.color = withAlpha(Color.BLACK, min(shade, 1f) * max(wall, dusk))
                canvas.drawRect(inner, fill)
            }
        }
        if (desk * deskFade > 0f) {
            val alpha = desk * deskFade * (if (locked) 0.35f else 1f)
            val save = canvas.saveLayerAlpha(inner, (alpha * 255).toInt())
            canvas.scale(wallScale, wallScale, inner.centerX(), inner.centerY())
            Looks.paintDesk(canvas, inner, os, max(1f, dp(0.5f)))
            canvas.restoreToCount(save)
        }

        val cw = inner.width() / 100
        val ch = inner.height() / 100
        if (state == "checking") drawSweep(canvas, now)
        if (locked && wall > 0f) {
            // A locked screen (wrong key), or one waiting for an update.
            val glyph = if (state == "wrong-key") lock else up
            val size = 13 * cw
            val pill = state == "wrong-key"
            val total = size + if (pill) 5 * ch + 8 * ch else 0f
            val top = inner.centerY() - total / 2
            glyph?.setBounds((inner.centerX() - size / 2).toInt(), top.toInt(), (inner.centerX() + size / 2).toInt(), (top + size).toInt())
            glyph?.alpha = (wall * 255).toInt()
            glyph?.draw(canvas)
            if (pill) {
                box.set(inner.centerX() - 17 * cw, top + size + 5 * ch, inner.centerX() + 17 * cw, top + size + 13 * ch)
                fill.color = withAlpha(Color.WHITE, 0.16f * wall)
                canvas.drawRoundRect(box, box.height() / 2, box.height() / 2, fill)
                stroke.color = withAlpha(Color.WHITE, 0.18f * wall)
                stroke.strokeWidth = max(1f, dp(0.5f))
                canvas.drawRoundRect(box, box.height() / 2, box.height() / 2, stroke)
            }
        }
        if (state == "busy" && wall > 0f) {
            // Someone's watching: a small light, like the menu bar's.
            val size = max(dp(6f), 3.2f * cw)
            val cx = inner.right - 5 * cw - size / 2
            val cy = inner.top + 7 * ch + size / 2
            val pulse = 0.7f + 0.3f * cos((now % 2000L) / 2000f * 2 * Math.PI).toFloat()
            fill.color = withAlpha(Color.BLACK, 0.35f * wall)
            canvas.drawCircle(cx, cy, size / 2 + dp(2f), fill)
            fill.color = withAlpha(Palette.BUSY, pulse * wall)
            canvas.drawCircle(cx, cy, size / 2, fill)
        }
        if (boot > 0f) drawBoot(canvas, boot, progress, cw)
        canvas.restore()

        // A fixed sheen over the glass, and the screen's edge.
        canvas.save()
        clip.reset()
        clip.addRoundRect(screen, radius, radius, Path.Direction.CW)
        canvas.clipPath(clip)
        shaded.shader = css(150f, screen, intArrayOf(white(.07f), 0x00FFFFFF), floatArrayOf(0f, 0.34f))
        canvas.drawRect(screen, shaded)
        shaded.shader = null
        canvas.restore()
        if (radius > 0f || bezelWidth > 0f) {
            stroke.color = white(.1f)
            stroke.strokeWidth = max(1f, dp(1f))
            box.set(screen.left + stroke.strokeWidth / 2, screen.top + stroke.strokeWidth / 2, screen.right - stroke.strokeWidth / 2, screen.bottom - stroke.strokeWidth / 2)
            canvas.drawRoundRect(box, radius, radius, stroke)
        }
    }

    /** Looking for it: a soft light sweeps across the glass. */
    private fun drawSweep(canvas: Canvas, now: Long) {
        val cycle = (now % 1900L) / 1900f
        val eased = Motion.easeInOut.getInterpolation(cycle)
        val w = inner.width()
        val center = inner.left + (-0.7f + 3.0f * eased) * w
        val band = w * 0.5f
        val tilt = inner.height() * 0.17f
        shaded.shader = LinearGradient(center - band, inner.top + tilt, center + band, inner.bottom - tilt,
            intArrayOf(0x00FFFFFF, white(.05f), white(.1f), white(.05f), 0x00FFFFFF), floatArrayOf(0f, 0.4f, 0.5f, 0.6f, 1f), Shader.TileMode.CLAMP)
        canvas.drawRect(inner, shaded)
        shaded.shader = null
    }

    /** Booting: black, the OS's mark, its name for Linux, a progress bar. */
    private fun drawBoot(canvas: Canvas, alpha: Float, progress: Float, cw: Float) {
        fill.color = withAlpha(Color.BLACK, alpha)
        canvas.drawRect(inner, fill)
        val drawable = mark ?: context.getDrawable(os.mark)?.mutate()?.also {
            if (os.mark != R.drawable.mark_tux) it.setTint(Color.WHITE)
            mark = it
        }
        val size = 17 * cw
        val name = bootName.uppercase()
        text.textSize = max(dp(7f), 3.4f * cw)
        val nameH = if (name.isNotEmpty()) 2.5f * cw + text.textSize else 0f
        val barH = max(dp(2f), 1.1f * cw)
        val total = size + nameH + 5 * cw + barH
        var y = inner.centerY() - total / 2
        drawable?.setBounds((inner.centerX() - size / 2).toInt(), y.toInt(), (inner.centerX() + size / 2).toInt(), (y + size).toInt())
        drawable?.alpha = (alpha * 255).toInt()
        drawable?.draw(canvas)
        y += size
        if (name.isNotEmpty()) {
            text.color = withAlpha(Color.WHITE, 0.6f * alpha)
            canvas.drawText(name, inner.centerX(), y + 2.5f * cw + text.textSize * 0.8f, text)
            y += nameH
        }
        y += 5 * cw
        val barW = 28 * cw
        box.set(inner.centerX() - barW / 2, y, inner.centerX() + barW / 2, y + barH)
        fill.color = withAlpha(Color.WHITE, 0.2f * alpha)
        canvas.drawRoundRect(box, barH / 2, barH / 2, fill)
        box.right = box.left + barW * progress
        fill.color = withAlpha(Color.WHITE, alpha)
        if (progress > 0f) canvas.drawRoundRect(box, barH / 2, barH / 2, fill)
    }

    /** A CSS linear-gradient at `angle` over `rect`. */
    private fun css(angle: Float, rect: RectF, colors: IntArray, stops: FloatArray): LinearGradient {
        val radians = Math.toRadians(angle.toDouble())
        val dx = sin(radians).toFloat()
        val dy = -cos(radians).toFloat()
        val half = (abs(rect.width() * dx) + abs(rect.height() * dy)) / 2
        return LinearGradient(rect.centerX() - dx * half, rect.centerY() - dy * half, rect.centerX() + dx * half, rect.centerY() + dy * half, colors, stops, Shader.TileMode.CLAMP)
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        invalidate()
    }
}
