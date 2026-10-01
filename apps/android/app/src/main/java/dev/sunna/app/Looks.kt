package dev.sunna.app

import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.RadialGradient
import android.graphics.RectF
import android.graphics.Shader
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin

/** Which look a computer's screen gets, from the OS it reports. */
enum class Os(val key: String) {
    MACOS("macos"), WINDOWS("windows"), ARCH("arch"), UBUNTU("ubuntu"), FEDORA("fedora"), DEBIAN("debian"),
    MINT("mint"), MANJARO("manjaro"), NIXOS("nixos"), POP("pop"), LINUX("linux"), UNKNOWN("unknown");

    /** Our own marks: ⌘ for macOS, the penguin for Linux, a plain window for
     *  Windows. Never the vendors' logos (their trademark rules don't allow it). */
    val mark: Int
        get() = when (this) {
            MACOS -> R.drawable.mark_command
            WINDOWS -> R.drawable.mark_window
            UNKNOWN -> R.drawable.mark_screen
            else -> R.drawable.mark_tux
        }

    val isLinux: Boolean get() = this != MACOS && this != WINDOWS && this != UNKNOWN

    companion object {
        fun of(os: String): Os {
            val name = os.lowercase()
            if ("mac" in name) return MACOS
            if ("windows" in name) return WINDOWS
            for (distro in listOf(ARCH, UBUNTU, FEDORA, DEBIAN, MINT, MANJARO, NIXOS)) {
                if (distro.key in name) return distro
            }
            if (name.startsWith("pop")) return POP
            if ("linux" in name) return LINUX
            return UNKNOWN
        }

        /** What a Linux boot screen says under the penguin: the distro, no version. */
        fun bootName(os: String): String {
            if (!of(os).isLinux) return ""
            return os.replace(Regex("""\s*\(.*\)$"""), "").replace(Regex("""\s+[\d.]+.*$"""), "")
        }
    }
}

private fun c(value: Long): Int = value.toInt()

/** A CSS radial-gradient layer: an ellipse with radii (w, h) as fractions of
 *  the wallpaper box, centered at (x, y), `color` fading out by `stop`. */
private class Radial(val w: Float, val h: Float, val x: Float, val y: Float, val color: Int, val stop: Float)

/** A wallpaper: a linear gradient at `angle` (CSS degrees), radial layers
 *  over it (listed top first, as CSS lists them). */
private class Wall(val angle: Float, val from: Int, val to: Int, val radials: List<Radial>)

