//! Local host controls. Links contain credentials and must never be logged.
use serde::{Deserialize, Serialize};
use tauri::Emitter;

#[cfg(target_os = "linux")]
#[path = "sharing/linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "sharing/macos.rs"]
mod platform;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[path = "sharing/unsupported.rs"]
mod platform;

#[derive(Clone, Serialize, Deserialize)]
pub struct Permission {
    pub id: String,
    pub granted: bool,
    pub label: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Fix {
    pub label: String,
    pub command: Option<String>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Sharing {
    pub supported: bool,
    pub state: String,
    pub managed_by: Option<String>,
    pub mode: Option<String>,
    pub reach: Option<String>,
    pub address: Option<String>,
    pub link: Option<String>,
    pub viewer: Option<String>,
    pub permissions: Vec<Permission>,
    pub reason: Option<String>,
    pub fix: Option<Fix>,
    pub starts_by_itself: bool,
    pub shows_login_screen: Option<bool>,
}

static OPERATION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static PROGRESS: std::sync::Mutex<Option<Sharing>> = std::sync::Mutex::new(None);

#[tauri::command]
pub async fn sharing_status() -> Sharing {
    if let Some(snapshot) = PROGRESS.lock().unwrap().clone() {
        return snapshot;
    }
    platform::status().await
}

#[tauri::command]
pub async fn sharing_set(app: tauri::AppHandle, on: bool) -> Sharing {
    let Ok(_guard) = OPERATION.try_lock() else { return sharing_status().await };
    let progress = |snapshot: Sharing| {
        *PROGRESS.lock().unwrap() = Some(snapshot.clone());
        let _ = app.emit("sharing-changed", snapshot);
    };
    let result = platform::set(on, &progress).await;
    *PROGRESS.lock().unwrap() = None;
    let _ = app.emit("sharing-changed", &result);
    result
}

#[tauri::command]
pub fn sharing_open_permission(which: String) -> Result<(), String> {
    platform::open_permission(&which)
}
