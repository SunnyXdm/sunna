package dev.sunna.app

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.MotionEvent
import android.view.VelocityTracker
import android.view.View
import android.view.ViewConfiguration
import kotlin.math.abs
import kotlin.math.exp
import kotlin.math.hypot

/**
 * Fingers on the picture, into what the host should do. Two ways to work:
 *
 * Trackpad (the screen is a laptop's trackpad): one finger moves the
 * pointer, a tap clicks, two fingers tap for a right click or drag to
 * scroll, touch and hold then move to drag.
 *
 * Touch (tap where you want to click): a tap clicks there, touch and hold
 * right-clicks, one finger drags, two fingers scroll.
 *
 * In both, pinching zooms the picture here (the host's screen doesn't
 * change), and a three-finger tap brings up the keyboard.
 */
class Touch(view: View, private val remote: Remote) {
    interface Remote {
        val trackpad: Boolean
        /** Trackpad: move the pointer by a finger's movement on the glass. */
        fun moveBy(dx: Float, dy: Float, speed: Float)
        /** Touch: put the pointer under the finger (view coordinates). */
        fun moveTo(x: Float, y: Float)
        fun button(button: Int, down: Boolean)
        fun click(button: Int)
        /** Content follows the fingers by (dx, dy) view pixels. Phase: 0 begins, 1 moves, 2 ends. */
        fun scroll(dx: Float, dy: Float, phase: Int)
        fun pinch(factor: Float, fx: Float, fy: Float, dx: Float, dy: Float)
        fun pinchEnded()
        fun keyboard()
        fun held()
    }

    private enum class State { IDLE, PENDING, MOVING, DRAGGING, TWO, SCROLLING, PINCHING, THREE, DONE }

    private val slop = ViewConfiguration.get(view.context).scaledTouchSlop.toFloat()
    private val pinchSlop = view.dp(22f)
    private val handler = Handler(Looper.getMainLooper())
    private var state = State.IDLE
    private var downAt = 0L
    private var downX = 0f
    private var downY = 0f
    private var lastX = 0f
    private var lastY = 0f
    private var lastMoveAt = 0L
    private var startDistance = 0f
    private var lastDistance = 0f
    private var startMidX = 0f
    private var startMidY = 0f
    private var lastMidX = 0f
    private var lastMidY = 0f
    private var tracker: VelocityTracker? = null
    private var buttonHeld = false
    private var momentum: Momentum? = null

    private val hold = Runnable {
        if (state != State.PENDING) return@Runnable
        remote.held()
        if (remote.trackpad) {
            // Hold, then move: a drag.
            remote.button(0, true)
            buttonHeld = true
            state = State.DRAGGING
        } else {
            remote.moveTo(downX, downY)
            remote.click(1)
            state = State.DONE
        }
    }

