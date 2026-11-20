package dev.sunna.app

import android.app.Activity
import android.content.Intent
import android.content.pm.PackageInstaller

class InstallResult : Activity() {
    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        super.onCreate(savedInstanceState)
        if (intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE) == PackageInstaller.STATUS_PENDING_USER_ACTION) {
            @Suppress("DEPRECATION")
            intent.getParcelableExtra<Intent>(Intent.EXTRA_INTENT)?.let { startActivity(it) }
        } else if (intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE) != PackageInstaller.STATUS_SUCCESS) {
            android.widget.Toast.makeText(this, intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE) ?: "Installation cancelled", android.widget.Toast.LENGTH_LONG).show()
        }
        finish()
    }
}
