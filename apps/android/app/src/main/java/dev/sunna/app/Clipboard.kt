package dev.sunna.app

import android.content.ClipData
import android.content.ClipboardManager
import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.database.Cursor
import android.database.MatrixCursor
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import java.io.ByteArrayOutputStream
import java.io.File
import kotlin.math.max

/**
 * The phone's clipboard, shared with a session: what you copy here goes to
 * the host, what you copy there lands here. Android only lets the app in
 * front read the clipboard, so a copy made in another app is picked up when
 * you come back to Sunna; only copies made after the session starts are sent.
 */
class PhoneClipboard(private val context: Context, private val app: App) {
    private val manager = context.getSystemService(ClipboardManager::class.java)
    /** When the clip we last saw (or put there ourselves) was made. */
    private var seen = 0L
    private var delivered = 0L
    /** Moves on with every copy here and when sharing stops: an image still
     *  being read or written for an older clip is dropped. */
    private var generation = 0
    private val main = android.os.Handler(android.os.Looper.getMainLooper())
    var sharing = false
        private set
    private val listener = ClipboardManager.OnPrimaryClipChangedListener { pickUp() }

    fun start() {
        if (sharing) return
        sharing = true
        seen = manager?.primaryClipDescription?.timestamp ?: 0L
        delivered = Native.clipboardRemoteCount()
        manager?.addPrimaryClipChangedListener(listener)
    }

    fun stop() {
        if (!sharing) return
        sharing = false
        generation++
        manager?.removePrimaryClipChangedListener(listener)
    }

    /** Something new on the phone's clipboard: send it (only new clips are read,
     *  so Android's "pasted from your clipboard" note shows only for those). */
    fun pickUp() {
        if (!sharing) return
        val description = manager?.primaryClipDescription ?: return
        if (description.timestamp == seen) return
        seen = description.timestamp
        val copy = ++generation
        val item = runCatching { manager.primaryClip?.getItemAt(0) }.getOrNull() ?: return
        val uri = item.uri
        if (uri != null && description.hasMimeType("image/*")) {
            app.background {
                val png = png(uri) ?: return@background
                main.post { if (sharing && generation == copy) Native.clipboardCopied(1, png) }
            }
            return
        }
        val text = item.text?.toString() ?: runCatching { item.coerceToText(context)?.toString() }.getOrNull()
        if (!text.isNullOrEmpty()) Native.clipboardCopied(0, text.toByteArray())
    }

    /** Once a frame: what the host copied, onto the phone's clipboard. */
    fun deliver() {
        if (!sharing) return
        val count = Native.clipboardRemoteCount()
        if (count == delivered) return
        delivered = count
        val bytes = Native.clipboardTake() ?: return
        if (bytes.isEmpty()) return
        when (bytes[0].toInt()) {
            0 -> put(ClipData.newPlainText("Sunna", String(bytes, 1, bytes.size - 1, Charsets.UTF_8)))
            1 -> {
                // Written to a file first, away from drawing; unless something
                // newer was copied here or there meanwhile.
                val copy = generation
                app.background {
                    val uri = runCatching { ClipboardFiles.store(context, bytes, 1) }.getOrNull() ?: return@background
                    main.post {
                        if (sharing && generation == copy && delivered == count) {
                            runCatching { ClipData.newUri(context.contentResolver, "Image", uri) }.getOrNull()?.let(::put)
                        }
                    }
                }
            }
        }
    }

    /** Onto the phone's clipboard; ours, so not a copy to send back. */
    private fun put(clip: ClipData) {
        runCatching { manager?.setPrimaryClip(clip) }
        seen = manager?.primaryClipDescription?.timestamp ?: seen
    }

    /** The clipboard's text, for typing it out. */
    fun text(): String? = runCatching { manager?.primaryClip?.getItemAt(0)?.coerceToText(context)?.toString() }.getOrNull()

    /** An image on the clipboard, as PNG (huge ones scaled down to 4096 px). */
    private fun png(uri: Uri): ByteArray? = runCatching {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) }
        var sample = 1
        while (max(bounds.outWidth, bounds.outHeight) / sample > 4096) sample *= 2
        val bitmap = context.contentResolver.openInputStream(uri)?.use {
            BitmapFactory.decodeStream(it, null, BitmapFactory.Options().apply { inSampleSize = sample })
        } ?: return null
        ByteArrayOutputStream().use { out ->
            bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)
            out.toByteArray().takeIf { it.size <= 16 * 1024 * 1024 }
        }
    }.getOrNull()
}

/** Hands the host's copied images to apps that paste them: the clipboard
 *  itself only holds a link to one. */
class ClipboardFiles : ContentProvider() {
    override fun onCreate() = true

    override fun getType(uri: Uri) = "image/png"

    private fun file(uri: Uri): File? {
        val name = uri.lastPathSegment ?: return null
        if (!name.matches(Regex("""clip-\d+\.png"""))) return null
        return File(context?.cacheDir ?: return null, "clipboard/$name").takeIf { it.exists() }
    }

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        val file = file(uri) ?: throw java.io.FileNotFoundException(uri.toString())
        return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
    }

    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, selectionArgs: Array<out String>?, sortOrder: String?): Cursor? {
        val file = file(uri) ?: return null
        return MatrixCursor(arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)).apply { addRow(arrayOf<Any>(file.name, file.length())) }
    }

    override fun insert(uri: Uri, values: ContentValues?): Uri? = null
    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?) = 0
    override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?) = 0

    companion object {
        private const val AUTHORITY = "dev.sunna.app.clipboard"

        /** Keep an image for the clipboard (the last few only) and name it. */
        fun store(context: Context, bytes: ByteArray, offset: Int): Uri {
            val dir = File(context.cacheDir, "clipboard").apply { mkdirs() }
            dir.listFiles()?.sortedBy { it.lastModified() }?.dropLast(2)?.forEach { it.delete() }
            val file = File(dir, "clip-${System.currentTimeMillis()}.png")
            file.outputStream().use { it.write(bytes, offset, bytes.size - offset) }
            return Uri.parse("content://$AUTHORITY/${file.name}")
        }
    }
}
