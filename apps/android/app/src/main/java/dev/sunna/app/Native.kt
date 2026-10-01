package dev.sunna.app

import android.view.Surface

/** Sunna's Rust side (crates/android): checking hosts and running sessions. */
object Native {
    init {
        System.loadLibrary("sunna_android")
    }

    /** Once, before anything else: where this phone's Sunna state lives. */
    @JvmStatic external fun init(filesDir: String)

    /** Is a host there and does the key fit? JSON (`reach::Check`); blocks. */
    @JvmStatic external fun check(address: String, key: String): String

    /** Several at once: a JSON array, in order; blocks. */
    @JvmStatic external fun checkAll(addresses: Array<String>, keys: Array<String>): String

    /** JSON {"host", "port"}, or {"error"} in words. */
    @JvmStatic external fun parseAddress(address: String): String

    /** Start a session; it connects in the background. Returns its handle. */
    @JvmStatic external fun connect(address: String, key: String, name: String, hostOs: String, codec: String, maxWidth: Int, maxHeight: Int, audio: Boolean): Long

    /** The surface the decoder draws sessions on; null when it goes away. */
    @JvmStatic external fun setSurface(surface: Surface?)

    /** Moves on whenever the status changes. */
    @JvmStatic external fun changes(handle: Long): Long

    /** JSON: phase, why it ended, the stream and its numbers. */
    @JvmStatic external fun status(handle: Long): String

    /** The host's pointer shape: a 20-byte header, then premultiplied RGBA. */
    @JvmStatic external fun cursor(handle: Long): ByteArray?

    /** The pointer at (x, y), from 0 to 1 across the host's screen. */
    @JvmStatic external fun pointer(handle: Long, x: Float, y: Float)

    /** 0 left, 1 right, 2 middle, 3 back, 4 forward. */
    @JvmStatic external fun button(handle: Long, button: Int, pressed: Boolean)

    /** Content follows by (dx, dy). Phase -1 a wheel, 0 begins, 1 moves, 2 ends. */
    @JvmStatic external fun scroll(handle: Long, dx: Float, dy: Float, phase: Int)

    /** A hardware keyboard's key; false if the host has no such key. */
    @JvmStatic external fun key(handle: Long, keyCode: Int, pressed: Boolean, repeat: Boolean): Boolean

    /** A key by its Mac keycode. */
    @JvmStatic external fun macKey(handle: Long, code: Int, pressed: Boolean)

    /** Type text key by key; returns how many characters had no key. */
    @JvmStatic external fun type(handle: Long, text: String): Int

    @JvmStatic external fun shortcuts(hostOs: String): Array<String>

    @JvmStatic external fun shortcut(handle: Long, index: Int)

    @JvmStatic external fun setStream(handle: Long, codec: String, maxWidth: Int, maxHeight: Int, bitrateMbps: Int, fps: Int, audio: Boolean)

    @JvmStatic external fun leave(handle: Long)

    @JvmStatic external fun close(handle: Long)
}
