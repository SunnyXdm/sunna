package dev.sunna.app

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.os.SystemClock
import android.view.View
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.max
import kotlin.math.min

/**
 * Where the picture sits: fitted to the screen, then zoomed and moved by the
 * user's fingers. Stream coordinates are the host's pixels; view coordinates
 * are this phone's.
 */
class Viewport {
    /** The area to fit the picture in (the screen, less any camera cutout). */
    val safe = RectF()
    var viewWidth = 0f
    var viewHeight = 0f
    var streamWidth = 0
    var streamHeight = 0
    /** 1 is fitted; more is closer. */
    var zoom = 1f
        private set
    /** The picture's center, offset from the fitted center. */
    private var panX = 0f
    private var panY = 0f
    val picture = RectF()

    val ready: Boolean get() = streamWidth > 0 && streamHeight > 0 && safe.width() > 0

    private val fit: Float
        get() = if (!ready) 1f else min(safe.width() / streamWidth, safe.height() / streamHeight)

    /** View pixels per stream pixel. */
    val scale: Float get() = fit * zoom

    /** Close enough that a host pixel is three of ours, and at least 2×. */
    private val maxZoom: Float get() = max(2f, 3f / fit)

    fun update() {
        if (!ready) {
            picture.set(safe)
            return
        }
        val w = streamWidth * scale
        val h = streamHeight * scale
        val cx = safe.centerX() + panX
        val cy = safe.centerY() + panY
        picture.set(cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2)
    }

    fun reset() {
        zoom = 1f
        panX = 0f
        panY = 0f
        update()
    }

    fun toStreamX(x: Float) = (x - picture.left) / scale
    fun toStreamY(y: Float) = (y - picture.top) / scale
    fun toViewX(x: Float) = picture.left + x * scale
    fun toViewY(y: Float) = picture.top + y * scale

    /** Zoom by `factor` around (fx, fy), and move by (dx, dy): a pinch. */
    fun pinch(factor: Float, fx: Float, fy: Float, dx: Float, dy: Float) {
        if (!ready) return
        val sx = toStreamX(fx)
        val sy = toStreamY(fy)
        zoom = (zoom * factor).coerceIn(1f, maxZoom)
        update()
        // Keep the point under the fingers there, moved with them.
        panX += fx + dx - toViewX(sx)
        panY += fy + dy - toViewY(sy)
        clamp()
    }

    fun panBy(dx: Float, dy: Float) {
        panX += dx
        panY += dy
        clamp()
    }

    /** A zoomed picture covers the area it's shown in (above the keyboard,
     *  clear of the cutout); no empty margins past its edges. */
    private fun clamp() {
        update()
        val w = picture.width()
        val h = picture.height()
        panX = if (w <= safe.width()) 0f else panX.coerceIn(safe.right - w / 2 - safe.centerX(), safe.left + w / 2 - safe.centerX())
        panY = if (h <= safe.height()) 0f else panY.coerceIn(safe.bottom - h / 2 - safe.centerY(), safe.top + h / 2 - safe.centerY())
        update()
    }

    /** Move the picture so (x, y), in view coordinates, is at least `margin`
     *  inside the screen: the zoomed view follows the pointer. */
    fun follow(x: Float, y: Float, margin: Float): Boolean {
        if (zoom <= 1.001f) return false
        var dx = 0f
        var dy = 0f
        if (x < safe.left + margin) dx = safe.left + margin - x else if (x > safe.right - margin) dx = safe.right - margin - x
        if (y < safe.top + margin) dy = safe.top + margin - y else if (y > safe.bottom - margin) dy = safe.bottom - margin - y
        if (dx == 0f && dy == 0f) return false
        panBy(dx, dy)
        return true
    }
}

/**
 * The host's pointer, drawn over the picture where it is, in the shape the
 * host says (a text beam over text, a hand over a link), sized like the rest
 * of the picture. Taps leave a brief ring.
 */
