package dev.sunna.app

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.DashPathEffect
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.Shader
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.text.TextUtils
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import org.json.JSONArray
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/** What the screens need from the app. */
interface App {
    val store: MachineStore
    val prefs: Prefs
    val checks: MutableMap<String, Check>
    val notice: Notice
    fun connect(machine: Machine, from: ScreenView)
    fun editMachine(machine: Machine?, focusKey: Boolean = false, link: String? = null)
    fun machineMenu(machine: Machine, anchor: View)
    fun background(work: () -> Unit)
}

/** A frame `ratio` times as wide as it is tall. */
class AspectFrame(context: Context, private val ratio: Float) : FrameLayout(context) {
    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val width = MeasureSpec.getSize(widthMeasureSpec)
        val height = (width / ratio).roundToInt()
        super.onMeasure(MeasureSpec.makeMeasureSpec(width, MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(height, MeasureSpec.EXACTLY))
    }
}

/** A computer in the grid: its screen, its name, how it is, what it is. */
@SuppressLint("ViewConstructor", "ClickableViewAccessibility")
class TileView(context: Context, onOpen: () -> Unit, onMore: (View) -> Unit) : LinearLayout(context) {
    val screen = ScreenView(context)
    private val frame = AspectFrame(context, 16f / 11f)
    private val name = context.text(15.5f, Palette.TEXT, Type.semibold)
    private val dot = Dot(context)
    private val status = context.text(13.5f, Palette.TEXT_2)
    private val badge = Badge(context)
    private val system = context.text(13f, Palette.TEXT_3)
    private val more = FrameLayout(context)

    init {
        orientation = VERTICAL
        clipChildren = false
        frame.clipChildren = false
        frame.addView(screen, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        more.apply {
            background = ripple(rounded(0xEB16171C.toInt(), dp(16f), white(.14f), dpi(1f)), dp(16f))
            addView(context.icon(R.drawable.ic_more, Color.WHITE, 16f), FrameLayout.LayoutParams(dpi(16f), dpi(16f), Gravity.CENTER))
            contentDescription = "More"
            setOnClickListener { onMore(this) }
            elevation = dp(2f)
        }
        frame.addView(more, FrameLayout.LayoutParams(dpi(32f), dpi(32f), Gravity.TOP or Gravity.END).apply {
            topMargin = dpi(8f)
            rightMargin = dpi(8f)
        })
        addView(frame, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))
        val labels = LinearLayout(context).apply {
            orientation = VERTICAL
            setPadding(dpi(2f), dpi(12f), dpi(2f), 0)
        }
        name.apply {
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
        }
        labels.addView(name)
        val statusRow = LinearLayout(context).apply {
            orientation = HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(0, dpi(5f), 0, 0)
            addView(dot, LayoutParams(dpi(7f), dpi(7f)).apply { marginEnd = dpi(7f) })
            addView(status.apply { maxLines = 1; ellipsize = TextUtils.TruncateAt.END }, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
        }
        labels.addView(statusRow)
        val systemRow = LinearLayout(context).apply {
            orientation = HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(0, dpi(6f), 0, 0)
            addView(badge, LayoutParams(dpi(17f), dpi(17f)).apply { marginEnd = dpi(7f) })
            addView(system.apply { maxLines = 1; ellipsize = TextUtils.TruncateAt.END }, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
        }
        labels.addView(systemRow)
        addView(labels)

        isClickable = true
        isFocusable = true
        setOnClickListener { onOpen() }
        setOnLongClickListener {
            performHapticFeedback(android.view.HapticFeedbackConstants.LONG_PRESS)
            onMore(more)
            true
        }
        setOnTouchListener { _, event ->
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    frame.animate().scaleX(0.975f).scaleY(0.975f).translationY(-dp(2f)).setDuration(140).setInterpolator(Motion.snappy).start()
                    screen.lifted = true
                }
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                    frame.animate().scaleX(1f).scaleY(1f).translationY(0f).setDuration(Motion.BOUNCY).setInterpolator(Motion.bouncy).start()
                    screen.lifted = false
                }
            }
            false
        }
    }

    fun paint(machine: Machine, check: Check?, wakeDelay: Long, animate: Boolean) {
        val state = check?.state ?: "checking"
        val os = check?.os?.ifEmpty { null } ?: machine.os
        screen.os = Os.of(os)
        screen.device = check?.device?.ifEmpty { null } ?: machine.device
        screen.bootName = Os.bootName(os)
        val (w, h) = (check?.width?.takeIf { it > 0 } ?: machine.width) to (check?.height?.takeIf { it > 0 } ?: machine.height)
        screen.ratio = if (w > 0 && h > 0) w.toFloat() / h else 1.6f
        if (state != screen.state) screen.show(state, wakeDelay, animate)
        name.text = machine.name
        name.setTextColor(if (state in setOf("unreachable", "not-found", "invalid")) Palette.TEXT_2 else Palette.TEXT)
        val words = describe(machine, check)
        status.text = words
        dot.color = statusColor(state)
        dot.pulsing = state == "checking"
        badge.os = Os.of(os)
        system.text = systemText(machine, check)
        contentDescription = "${machine.name}. $words"
    }
}