private val WALLS: Map<Os, Wall> = mapOf(
    Os.MACOS to Wall(160f, c(0xFF1B1446), c(0xFF3A1A5C), listOf(
        Radial(.60f, .80f, .12f, 1.02f, c(0xFFFF7A59), .62f),
        Radial(.70f, .90f, .92f, 0f, c(0xFF4F7DFF), .60f),
        Radial(.60f, .60f, .56f, .56f, c(0xFFB04DFF), .70f),
        Radial(.55f, .65f, 0f, .18f, c(0xFFFF4FA0), .62f),
    )),
    Os.ARCH to Wall(165f, c(0xFF04101B), c(0xFF0B2233), listOf(
        Radial(.70f, .90f, .80f, .10f, c(0xFF1793D1), .60f),
        Radial(.80f, .70f, .08f, 1f, c(0xFF0B5A8E), .65f),
        Radial(.40f, .50f, .40f, .45f, c(0x595AC8FF), .70f),
    )),
    Os.UBUNTU to Wall(160f, c(0xFF2A0B22), c(0xFF4B1335), listOf(
        Radial(.70f, .90f, .86f, .08f, c(0xFFE95420), .60f),
        Radial(.80f, .80f, .04f, .96f, c(0xFF77216F), .66f),
        Radial(.50f, .50f, .52f, .52f, c(0x47FF8C50), .70f),
    )),
    Os.FEDORA to Wall(160f, c(0xFF0B1630), c(0xFF13254A), listOf(
        Radial(.70f, .90f, .78f, .12f, c(0xFF51A2DA), .60f),
        Radial(.80f, .80f, .10f, 1f, c(0xFF294172), .65f),
        Radial(.40f, .40f, .46f, .50f, c(0x40C8E6FF), .70f),
    )),
    Os.DEBIAN to Wall(160f, c(0xFF1D0712), c(0xFF33091F), listOf(
        Radial(.70f, .90f, .80f, .10f, c(0xFFD70A53), .60f),
        Radial(.80f, .80f, .08f, 1f, c(0xFF5A0D3A), .66f),
    )),
    Os.MINT to Wall(160f, c(0xFF06150F), c(0xFF0D2A1E), listOf(
        Radial(.70f, .90f, .80f, .10f, c(0xFF6CCF3E), .60f),
        Radial(.80f, .80f, .08f, 1f, c(0xFF1D6B50), .66f),
    )),
    Os.POP to Wall(160f, c(0xFF0B1B20), c(0xFF1A2F33), listOf(
        Radial(.70f, .90f, .82f, .10f, c(0xFF48B9C7), .60f),
        Radial(.70f, .80f, .10f, 1f, c(0xFFFAA41A), .60f),
    )),
    Os.NIXOS to Wall(160f, c(0xFF0A1428), c(0xFF16264A), listOf(
        Radial(.70f, .90f, .80f, .10f, c(0xFF7EBAE4), .60f),
        Radial(.80f, .80f, .08f, 1f, c(0xFF5277C3), .66f),
    )),
    Os.LINUX to Wall(160f, c(0xFF04161A), c(0xFF0A2230), listOf(
        Radial(.70f, .90f, .10f, .08f, c(0xFF2EE6A8), .58f),
        Radial(.80f, .90f, .95f, 1f, c(0xFF1F7AFF), .62f),
        Radial(.50f, .50f, .50f, .55f, c(0x5914BEB4), .70f),
    )),
    Os.WINDOWS to Wall(160f, c(0xFF06112E), c(0xFF0B1A44), listOf(
        Radial(.80f, .90f, .70f, 0f, c(0xFF3AA0FF), .60f),
        Radial(.90f, .90f, .20f, 1f, c(0xFF1B3FD6), .65f),
        Radial(.40f, .40f, .55f, .45f, c(0x59B4DCFF), .70f),
    )),
    Os.UNKNOWN to Wall(160f, c(0xFF15171D), c(0xFF23262F), listOf(
        Radial(.90f, .90f, .30f, .20f, c(0xFF4B5162), .60f),
    )),
)

private fun wallOf(os: Os): Wall = WALLS[if (os == Os.MANJARO) Os.MINT else os] ?: WALLS.getValue(Os.UNKNOWN)

/** Paints computers' screens, as the Mac and Linux app draws them. */
object Looks {
    /** The color under an OS's mark, like a small app icon's plate. */
    fun plate(os: Os): IntArray = when (os) {
        Os.MACOS -> intArrayOf(c(0xFF5D6069), c(0xFF2C2E34))
        Os.ARCH -> intArrayOf(c(0xFF1793D1), c(0xFF1793D1))
        Os.UBUNTU -> intArrayOf(c(0xFFE95420), c(0xFFE95420))
        Os.FEDORA -> intArrayOf(c(0xFF3C6EB4), c(0xFF3C6EB4))
        Os.DEBIAN -> intArrayOf(c(0xFFD70A53), c(0xFFD70A53))
        Os.MINT, Os.MANJARO -> intArrayOf(c(0xFF3FAE4E), c(0xFF3FAE4E))
        Os.POP -> intArrayOf(c(0xFF48B9C7), c(0xFF48B9C7))
        Os.NIXOS -> intArrayOf(c(0xFF5277C3), c(0xFF5277C3))
        Os.LINUX -> intArrayOf(c(0xFFF0EDE6), c(0xFFF0EDE6))
        Os.WINDOWS -> intArrayOf(c(0xFF0F6CBD), c(0xFF0F6CBD))
        Os.UNKNOWN -> intArrayOf(c(0xFF4A4D56), c(0xFF2C2E35))
    }

