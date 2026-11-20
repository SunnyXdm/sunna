//! Desktop update commands consumed by ui/updates.js:
//! `update_status()` -> Status {last_check, available, pending, current,
//! previous, failed, starts, healthy, policy, version, managed, message,
//! phase: "idle"|"checking"|"downloading", error, latest_version, policy_locked}.
//! Missing versions are null; phase/error describe this process’s last check.
//! `update_check()` -> Status; checks the signed feed and stages a newer build.
//! `update_policy(policy: "auto"|"notify"|"off")` -> ().
//! `update_restart()` -> (); refuses while a viewer is open, switches, relaunches.
//! `host_update(address, key, action: "status"|"update"|"auto"|"notify"|"off")`
//! -> host JSON {version, state, policy, supported}; errors are displayable strings.
//! Host probes also expose optional `reach::Check.update` for the Computers list.
//! Keys are never logged. The host accepts no viewer-supplied URL or artifact.
use std::sync::Mutex;
use sunna_update::{State, Updater};
use tauri::Manager;

const UNMANAGED: &str = "This copy cannot update itself. Install a signed download first. For a system package, use apt, dnf or pacman.";
struct Progress {
    phase: &'static str,
    error: Option<String>,
    latest: Option<String>,
}
static PROGRESS: Mutex<Progress> = Mutex::new(Progress {
    phase: "idle",
    error: None,
    latest: None,
});
#[derive(serde::Serialize)]
pub struct Status {
    #[serde(flatten)]
    state: State,
    version: &'static str,
    managed: bool,
    packaged: bool,
    message: Option<&'static str>,
    phase: &'static str,
    error: Option<String>,
    latest_version: Option<String>,
    policy_locked: bool,
}
fn check_and_prepare() -> Result<(), String> {
    let u = updater()?;
    if !u.is_managed_process() {
        return Err(UNMANAGED.into());
    }
    {
        let mut p = PROGRESS.lock().map_err(|e| e.to_string())?;
        if p.phase != "idle" {
            return Err("An update check is already running.".into());
        }
        p.phase = "checking";
        p.error = None;
    }
    let result = (|| {
        let manifest = u.check().map_err(|e| e.to_string())?;
        {
            let mut p = PROGRESS.lock().map_err(|e| e.to_string())?;
            p.latest = Some(manifest.version);
        }
        // An equal or already staged release needs no download.
        let state = u.state().map_err(|e| e.to_string())?;
        if state.available.is_some() && state.pending != state.available {
            PROGRESS.lock().map_err(|e| e.to_string())?.phase = "downloading";
            u.prepare().map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    let mut p = PROGRESS.lock().map_err(|e| e.to_string())?;
    p.phase = "idle";
    p.error = result.as_ref().err().cloned();
    result
}
fn updater() -> Result<Updater, String> {
    Updater::new("app").map_err(|e| e.to_string())
}
#[tauri::command]
pub fn update_status() -> Result<Status, String> {
    let u = updater()?;
    let mut state = u.state().map_err(|e| e.to_string())?;
    state.policy = u.policy().map_err(|e| e.to_string())?;
    let managed = u.is_managed_process();
    let packaged = !managed
        && cfg!(target_os = "linux")
        && std::env::current_exe().is_ok_and(|path| path.starts_with("/usr"));
    let p = PROGRESS.lock().map_err(|e| e.to_string())?;
    Ok(Status {
        state,
        version: env!("CARGO_PKG_VERSION"),
        managed,
        packaged,
        message: if managed {
            None
        } else if packaged {
            Some("Sunna was installed as a system package. New versions arrive with your system's software updates.")
        } else {
            Some(UNMANAGED)
        },
        phase: p.phase,
        error: p.error.clone(),
        latest_version: p.latest.clone(),
        policy_locked: std::env::var_os("SUNNA_UPDATE").is_some(),
    })
}
#[tauri::command]
pub async fn update_check() -> Result<Status, String> {
    tauri::async_runtime::spawn_blocking(|| {
        check_and_prepare()?;
        update_status()
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn update_policy(policy: String) -> Result<(), String> {
    updater()?.set_policy(&policy).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn update_restart(app: tauri::AppHandle) -> Result<(), String> {
    let session = app.state::<crate::Session>();
    let viewer = session.viewer.lock().map_err(|e| e.to_string())?;
    if viewer.is_some() {
        return Err("Disconnect the viewer before restarting Sunna.".into());
    }
    let u = updater()?;
    if !u.is_managed_process() {
        return Err(
            "Use the package manager for this installation, or install a signed download first."
                .into(),
        );
    }
    #[cfg(not(target_os = "macos"))]
    let _idle = u.lock("session.lock", false).map_err(|e| e.to_string())?;
    if std::env::var_os("SUNNA_SUPERVISED").is_none() {
        return Err("Quit and relaunch Sunna through the installed launcher.".into());
    }
    #[cfg(not(target_os = "macos"))]
    if let Err(error) = u.activate() {
        if !u.is_managed_process() { app.exit(75); }
        return Err(error.to_string());
    }
    app.exit(75);
    Ok(())
}
#[tauri::command]
pub async fn host_update(
    record: Option<String>,
    address: String,
    key: String,
    action: String,
) -> Result<serde_json::Value, String> {
    let addr = sunna_client::reach::resolve(&address)
        .await
        .map_err(|e| e.detail)?;
    let trust = record.as_deref().and_then(crate::machines::get)
        .filter(|m| m.address == address && m.key == key).map(|m| m.trust()).unwrap_or_default();
    let reply = sunna_client::host_update_with(addr, &key, &action, &trust)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&reply).map_err(|e| e.to_string())
}
pub fn startup() {
    let Ok(u) = updater() else { return };
    std::thread::spawn(move || {
        #[cfg(target_os = "macos")]
        if let Err(error) = crate::mac_host::agent::reconcile(std::env::var_os("SUNNA_READY_FILE").is_some(), true) {
            eprintln!("Background host startup: {error:#}");
            if std::env::var_os("SUNNA_READY_FILE").is_some() { std::process::exit(1); }
        }
        if !u.is_managed_process() { return; }
        // Called once the Tauri setup hook has created the application.
        if u.signal_ready().is_err() {
            return;
        }
        let mut clock = None;
        loop {
            if let Ok(policy) = u.policy() {
                if policy != "off"
                    && u.state()
                        .is_ok_and(|s| u.due_in_process(s.last_check, &mut clock))
                {
                    let _ = check_and_prepare();
                    if let Ok(state) = u.state() {
                        clock = Some((state.last_check, std::time::Instant::now()));
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });
}

pub fn publication_probe() {
    if let Some(result) = sunna_update::publication_probe() {
        match result {
            Ok(()) => std::process::exit(0),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
}