/** The last tile: add a computer. In a single column it's a slim row
 *  rather than a whole empty screen. */
@SuppressLint("ViewConstructor")
class AddTileView(context: Context, onAdd: () -> Unit) : FrameLayout(context) {
    private val full = LinearLayout(context)
    private val slim = LinearLayout(context)
    private val slimFrame = FrameLayout(context)
    private val plus = FrameLayout(context)
    private val slimPlus = FrameLayout(context)

    var compact = false
        set(value) {
            if (field == value) return
            field = value
            full.visibility = if (value) View.GONE else View.VISIBLE
            slimFrame.visibility = if (value) View.VISIBLE else View.GONE
        }

    private class Dashes(context: Context, private val screenOnly: Boolean) : View(context) {
        private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeWidth = dp(1.5f)
            color = white(.14f)
            pathEffect = DashPathEffect(floatArrayOf(dp(6f), dp(5f)), 0f)
        }
        private val box = RectF()

        override fun onDraw(canvas: Canvas) {
            val bottom = if (screenOnly) width * 10f / 16f else height.toFloat()
            box.set(dp(1f), dp(1f), width - dp(1f), bottom - dp(1f))
            canvas.drawRoundRect(box, dp(14f), dp(14f), paint)
        }
    }

    private fun plusIn(holder: FrameLayout, size: Float) = holder.apply {
        background = rounded(white(.06f), dp(size / 2))
        addView(context.icon(R.drawable.ic_plus, Palette.TEXT_2, 20f), LayoutParams(dpi(20f), dpi(20f), Gravity.CENTER))
    }

    init {
        // The full tile: a dashed screen with + in it.
        full.orientation = LinearLayout.VERTICAL
        val frame = AspectFrame(context, 16f / 11f)
        frame.addView(Dashes(context, screenOnly = true), LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        plusIn(plus, 48f)
        val holder = object : FrameLayout(context) {
            override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
                // Centered in the screen area (the top 10/11 of the frame).
                val size = dpi(48f)
                val area = ((right - left) * 10f / 16f).roundToInt()
                val x = (right - left - size) / 2
                val y = (area - size) / 2
                plus.layout(x, y, x + size, y + size)
            }
        }
        holder.addView(plus, LayoutParams(dpi(48f), dpi(48f)))
        frame.addView(holder, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        full.addView(frame, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT))
        full.addView(LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dpi(2f), dpi(12f), dpi(2f), 0)
            addView(context.text(15.5f, Palette.TEXT_2, Type.semibold).apply { text = "Add a Computer" })
            addView(context.text(13.5f, Palette.TEXT_3).apply {
                text = "By address and key"
                setPadding(0, dpi(5f), 0, 0)
            })
        })
        addView(full, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))

        // The slim row.
        slim.apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dpi(16f), 0, dpi(16f), 0)
            minimumHeight = dpi(76f)
            addView(plusIn(slimPlus, 44f), LinearLayout.LayoutParams(dpi(44f), dpi(44f)).apply { marginEnd = dpi(14f) })
            addView(LinearLayout(context).apply {
                orientation = LinearLayout.VERTICAL
                addView(context.text(15.5f, Palette.TEXT_2, Type.semibold).apply { text = "Add a Computer" })
                addView(context.text(13.5f, Palette.TEXT_3).apply {
                    text = "By address and key, or its sunna:// link"
                    setPadding(0, dpi(4f), 0, 0)
                })
            })
        }
        slimFrame.apply {
            addView(Dashes(context, screenOnly = false), LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
            addView(slim, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))
            visibility = View.GONE
        }
        addView(slimFrame, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))

        isClickable = true
        isFocusable = true
        contentDescription = "Add a computer"
        setOnClickListener {
            val turning = if (compact) slimPlus else plus
            turning.animate().rotation(90f).setDuration(Motion.BOUNCY).setInterpolator(Motion.bouncy).withEndAction { turning.rotation = 0f }.start()
            onAdd()
        }
        pressable(0.975f)
    }
}

