package dev.sunna.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.drawable.Icon
import android.os.Build
import android.os.IBinder

/**
 * Keeps a session connected while Sunna is in the background: Android freezes
 * apps there, which would drop the connection a few seconds after you switch
 * to another app (to copy a password, say). Its notification takes you back,
 * or disconnects.
 */
class SessionService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == DISCONNECT) {
            onDisconnect?.invoke()
            return START_NOT_STICKY
        }
        val notification = notification(intent?.getStringExtra(NAME) ?: "a computer")
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        } else {
            startForeground(ID, notification)
        }
        return START_NOT_STICKY
    }

    private fun notification(name: String): Notification {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(CHANNEL, "Sessions", NotificationManager.IMPORTANCE_LOW).apply {
            description = "While you're connected to a computer"
            setShowBadge(false)
        })
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP), PendingIntent.FLAG_IMMUTABLE)
        val disconnect = PendingIntent.getService(this, 1, Intent(this, SessionService::class.java).setAction(DISCONNECT), PendingIntent.FLAG_IMMUTABLE)
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle("Connected to $name")
            .setContentText("Tap to go back to it")
            .setContentIntent(open)
            .setOngoing(true)
            .setCategory(Notification.CATEGORY_SERVICE)
            .setColor(0xFFFFB15E.toInt())
            .addAction(Notification.Action.Builder(Icon.createWithResource(this, R.drawable.ic_power), "Disconnect", disconnect).build())
            .build()
    }

    companion object {
        private const val ID = 1
        private const val CHANNEL = "sessions"
        private const val NAME = "name"
        private const val DISCONNECT = "dev.sunna.app.DISCONNECT"

        /** What the notification's Disconnect does (the activity sets it). */
        var onDisconnect: (() -> Unit)? = null

        fun start(context: Context, name: String) {
            runCatching { context.startForegroundService(Intent(context, SessionService::class.java).putExtra(NAME, name)) }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, SessionService::class.java))
        }
    }
}