class PointerView(context: Context, private val viewport: Viewport) : View(context) {
    /** Where the pointer is, in stream pixels. */
    var streamX = 0f
        private set
    var streamY = 0f
        private set
    var placed = false
        private set
    var shown = true
        set(value) {
            field = value
            invalidate()
        }
    private var shape: Bitmap? = null
    private var shapeId = -1L
    private var hotX = 0
    private var hotY = 0
    private var screenWidth = 0
    private val bitmapPaint = Paint(Paint.FILTER_BITMAP_FLAG or Paint.ANTI_ALIAS_FLAG)
    private val ring = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val arrowFill = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE }
    private val arrowEdge = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.BLACK
        style = Paint.Style.STROKE
        strokeJoin = Paint.Join.ROUND
    }
    private val arrow = Path()
    private val box = RectF()
    private val rings = ArrayList<FloatArray>()

    init {
        // The macOS arrow's outline, in a 12×19 box (hot spot at the tip).
        arrow.moveTo(0f, 0f)
        arrow.lineTo(0f, 16.5f)
        arrow.lineTo(4f, 12.8f)
        arrow.lineTo(6.6f, 18.6f)
        arrow.lineTo(9f, 17.6f)
        arrow.lineTo(6.5f, 11.9f)
        arrow.lineTo(11.6f, 11.6f)
        arrow.close()
    }

    fun moveTo(streamX: Float, streamY: Float) {
        this.streamX = streamX.coerceIn(0f, max(0f, viewport.streamWidth - 1f))
        this.streamY = streamY.coerceIn(0f, max(0f, viewport.streamHeight - 1f))
        placed = true
        invalidate()
    }

    /** A shape from `Native.cursor`: a 20-byte header, then premultiplied RGBA. */
    fun setShape(bytes: ByteArray?) {
        if (bytes == null || bytes.size < 20) return
        val header = ByteBuffer.wrap(bytes, 0, 20).order(ByteOrder.LITTLE_ENDIAN)
        val id = header.long
        val width = header.short.toInt() and 0xFFFF
        val height = header.short.toInt() and 0xFFFF
        hotX = header.short.toInt() and 0xFFFF
        hotY = header.short.toInt() and 0xFFFF
        screenWidth = header.int
        if (id == shapeId) return invalidate()
        if (width == 0 || height == 0 || bytes.size < 20 + width * height * 4) return
        // ARGB_8888 is premultiplied RGBA in memory, as the host sends it.
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(bytes, 20, width * height * 4))
        shape = bitmap
        shapeId = id
        invalidate()
    }

    /** A ring where a click landed. */
    fun pulse() {
        rings += floatArrayOf(streamX, streamY, SystemClock.uptimeMillis().toFloat())
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        if (!viewport.ready) return
        val now = SystemClock.uptimeMillis()
        val iterator = rings.iterator()
        while (iterator.hasNext()) {
            val r = iterator.next()
            val t = (now - r[2]) / 380f
            if (t >= 1f) {
                iterator.remove()
                continue
            }
            val eased = Motion.outExpo.getInterpolation(t)
            ring.strokeWidth = dp(2f)
            ring.color = withAlpha(Color.WHITE, 0.55f * (1 - t))
            canvas.drawCircle(viewport.toViewX(r[0]), viewport.toViewY(r[1]), dp(6f) + dp(18f) * eased, ring)
        }
        if (shown && placed) {
            val px = viewport.toViewX(streamX)
            val py = viewport.toViewY(streamY)
            val bitmap = shape
            if (bitmap != null && screenWidth > 0) {
                // As big as the host shows it, relative to its screen.
                val size = viewport.picture.width() / screenWidth
                val s = max(size, dp(0.55f) / 2)
                box.set(px - hotX * s, py - hotY * s, px + (bitmap.width - hotX) * s, py + (bitmap.height - hotY) * s)
                canvas.drawBitmap(bitmap, null, box, bitmapPaint)
            } else {
                val s = dp(1.25f)
                canvas.save()
                canvas.translate(px, py)
                canvas.scale(s, s)
                arrowEdge.strokeWidth = 1.4f
                canvas.drawPath(arrow, arrowEdge)
                canvas.drawPath(arrow, arrowFill)
                canvas.restore()
            }
        }
        if (rings.isNotEmpty()) postInvalidateOnAnimation()
    }
}