/** Tiles in columns: fewer computers, bigger screens. */
class TileGrid(context: Context) : ViewGroup(context) {
    private val columnGap = dp(16f)
    private val rowGap = dp(30f)
    private var columns = 1

    init {
        clipChildren = false
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val width = MeasureSpec.getSize(widthMeasureSpec)
        val visible = (0 until childCount).map { getChildAt(it) }.filter { it.visibility != View.GONE }
        val minimum = dp(when {
            visible.size <= 3 -> 290f
            visible.size <= 6 -> 190f
            else -> 150f
        })
        columns = max(1, ((width + columnGap) / (minimum + columnGap)).toInt())
        // A tile wider than this is just big, not better.
        columns = max(columns, ceil((width + columnGap) / (dp(440f) + columnGap)).toInt())
        columns = min(columns, max(1, visible.size))
        for (tile in visible) (tile as? AddTileView)?.compact = columns == 1
        val tileWidth = ((width - columnGap * (columns - 1)) / columns).toInt()
        var height = 0
        visible.chunked(columns).forEachIndexed { row, tiles ->
            var rowHeight = 0
            for (tile in tiles) {
                tile.measure(MeasureSpec.makeMeasureSpec(tileWidth, MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED))
                rowHeight = max(rowHeight, tile.measuredHeight)
            }
            height += rowHeight + if (row > 0) rowGap.toInt() else 0
        }
        setMeasuredDimension(width, height)
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        val visible = (0 until childCount).map { getChildAt(it) }.filter { it.visibility != View.GONE }
        var y = 0
        for (tiles in visible.chunked(columns)) {
            var x = 0f
            var rowHeight = 0
            for (tile in tiles) {
                tile.layout(x.toInt(), y, x.toInt() + tile.measuredWidth, y + tile.measuredHeight)
                x += tile.measuredWidth + columnGap
                rowHeight = max(rowHeight, tile.measuredHeight)
            }
            y += rowHeight + rowGap.toInt()
        }
    }
}

/** A monitor with nothing plugged in: its "No Signal" box drifts and bounces
 *  off the edges, as on the desktop. */
