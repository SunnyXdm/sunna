package dev.sunna.app

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.view.View
import android.widget.FrameLayout
import android.window.OnBackInvokedCallback
import android.window.OnBackInvokedDispatcher
import java.util.concurrent.Executors

/** Sunna: your computers, and a session with one of them. */
class MainActivity : Activity(), App {
    override lateinit var store: MachineStore
    override lateinit var prefs: Prefs
    override val checks: MutableMap<String, Check> = HashMap()
    override lateinit var notice: Notice
    private lateinit var root: FrameLayout
    private lateinit var home: HomeView
    private var session: SessionView? = null
    private var settings: SettingsView? = null
    private val io = Executors.newCachedThreadPool()
    private var insets = Insets()
    private var backCallback: Any? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Native.init(filesDir.absolutePath)
        Palette.load(this)
        store = MachineStore.get(this)
        prefs = Prefs(this)
        if (Build.VERSION.SDK_INT >= 30) {
            // Edge to edge (the default from Android 15).
            @Suppress("DEPRECATION")
            window.setDecorFitsSystemWindows(false)
        } else {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_LAYOUT_STABLE or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
        }
        root = FrameLayout(this)
        root.setBackgroundColor(Palette.BG)
        home = HomeView(this, this)
        root.addView(home, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        notice = Notice(this)
        notice.elevation = dp(30f)
        root.addView(notice, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        setContentView(root)
        root.setOnApplyWindowInsetsListener { _, windowInsets ->
            insets = Insets.of(windowInsets)
            home.setInsets(insets.left, insets.top, insets.right, insets.bottom)
            for (i in 0 until root.childCount) (root.getChildAt(i) as? Sheet)?.setInsets(insets)
            settings?.setInsets(insets)
            session?.dispatchApplyWindowInsets(windowInsets)
            windowInsets
        }
        Sheet.changed = { updateBack() }
        // Learn what this phone decodes now, not when the first session starts.
        background { Decoders.hardware("video/hevc") }
        home.render(animate = false)
        open(intent)
        updateBack()
    }

    override fun onResume() {
        super.onResume()
        if (session == null) home.resume() else session?.resume()
    }

    override fun onPause() {
        super.onPause()
        home.pause()
        session?.pause()
    }

    override fun onDestroy() {
        session?.destroy()
        SessionService.stop(this)
        Sheet.changed = null
        SessionService.onDisconnect = null
        io.shutdown()
        super.onDestroy()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) session?.focused() else session?.releaseAll()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        open(intent)
    }

    /** A `sunna://` link opened on this phone: add that computer. */
    private fun open(intent: Intent?) {
        val link = intent?.data?.takeIf { intent.action == Intent.ACTION_VIEW && it.scheme == "sunna" } ?: return
        intent.data = null
        if (session != null) {
            notice.show("Leave the session to add a computer.")
            return
        }
        root.post { editMachine(null, link = link.toString()) }
    }

    /** Once, at the first session: the notification that keeps a session
     *  reachable while you're in another app needs permission (Android 13+). */
    override fun askForNotifications() {
        if (Build.VERSION.SDK_INT < 33) return
        if (checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) == android.content.pm.PackageManager.PERMISSION_GRANTED) return
        val asked = getSharedPreferences("sunna", 0)
        if (asked.getBoolean("asked_notifications", false)) return
        asked.edit().putBoolean("asked_notifications", true).apply()
        requestPermissions(arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 1)
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        // The session's notification was posted before it was allowed: again.
        val granted = grantResults.isNotEmpty() && grantResults[0] == android.content.pm.PackageManager.PERMISSION_GRANTED
        if (granted) session?.let { SessionService.start(this, it.machine.name) }
    }

    override fun background(work: () -> Unit) {
        if (!io.isShutdown) io.execute(work)
    }

    override fun connect(machine: Machine, from: ScreenView) {
        if (session != null) return
        notice.dismiss()
        val check = checks[machine.id] ?: Check("ready")
        val view = SessionView(this, this, machine, check, { home.tileFor(machine.id)?.screen },
            onClosing = { home.animate().alpha(1f).setDuration(350).start() },
        ) { closed, reason -> ended(closed, reason) }
        session = view
        root.addView(view, root.indexOfChild(notice), FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        SessionService.onDisconnect = { session?.leave() }
        SessionService.start(this, machine.name)
        home.pause()
        home.animate().alpha(0f).setDuration(450).start()
        view.post { view.start() }
        updateBack()
    }

    private fun ended(view: SessionView, reason: String?) {
        root.removeView(view)
        if (session === view) session = null
        SessionService.stop(this)
        home.alpha = 1f
        home.resume()
        if (!reason.isNullOrEmpty()) notice.show(reason)
        updateBack()
    }

    override fun editMachine(machine: Machine?, focusKey: Boolean, link: String?) {
        val sheet = MachineSheet(this, root, this, machine, focusKey, link) { _, added ->
            home.render()
            if (added) home.scrollToBottom()
            home.poll()
        }
        sheet.open()
        sheet.setInsets(insets)
    }

    override fun machineMenu(machine: Machine, anchor: View) {
        val items = ArrayList<MenuPopup.Item>()
        if (checks[machine.id]?.state == "ready") {
            items += MenuPopup.Item("Connect", R.drawable.ic_arrow) { home.tileFor(machine.id)?.let { connect(machine, it.screen) } }
        }
        items += MenuPopup.Item("Edit…", R.drawable.ic_edit) { editMachine(machine) }
        items += MenuPopup.Item("Copy Address", R.drawable.ic_copy) {
            getSystemService(ClipboardManager::class.java)?.setPrimaryClip(ClipData.newPlainText("Address", machine.address))
            notice.show("Copied ${machine.address}")
        }
        items += MenuPopup.Item("Remove…", R.drawable.ic_trash, danger = true) {
            confirm(this, root, "Remove “${machine.name}”?", "You can add it again with its address and key.", "Remove") {
                if (!store.remove(machine.id)) {
                    notice.show("Couldn't remove it: the phone's storage may be full.")
                    return@confirm
                }
                checks.remove(machine.id)
                home.render()
            }.setInsets(insets)
        }
        MenuPopup(this).show(anchor, items)
    }

    // ---- Back ------------------------------------------------------------------------------

    override fun openSettings() {
        if (settings != null || session != null) return
        settings = SettingsView(this, this) {
            settings = null
            updateBack()
        }.also { it.open(root) }
        notice.bringToFront()
        updateBack()
    }

    private fun topSheet(): Sheet? = (root.childCount - 1 downTo 0).map { root.getChildAt(it) }.firstOrNull { it is Sheet && it.isOpen } as? Sheet

    /** Back closes the sheet on top; in a session it opens the menu. */
    private fun handleBack(): Boolean {
        topSheet()?.let {
            it.close()
            return true
        }
        settings?.let {
            it.close()
            return true
        }
        session?.let { return it.back() }
        return false
    }

    private fun updateBack() {
        if (Build.VERSION.SDK_INT < 33) return
        val wanted = topSheet() != null || session != null || settings != null
        val dispatcher = onBackInvokedDispatcher
        if (wanted && backCallback == null) {
            val callback = OnBackInvokedCallback { handleBack() }
            dispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT, callback)
            backCallback = callback
        } else if (!wanted && backCallback != null) {
            dispatcher.unregisterOnBackInvokedCallback(backCallback as OnBackInvokedCallback)
            backCallback = null
        }
    }

    @Deprecated("Android 13 and later use the back callback")
    override fun onBackPressed() {
        if (!handleBack()) {
            @Suppress("DEPRECATION")
            super.onBackPressed()
        }
    }
}