    /** The wallpaper's brightest colors, for the light it casts below. */
    fun glow(os: Os): IntArray = wallOf(os).radials.take(2).map { it.color or 0xFF000000.toInt() }.toIntArray()

    private class Cached(val os: Os, val width: Float, val height: Float, val paints: List<Paint>)

    private val cache = ArrayList<Cached>()

    private fun paints(os: Os, width: Float, height: Float): List<Paint> {
        cache.firstOrNull { it.os == os && it.width == width && it.height == height }?.let { return it.paints }
        val wall = wallOf(os)
        val paints = ArrayList<Paint>()
        // The base: CSS's linear gradient line runs through the center, as
        // long as the box is along it.
        val radians = Math.toRadians(wall.angle.toDouble())
        val dx = sin(radians).toFloat()
        val dy = -cos(radians).toFloat()
        val half = (abs(width * dx) + abs(height * dy)) / 2
        val (cx, cy) = width / 2 to height / 2
        paints += Paint(Paint.ANTI_ALIAS_FLAG).apply {
            shader = LinearGradient(cx - dx * half, cy - dy * half, cx + dx * half, cy + dy * half, wall.from, wall.to, Shader.TileMode.CLAMP)
        }
        for (layer in wall.radials.asReversed()) {
            val rx = max(layer.w * width, 1f)
            val ry = max(layer.h * height, 1f)
            val x = layer.x * width
            val y = layer.y * height
            val clear = layer.color and 0x00FFFFFF
            val gradient = RadialGradient(x, y, rx, intArrayOf(layer.color, clear), floatArrayOf(0f, layer.stop), Shader.TileMode.CLAMP)
            gradient.setLocalMatrix(Matrix().apply { setScale(1f, ry / rx, x, y) })
            paints += Paint(Paint.ANTI_ALIAS_FLAG).apply { shader = gradient }
        }
        if (cache.size > 48) cache.removeAt(0)
        cache += Cached(os, width, height, paints)
        return paints
    }

    /** The wallpaper filling `screen`, drifting slowly with `drift` (0..1). */
    fun paintWall(canvas: Canvas, screen: RectF, os: Os, drift: Float) {
        // As on the desktop: a box 18% larger on every side, so the drift
        // never shows an edge.
        val w = screen.width() * 1.36f
        val h = screen.height() * 1.36f
        val eased = (1 - cos(drift * Math.PI).toFloat()) / 2
        val tx = (-0.03f + 0.06f * eased) * w
        val ty = (-0.02f + 0.045f * eased) * h
        val scale = 1.02f + 0.08f * eased
        canvas.save()
        canvas.clipRect(screen)
        canvas.translate(screen.centerX() + tx, screen.centerY() + ty)
        canvas.scale(scale, scale)
        canvas.translate(-w / 2, -h / 2)
        for (paint in paints(os, w, h)) canvas.drawRect(0f, 0f, w, h, paint)
        canvas.restore()
    }

    private val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    private val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val box = RectF()

    private fun white(alpha: Float) = (((alpha * 255).toInt() shl 24) or 0xFFFFFF)
    private fun rgba(r: Int, g: Int, b: Int, a: Float) = (((a * 255).toInt() shl 24) or (r shl 16) or (g shl 8) or b)

    private fun round(canvas: Canvas, left: Float, top: Float, right: Float, bottom: Float, radius: Float, color: Int) {
        box.set(left, top, right, bottom)
        fill.color = color
        canvas.drawRoundRect(box, radius, radius, fill)
    }