class NoSignal(context: Context) : View(context) {
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    private val shaded = Paint(Paint.ANTI_ALIAS_FLAG)
    private val label = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = Type.semibold
        color = 0xFFD6D7DC.toInt()
    }
    private val box = RectF()
    private val shape = Path()
    private val started = SystemClock.uptimeMillis()

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val width = min(MeasureSpec.getSize(widthMeasureSpec), dpi(260f))
        setMeasuredDimension(width, (width * 10f / 16f + dp(29f)).roundToInt())
    }

    override fun onDraw(canvas: Canvas) {
        val w = width.toFloat()
        val sh = w * 10f / 16f
        box.set(0f, 0f, w, sh)
        fill.color = 0xFF060709.toInt()
        canvas.drawRoundRect(box, dp(12f), dp(12f), fill)
        box.inset(dp(4f), dp(4f))
        fill.color = 0xFF08090C.toInt()
        canvas.drawRoundRect(box, dp(8f), dp(8f), fill)
        shaded.shader = LinearGradient(box.left, box.top, box.left + box.width() * 0.6f, box.bottom, intArrayOf(white(.05f), white(.012f), 0x00FFFFFF), floatArrayOf(0f, 0.34f, 0.35f), Shader.TileMode.CLAMP)
        canvas.drawRoundRect(box, dp(8f), dp(8f), shaded)
        shaded.shader = null
        // The box, bouncing between the edges.
        val cw = box.width() / 100
        val ch = box.height() / 100
        label.textSize = 5 * cw
        val text = "No Signal"
        val boxW = label.measureText(text) + 8 * cw
        val boxH = label.textSize * 1.2f + 4.8f * ch
        val t = (SystemClock.uptimeMillis() - started) / 1000f
        fun bounce(period: Float) = (t % (2 * period)).let { if (it < period) it / period else 2 - it / period }
        val x = box.left + 4 * cw + (box.width() - boxW - 8 * cw) * bounce(6.1f)
        val y = box.top + 5 * ch + (box.height() - boxH - 10 * ch) * bounce(4.3f)
        val tag = RectF(x, y, x + boxW, y + boxH)
        fill.color = 0xFF1D1F25.toInt()
        canvas.drawRoundRect(tag, 1.4f * cw, 1.4f * cw, fill)
        canvas.drawText(text, x + 4 * cw, y + 2.4f * ch + label.textSize * 0.95f, label)
        // The stand.
        val neckW = w * 0.10f
        val neckTop = sh
        shape.reset()
        shape.moveTo(w / 2 - neckW * 0.26f, neckTop)
        shape.lineTo(w / 2 + neckW * 0.26f, neckTop)
        shape.lineTo(w / 2 + neckW * 0.38f, neckTop + dp(22f))
        shape.lineTo(w / 2 - neckW * 0.38f, neckTop + dp(22f))
        shape.close()
        shaded.shader = LinearGradient(w / 2 - neckW / 2, 0f, w / 2 + neckW / 2, 0f, intArrayOf(0xFF202228.toInt(), 0xFF3D414B.toInt(), 0xFF202228.toInt()), null, Shader.TileMode.CLAMP)
        canvas.drawPath(shape, shaded)
        val footW = w * 0.28f
        box.set(w / 2 - footW / 2, neckTop + dp(22f), w / 2 + footW / 2, neckTop + dp(29f))
        shaded.shader = LinearGradient(0f, box.top, 0f, box.bottom, 0xFF4A4E57.toInt(), 0xFF1A1C21.toInt(), Shader.TileMode.CLAMP)
        canvas.drawRoundRect(box, dp(4f), dp(4f), shaded)
        shaded.shader = null
        if (isShown) postInvalidateOnAnimation()
    }
}

/** The Computers screen. */
@SuppressLint("ViewConstructor")
class HomeView(context: Context, private val app: App) : FrameLayout(context) {
    private val scroll = ScrollView(context)
    private val column = LinearLayout(context)
    private val grid = TileGrid(context)
    private val toolbar = LinearLayout(context)
    private val summary = context.text(13.5f, Palette.TEXT_2)
    private val welcome = LinearLayout(context)
    private val tiles = LinkedHashMap<String, TileView>()
    private val addTile = AddTileView(context) { app.editMachine(null) }
    private val handler = Handler(Looper.getMainLooper())
    private var loaded = false
    private var polled = false
    private var polling = false
    private var active = false
    private var topInset = 0

