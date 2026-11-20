package dev.sunna.app

import android.app.Activity
import android.app.PendingIntent
import android.content.pm.PackageInstaller
import android.os.SystemClock
import android.app.AlertDialog
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import org.json.JSONObject
import java.io.File

/** Only Rust verifies/downloads releases; Android owns installation consent. */
object Updates {
    private val busy = java.util.concurrent.atomic.AtomicBoolean(false)
    private var lastElapsed: Long? = null
    fun check(activity: Activity, manual: Boolean = false) {
        val prefs = activity.getSharedPreferences("updates", 0)
        val elapsed = SystemClock.elapsedRealtime()
        val age = lastElapsed?.let { elapsed - it } ?: (System.currentTimeMillis() - prefs.getLong("checked", 0))
        if (!manual && age in 0 until 86_400_000L) return
        if (!busy.compareAndSet(false, true)) return
        lastElapsed = elapsed
        prefs.edit().putLong("checked", System.currentTimeMillis()).apply()
        Thread {
            val result = runCatching { JSONObject(Native.checkUpdate()) }.getOrElse { JSONObject().put("error", it.message) }
            Handler(Looper.getMainLooper()).post {
                busy.set(false)
                if (activity.isFinishing || activity.isDestroyed) return@post
                if (result.has("error")) { if (manual) message(activity, result.optString("error")); return@post }
                val installed = activity.packageManager.getPackageInfo(activity.packageName, 0).versionName ?: "0.0.0"
                val newer = compare(result.getString("version"), installed) > 0
                if (!newer) { if (manual) message(activity, "Sunna is up to date."); return@post }
                AlertDialog.Builder(activity).setTitle("Sunna ${result.getString("version")} is available")
                    .setMessage(result.optString("notes")).setNegativeButton("Later", null)
                    .setPositiveButton("Download") { _, _ -> download(activity, result.getString("version"), result.getString("digest")) }.show()
            }
        }.start()
    }
    internal fun copyApprovedApk(input: java.io.InputStream, output: java.io.OutputStream, digest: String) {
        val hash = java.security.MessageDigest.getInstance("SHA-256")
        java.security.DigestInputStream(input, hash).use { it.copyTo(output) }
        require(hash.digest().joinToString("") { "%02x".format(it) } == digest) { "Downloaded APK changed." }
    }
    internal fun compare(a: String, b: String): Int {
        val left = a.split('.').map { it.toIntOrNull() ?: 0 }
        val right = b.split('.').map { it.toIntOrNull() ?: 0 }
        for (i in 0..2) { val c = (left.getOrElse(i) { 0 }).compareTo(right.getOrElse(i) { 0 }); if (c != 0) return c }
        return 0
    }
    private fun download(activity: Activity, version: String, digest: String) {
        if (!busy.compareAndSet(false, true)) return
        Thread {
            val result = runCatching { JSONObject(Native.downloadUpdate(activity.cacheDir.absolutePath, version, digest)) }.getOrElse { JSONObject().put("error", it.message) }
            Handler(Looper.getMainLooper()).post {
                busy.set(false)
                if (activity.isFinishing || activity.isDestroyed) return@post
                if (result.has("error")) message(activity, result.optString("error")) else install(activity, version, digest)
            }
        }.start()
    }
    fun resume(activity: Activity) {
        val prefs = activity.getSharedPreferences("updates", 0)
        val version = prefs.getString("version", null) ?: return
        val digest = prefs.getString("digest", null) ?: return
        prefs.edit().remove("version").remove("digest").commit()
        if (activity.packageManager.canRequestPackageInstalls()) install(activity, version, digest)
    }
    private fun install(activity: Activity, version: String, digest: String) {
        if (!digest.matches(Regex("[a-f0-9]{64}"))) return
        val apk = File(activity.cacheDir, "$digest.apk")
        val pm = activity.packageManager
        val archive = pm.getPackageArchiveInfo(apk.absolutePath, PackageManager.GET_SIGNING_CERTIFICATES)
        val ours = pm.getPackageInfo(activity.packageName, PackageManager.GET_SIGNING_CERTIFICATES)
        if (archive?.packageName != activity.packageName || archive.versionName != version || archive.longVersionCode <= ours.longVersionCode) {
            message(activity, "This APK is not a newer Sunna release."); return
        }
        val signers = archive.signingInfo?.apkContentsSigners
        if (signers.isNullOrEmpty() || !signers.all { pm.hasSigningCertificate(activity.packageName, it.toByteArray(), PackageManager.CERT_INPUT_RAW_X509) }) {
            message(activity, "This installation uses a different signing key (for example a debug build). It cannot be updated with the release APK. Keep your computers' links before uninstalling it."); return
        }
        if (!pm.canRequestPackageInstalls()) {
            AlertDialog.Builder(activity).setTitle("Allow Sunna to install updates")
                .setMessage("Enable Allow from this source, then return to Sunna. Android will still ask you to confirm each update.")
                .setNegativeButton("Cancel", null).setPositiveButton("Open settings") { _, _ ->
                    activity.getSharedPreferences("updates", 0).edit().putString("version", version).putString("digest", digest).commit()
                    activity.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${activity.packageName}")))
                }.show()
            return
        }
        Thread {
            val installer = pm.packageInstaller
            runCatching {
                val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
                params.setAppPackageName(activity.packageName)
                if (android.os.Build.VERSION.SDK_INT >= 31) params.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_REQUIRED)
                val id = installer.createSession(params)
                try {
                    installer.openSession(id).use { session ->
                        session.openWrite("base.apk", 0, apk.length()).use { out ->
                            copyApprovedApk(apk.inputStream(), out, digest)
                            session.fsync(out)
                        }
                        val receiver = Intent(activity, InstallResult::class.java)
                        val flags = if (android.os.Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0
                        session.commit(PendingIntent.getActivity(activity, id, receiver, flags).intentSender)
                    }
                } catch (e: Exception) {
                    installer.abandonSession(id)
                    throw e
                }
            }.onFailure { e -> activity.runOnUiThread { if (!activity.isDestroyed) message(activity, e.message ?: "Installation failed") } }
        }.start()
    }
    private fun message(activity: Activity, text: String) { AlertDialog.Builder(activity).setTitle("Sunna updates").setMessage(text).setPositiveButton("OK", null).show() }
}