    private fun outline(canvas: Canvas, left: Float, top: Float, right: Float, bottom: Float, radius: Float, color: Int, width: Float) {
        stroke.color = color
        stroke.strokeWidth = width
        box.set(left + width / 2, top + width / 2, right - width / 2, bottom - width / 2)
        canvas.drawRoundRect(box, radius, radius, stroke)
    }

    /** A window: its body, a lighter title band, a hairline edge. */
    private fun window(canvas: Canvas, left: Float, top: Float, right: Float, bottom: Float, radius: Float, body: Int, band: Int, bandShare: Float, edge: Int, hairline: Float) {
        round(canvas, left, top, right, bottom, radius, body)
        canvas.save()
        box.set(left, top, right, bottom)
        val clip = android.graphics.Path().apply { addRoundRect(box, radius, radius, android.graphics.Path.Direction.CW) }
        canvas.clipPath(clip)
        fill.color = band
        canvas.drawRect(left, top, right, top + (bottom - top) * bandShare, fill)
        canvas.restore()
        outline(canvas, left, top, right, bottom, radius, edge, hairline)
    }

    /** Desktop details (menu bar, dock, windows, panels), sized to the screen
     *  so they hold up from a tile to the whole display. */
    fun paintDesk(canvas: Canvas, r: RectF, os: Os, hairline: Float) {
        val w = r.width()
        val h = r.height()
        val cw = w / 100
        val ch = h / 100
        canvas.save()
        canvas.clipRect(r)
        when (os) {
            Os.MACOS -> {
                fill.color = white(.15f)
                canvas.drawRect(r.left, r.top, r.right, r.top + 5 * ch, fill)
                // A window, with its traffic lights.
                val wl = r.left + .12f * w
                val wt = r.top + .14f * h
                val ww = .44f * w
                val wh = .52f * h
                round(canvas, wl + .3f * cw, wt + 1.5f * cw, wl + ww + .3f * cw, wt + wh + 1.5f * cw, 1.6f * cw, rgba(0, 0, 0, .14f))
                window(canvas, wl, wt, wl + ww, wt + wh, 1.6f * cw, rgba(20, 16, 40, .3f), white(.1f), .12f, white(.14f), max(hairline, .2f * cw))
                val lights = intArrayOf(c(0xFFFF5F57), c(0xFFFEBC2E), c(0xFF28C840))
                for ((i, color) in lights.withIndex()) {
                    fill.color = color
                    canvas.drawCircle(wl + .035f * ww + .7f * cw + i * 2.3f * cw, wt + .028f * wh + .7f * cw, .7f * cw, fill)
                }
                // The Dock.
                val dl = r.left + .30f * w
                val dr = r.right - .30f * w
                val db = r.bottom - 2.4f * ch
                val dt = db - 9 * ch
                round(canvas, dl, dt, dr, db, 2.4f * ch, white(.17f))
                outline(canvas, dl, dt, dr, db, 2.4f * ch, white(.2f), max(hairline, .2f * cw))
                val dots = longArrayOf(0xFF4AA3FF, 0xFFF2F2F7, 0xFF34C759, 0xFFFF9F0A, 0xFFFF375F, 0xFFBF5AF2, 0xFFFFD60A, 0xFF8E8E93)
                val dockW = dr - dl
                val bw = .10f * dockW
                val bh = .72f * (db - dt)
                for ((i, color) in dots.withIndex()) {
                    val at = (5 + 13 * i) / 100f
                    fill.color = color.toInt()
                    canvas.drawCircle(dl + at * (dockW - bw) + bw / 2, (dt + db) / 2, .78f * min(bw, bh) / 2, fill)
                }
            }
            Os.ARCH -> {
                // A tiling window manager: one big window, two stacked.
                val tiles = listOf(
                    floatArrayOf(.025f, .04f, .585f, .96f),
                    floatArrayOf(.605f, .04f, .975f, .48f),
                    floatArrayOf(.605f, .52f, .975f, .96f),
                )
                for ((i, t) in tiles.withIndex()) {
                    val l = r.left + t[0] * w
                    val tp = r.top + t[1] * h
                    val rt = r.left + t[2] * w
                    val b = r.top + t[3] * h
                    if (i == 0) {
                        // The focused one glows in Arch blue.
                        round(canvas, l - 1.5f * cw, tp - 1.5f * cw, rt + 1.5f * cw, b + 1.5f * cw, 2.8f * cw, rgba(23, 147, 209, .12f))
                    }
                    round(canvas, l, tp, rt, b, 1.3f * cw, rgba(6, 12, 20, .58f))
                    // Lines of text.
                    canvas.save()
                    canvas.clipRect(l + .05f * (rt - l), tp + .08f * (b - tp), l + .75f * (rt - l), tp + .68f * (b - tp))
                    fill.color = white(.07f)
                    var y = tp + .08f * (b - tp) + 3.2f * ch
                    while (y < b) {
                        canvas.drawRect(l, y, rt, y + 1.2f * ch, fill)
                        y += 4.4f * ch
                    }
                    canvas.restore()
                    if (i == 0) outline(canvas, l, tp, rt, b, 1.3f * cw, c(0xFF1793D1), max(hairline, .3f * cw))
                    else outline(canvas, l, tp, rt, b, 1.3f * cw, white(.16f), max(hairline, .22f * cw))
                }
            }
            Os.UBUNTU, Os.FEDORA, Os.DEBIAN, Os.NIXOS, Os.POP -> {
                fill.color = rgba(0, 0, 0, .62f)
                canvas.drawRect(r.left, r.top, r.right, r.top + 4.4f * ch, fill)
                if (os == Os.UBUNTU) {
                    // The dock down the left side.
                    val l = r.left + .012f * w
                    val t = r.top + .20f * h
                    val b = r.bottom - .20f * h
                    val dw = 5.4f * cw
                    round(canvas, l, t, l + dw, b, 1.4f * cw, rgba(0, 0, 0, .4f))
                    val bw = .64f * dw
                    val bh = .15f * (b - t)
                    fill.color = white(.7f)
                    var y = t + .04f * ((b - t) - bh)
                    while (y + bh <= b + 0.5f) {
                        canvas.drawCircle(l + dw / 2, y + bh / 2, .6f * min(bw, bh) / 2, fill)
                        y += bh
                    }
                } else {
                    val l = r.left + .22f * w
                    val t = r.top + .18f * h
                    window(canvas, l, t, l + .56f * w, t + .58f * h, 1.8f * cw, rgba(20, 22, 30, .42f), white(.08f), .11f, white(.12f), max(hairline, .2f * cw))
                }
            }
            Os.LINUX, Os.MINT, Os.MANJARO -> {
                // A bottom panel and its launcher.
                fill.color = rgba(8, 12, 18, .66f)
                canvas.drawRect(r.left, r.bottom - 6.4f * ch, r.right, r.bottom, fill)
                fill.color = white(.7f)
                canvas.drawCircle(r.left + .014f * w + 2 * ch, r.bottom - 1.2f * ch - 2 * ch, 2 * ch, fill)
            }
            Os.WINDOWS -> {
                fill.color = rgba(16, 22, 44, .6f)
                canvas.drawRect(r.left, r.bottom - 7.2f * ch, r.right, r.bottom, fill)
                val l = r.left + .41f * w
                val iw = .18f * w
                val ih = 4.4f * ch
                val cy = r.bottom - 1.4f * ch - ih / 2
                fill.color = white(.8f)
                for (i in 0 until 5) canvas.drawCircle(l + (i + .5f) * iw / 5, cy, .58f * min(iw / 5, ih) / 2, fill)
            }
            Os.UNKNOWN -> {}
        }
        canvas.restore()
    }
}