    init {
        setBackgroundColor(Palette.BG)
        column.orientation = LinearLayout.VERTICAL
        // The screens' light reaches past their tiles, as on the desktop.
        column.clipChildren = false
        column.clipToPadding = false
        column.addView(grid, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT))
        scroll.addView(column, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))
        scroll.isVerticalScrollBarEnabled = false
        scroll.clipChildren = false
        scroll.overScrollMode = View.OVER_SCROLL_IF_CONTENT_SCROLLS
        scroll.setOnScrollChangeListener { _, _, y, _, _ -> paintToolbar(y > 0) }
        addView(scroll, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))

        // The toolbar: the title and how many are ready, and +.
        toolbar.apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            val titles = LinearLayout(context).apply {
                orientation = LinearLayout.VERTICAL
                addView(context.text(24f, Palette.TEXT, Type.bold).apply {
                    text = "Computers"
                    letterSpacing = -0.01f
                })
                addView(summary.apply { setPadding(0, dpi(3f), 0, 0) })
            }
            addView(titles, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
            val add = FrameLayout(context).apply {
                background = ripple(null, dp(22f))
                addView(context.icon(R.drawable.ic_plus, Palette.TEXT, 24f), FrameLayout.LayoutParams(dpi(24f), dpi(24f), Gravity.CENTER))
                contentDescription = "Add a computer"
                setOnClickListener { app.editMachine(null) }
            }
            addView(add, LinearLayout.LayoutParams(dpi(44f), dpi(44f)))
        }
        addView(toolbar, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT, Gravity.TOP))

        // No computers yet.
        welcome.apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            setPadding(0, dpi(36f), 0, 0)
            visibility = View.GONE
            addView(NoSignal(context), LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT))
            addView(context.text(20f, Palette.TEXT, Type.bold).apply {
                text = "No computers yet"
                gravity = Gravity.CENTER
                setPadding(0, dpi(30f), 0, dpi(8f))
            })
            addView(context.text(15f, Palette.TEXT_2).apply {
                text = "Add a computer that's sharing with Sunna. Setting up sharing prints a link (on Linux, sunna-host link shows it again): open it on this phone, or paste it here."
                gravity = Gravity.CENTER
                setLineSpacing(0f, 1.3f)
                maxWidth = dpi(380f)
            })
            addView(context.button("Add a Computer…", ButtonStyle.PRIMARY) { app.editMachine(null) },
                LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = dpi(22f) })
        }
        column.addView(welcome, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT))
        paintToolbar(false)
    }

    private fun paintToolbar(scrolled: Boolean) {
        toolbar.background = if (scrolled) rounded(0xF016171B.toInt(), 0f) else null
        toolbar.elevation = if (scrolled) dp(3f) else 0f
    }

    fun setInsets(left: Int, top: Int, right: Int, bottom: Int) {
        topInset = top
        val side = dpi(20f)
        toolbar.setPadding(left + side, top + dpi(10f), right + dpi(10f), dpi(12f))
        toolbar.measure(MeasureSpec.makeMeasureSpec(width.coerceAtLeast(1), MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED))
        column.setPadding(left + side, top + dpi(10f) + dpi(66f), right + side, bottom + dpi(40f))
        app.notice.bottomInset = bottom
    }

    private fun summaryText(): String {
        val machines = app.store.list()
        if (!loaded) return " "
        if (machines.isEmpty()) return "None added yet"
        if (!polled) return "Looking for your computers…"
        val ready = machines.count { app.checks[it.id]?.state == "ready" }
        if (machines.size == 1) return if (ready == 1) "${machines[0].name} is ready" else "${machines[0].name} isn't available"
        if (ready == machines.size) return "All ${machines.size} ready"
        if (ready == 0) return "None of ${machines.size} available"
        return "$ready of ${machines.size} ready"
    }

    /** Draw the machines; `stagger` wakes their screens one after another. */
    fun render(stagger: Boolean = false, animate: Boolean = true) {
        loaded = true
        val machines = app.store.list()
        val ids = machines.map { it.id }.toSet()
        for (id in tiles.keys.toList()) {
            if (id !in ids) grid.removeView(tiles.remove(id))
        }
        machines.forEachIndexed { index, machine ->
            val tile = tiles.getOrPut(machine.id) {
                TileView(context, onOpen = { open(machine.id) }, onMore = { anchor -> app.store.list().firstOrNull { it.id == machine.id }?.let { app.machineMenu(it, anchor) } }).also {
                    if (loaded && polled) {
                        it.alpha = 0f
                        it.translationY = dp(14f)
                        it.animate().alpha(1f).translationY(0f).setDuration(700).setInterpolator(Motion.outExpo).start()
                    }
                }
            }
            tile.paint(machine, app.checks[machine.id], if (stagger) 120L + index * 110L else 0L, animate)
            if (grid.indexOfChild(tile) != index) {
                grid.removeView(tile)
                grid.addView(tile, index)
            }
        }
        if (addTile.parent == null) grid.addView(addTile)
        else if (grid.indexOfChild(addTile) != grid.childCount - 1) {
            grid.removeView(addTile)
            grid.addView(addTile)
        }
        val empty = machines.isEmpty()
        grid.visibility = if (empty) View.GONE else View.VISIBLE
        welcome.visibility = if (empty) View.VISIBLE else View.GONE
        summary.text = summaryText()
    }

    fun tileFor(id: String): TileView? = tiles[id]

    /** A tap on a computer: connect, or say why not. */
    private fun open(id: String) {
        val machine = app.store.list().firstOrNull { it.id == id } ?: return
        val check = app.checks[id]
        val tile = tiles[id] ?: return
        when (check?.state) {
            "ready" -> app.connect(machine, tile.screen)
            "wrong-key" -> app.editMachine(machine, focusKey = true)
            "busy" -> app.notice.show("${check.viewer.ifEmpty { "Someone" }} is connected to ${machine.name} right now.")
            "update-needed" -> app.notice.show(check.detail.ifEmpty { "${machine.name} runs a different version of Sunna." })
            null, "checking" -> app.notice.show("Still looking for ${machine.name}…")
            else -> app.notice.show(check.detail.ifEmpty { "${machine.name} isn't responding." }, "Edit", onAction = { app.editMachine(machine) })
        }
    }

    private val tick = object : Runnable {
        override fun run() {
            poll()
            handler.postDelayed(this, 4000)
        }
    }

    /** Ask every computer how it is (all at once, on the Rust side). */
    fun poll() {
        val machines = app.store.list()
        if (polling || !active || machines.isEmpty()) return
        polling = true
        app.background {
            val result = runCatching {
                JSONArray(Native.checkAll(machines.map { it.address }.toTypedArray(), machines.map { it.key }.toTypedArray()))
            }.getOrNull()
            handler.post {
                polling = false
                if (result == null || !active) return@post
                val first = !polled
                val learned = HashMap<String, Check>()
                // Answers about where a computer was, not where it is now (it
                // was edited meanwhile), are dropped; the next round asks again.
                val now = app.store.list().associateBy { it.id }
                for (i in 0 until min(result.length(), machines.size)) {
                    val asked = machines[i]
                    val current = now[asked.id] ?: continue
                    if (current.address != asked.address || current.key != asked.key) continue
                    learned[asked.id] = Check.from(result.getJSONObject(i))
                }
                app.checks.putAll(learned)
                app.background { app.store.learn(learned) }
                polled = true
                render(stagger = first)
            }
        }
    }

    fun resume() {
        if (active) return
        active = true
        tiles.values.forEach { it.screen.running = true }
        handler.removeCallbacks(tick)
        handler.post(tick)
    }

    fun pause() {
        active = false
        handler.removeCallbacks(tick)
        tiles.values.forEach { it.screen.running = false }
    }

    fun scrollToBottom() {
        scroll.post { scroll.smoothScrollTo(0, column.height) }
    }
}