    fun onTouchEvent(event: MotionEvent): Boolean {
        if (tracker == null) tracker = VelocityTracker.obtain()
        tracker?.addMovement(event)
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                stopMomentum()
                state = State.PENDING
                downAt = SystemClock.uptimeMillis()
                downX = event.x
                downY = event.y
                lastX = event.x
                lastY = event.y
                lastMoveAt = event.eventTime
                handler.postDelayed(hold, ViewConfiguration.getLongPressTimeout().toLong().coerceAtMost(450))
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                handler.removeCallbacks(hold)
                releaseButton()
                when (event.pointerCount) {
                    2 -> if (state == State.PENDING || state == State.MOVING) {
                        state = State.TWO
                        startDistance = distance(event)
                        lastDistance = startDistance
                        startMidX = midX(event)
                        startMidY = midY(event)
                        lastMidX = startMidX
                        lastMidY = startMidY
                    } else if (state != State.SCROLLING && state != State.PINCHING) {
                        state = State.DONE
                    }
                    3 -> state = if (state == State.TWO) State.THREE else State.DONE
                    else -> state = State.DONE
                }
            }
            MotionEvent.ACTION_MOVE -> move(event)
            MotionEvent.ACTION_POINTER_UP -> {
                val quick = SystemClock.uptimeMillis() - downAt < 300
                when (state) {
                    State.TWO -> if (quick) {
                        // Two fingers tapped: a right click.
                        if (!remote.trackpad) remote.moveTo(startMidX, startMidY)
                        remote.click(1)
                    }
                    State.SCROLLING -> {
                        tracker?.computeCurrentVelocity(1000)
                        val vx = tracker?.xVelocity ?: 0f
                        val vy = tracker?.yVelocity ?: 0f
                        // Flicked: the content glides on, slowing down.
                        if (hypot(vx, vy) > slop * 20) startMomentum(vx, vy) else remote.scroll(0f, 0f, 2)
                    }
                    State.PINCHING -> remote.pinchEnded()
                    State.THREE -> if (quick) remote.keyboard()
                    else -> {}
                }
                state = State.DONE
            }
            MotionEvent.ACTION_UP -> {
                handler.removeCallbacks(hold)
                when (state) {
                    State.PENDING -> {
                        // A tap: a click (where the finger is, in touch mode).
                        if (!remote.trackpad) remote.moveTo(event.x, event.y)
                        remote.click(0)
                    }
                    State.DRAGGING -> releaseButton()
                    else -> {}
                }
                state = State.IDLE
                tracker?.recycle()
                tracker = null
            }
            MotionEvent.ACTION_CANCEL -> {
                handler.removeCallbacks(hold)
                stopMomentum()
                releaseButton()
                if (state == State.SCROLLING) remote.scroll(0f, 0f, 2)
                if (state == State.PINCHING) remote.pinchEnded()
                state = State.IDLE
                tracker?.recycle()
                tracker = null
            }
        }
        return true
    }

    private fun move(event: MotionEvent) {
        when (state) {
            State.PENDING -> {
                if (hypot(event.x - downX, event.y - downY) < slop) return
                handler.removeCallbacks(hold)
                if (remote.trackpad) {
                    state = State.MOVING
                } else {
                    // A finger dragged across the picture drags there.
                    remote.moveTo(downX, downY)
                    remote.button(0, true)
                    buttonHeld = true
                    state = State.DRAGGING
                }
                lastX = event.x
                lastY = event.y
                lastMoveAt = event.eventTime
            }
            State.MOVING, State.DRAGGING -> {
                if (remote.trackpad) {
                    val dt = (event.eventTime - lastMoveAt).coerceAtLeast(1)
                    val dx = event.x - lastX
                    val dy = event.y - lastY
                    remote.moveBy(dx, dy, hypot(dx, dy) / dt)
                } else {
                    remote.moveTo(event.x, event.y)
                }
                lastX = event.x
                lastY = event.y
                lastMoveAt = event.eventTime
            }
            State.TWO -> {
                if (event.pointerCount < 2) return
                val spread = abs(distance(event) - startDistance)
                val travel = hypot(midX(event) - startMidX, midY(event) - startMidY)
                if (spread > pinchSlop && spread > travel * 0.6f) {
                    state = State.PINCHING
                    lastDistance = distance(event)
                    lastMidX = midX(event)
                    lastMidY = midY(event)
                } else if (travel > slop) {
                    state = State.SCROLLING
                    if (!remote.trackpad) remote.moveTo(startMidX, startMidY)
                    remote.scroll(midX(event) - startMidX, midY(event) - startMidY, 0)
                    lastMidX = midX(event)
                    lastMidY = midY(event)
                }
            }
            State.SCROLLING -> {
                if (event.pointerCount < 2) return
                val x = midX(event)
                val y = midY(event)
                remote.scroll(x - lastMidX, y - lastMidY, 1)
                lastMidX = x
                lastMidY = y
            }
            State.PINCHING -> {
                if (event.pointerCount < 2) return
                val d = distance(event)
                val x = midX(event)
                val y = midY(event)
                if (lastDistance > 0) remote.pinch(d / lastDistance, x, y, x - lastMidX, y - lastMidY)
                lastDistance = d
                lastMidX = x
                lastMidY = y
            }
            else -> {}
        }
    }

    /** Drop whatever's in progress (focus went elsewhere mid-gesture). */
    fun cancel() {
        handler.removeCallbacks(hold)
        stopMomentum()
        releaseButton()
        if (state == State.PINCHING) remote.pinchEnded()
        if (state == State.SCROLLING) remote.scroll(0f, 0f, 2)
        state = State.IDLE
        tracker?.recycle()
        tracker = null
    }

    private fun releaseButton() {
        if (buttonHeld) {
            remote.button(0, false)
            buttonHeld = false
        }
    }

    private fun distance(event: MotionEvent) = hypot(event.getX(0) - event.getX(1), event.getY(0) - event.getY(1))
    private fun midX(event: MotionEvent) = (event.getX(0) + event.getX(1)) / 2
    private fun midY(event: MotionEvent) = (event.getY(0) + event.getY(1)) / 2

    /** The glide after a flick: velocity fading like a thrown page. */
    private inner class Momentum(var vx: Float, var vy: Float) : Runnable {
        private var last = SystemClock.uptimeMillis()

        override fun run() {
            val now = SystemClock.uptimeMillis()
            val dt = (now - last).coerceAtLeast(1) / 1000f
            last = now
            val decay = exp(-dt / 0.33f)
            remote.scroll(vx * dt, vy * dt, 1)
            vx *= decay
            vy *= decay
            if (hypot(vx, vy) < 40f) {
                remote.scroll(0f, 0f, 2)
                momentum = null
            } else {
                handler.postDelayed(this, 16)
            }
        }
    }

    private fun startMomentum(vx: Float, vy: Float) {
        stopMomentum()
        momentum = Momentum(vx, vy).also { handler.post(it) }
    }

    fun stopMomentum() {
        momentum?.let {
            handler.removeCallbacks(it)
            remote.scroll(0f, 0f, 2)
        }
        momentum = null
    }
}
