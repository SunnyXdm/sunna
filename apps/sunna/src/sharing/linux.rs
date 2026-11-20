use super::{Fix, Permission, Sharing};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use sunna_update::Updater;
use tokio::process::Command;

const UNIT: &str = "sunna-host.service";

fn off() -> Sharing {
    Sharing {
        supported: true,
        state: "off".into(),
        ..Sharing::default()
    }
}

fn failure(mut status: Sharing, reason: &str, action: &str) -> Sharing {
    status.state = "error".into();
    status.reason = Some(reason.into());
    status.fix = Some(Fix {
        label: action.into(),
        command: None,
    });
    status
}

fn config() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".config"))
}

fn from_path(name: &str) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join(name))
        .find(|p| p.is_absolute() && p.is_file())
}

fn managed_helper(root: &Path) -> Option<PathBuf> {
    let current = root.join("current");
    let selected = current.canonicalize().ok()?;
    if selected.parent()? != root.join("versions") {
        return None;
    }
    let helper = current.join("bin/sunna-host");
    (helper.canonicalize().ok()? == selected.join("bin/sunna-host") && helper.is_file())
        .then_some(helper)
}

async fn helper() -> Result<Option<PathBuf>, ()> {
    use std::os::unix::fs::MetadataExt;
    let managed = Updater::new("host")
        .ok()
        .and_then(|u| managed_helper(&u.root));
    let packaged = PathBuf::from("/usr/bin/sunna-host");
    let packaged = fs::symlink_metadata(&packaged)
        .ok()
        .filter(|m| m.is_file() && m.uid() == 0 && m.mode() & 0o022 == 0)
        .map(|_| packaged);
    let mut candidates = managed.into_iter().chain(packaged);
    let unit = unit_definition(true)
        .await
        .or_else(|| fs::read_to_string(config().join("systemd/user").join(UNIT)).ok());
    match unit {
        Some(unit) => candidates
            .find(|p| user_command_matches(&unit, p))
            .map(Some)
            .ok_or(()),
        None => Ok(candidates.next()),
    }
}

fn external() -> Sharing {
    Sharing {
        supported: false,
        managed_by: Some("external".into()),
        reason: Some("Sharing is set up outside Sunna.".into()),
        ..off()
    }
}

fn quoted_unit_path(path: &Path) -> String {
    let escaped: String = path
        .to_string_lossy()
        .chars()
        .map(|c| match c {
            '\\' => "\\\\".into(),
            '"' => "\\\"".into(),
            '%' => "%%".into(),
            c if c.is_ascii_control() => format!("\\x{:02x}", c as u32),
            c => c.to_string(),
        })
        .collect();
    format!("\"{escaped}\"")
}

fn user_command_matches(unit: &str, path: &Path) -> bool {
    let expected = format!("{} run", quoted_unit_path(path));
    let recovery = path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .filter(|_| path.ends_with("current/bin/sunna-host"))
        .map(|root| format!("{} run-user host", quoted_unit_path(&root.join("recovery"))));
    let mut commands = unit
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("Exec"));
    let command = commands.next().map(|line| {
        line.strip_prefix("ExecStart=:/usr/bin/env -- ")
            .or_else(|| line.strip_prefix("ExecStart="))
            .unwrap_or("")
    });
    unit.lines().filter(|l| l.starts_with("# /")).count() <= 1
        && commands.next().is_none()
        && (command == Some(expected.as_str())
            || command.is_some() && command == recovery.as_deref())
}

async fn unit_definition(user: bool) -> Option<String> {
    let mut cmd = Command::new("/usr/bin/systemctl");
    if user {
        cmd.arg("--user");
    }
    let out = cmd.args(["cat", UNIT]).output().await.ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn system_unit_matches(unit: &str) -> bool {
    let mut commands = unit
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("Exec"));
    unit.starts_with("# /etc/systemd/system/sunna-host.service\n")
        && unit.lines().filter(|l| l.starts_with("# /")).count() == 1
        && unit.lines().any(|l| l == "Description=Sunna whole-machine host")
        && matches!(commands.next(),
            Some("ExecStart=/opt/sunna/host/recovery system-serve host --system-host --name %H")
            | Some("ExecStart=/opt/sunna/host/current/bin/sunna-update supervise host --system-host --listen ${SUNNA_LISTEN_ADDR} --name %H"))
        && commands.next().is_none()
}

fn receipt_state<'a>(receipt: &'a str, unit: &str) -> Option<&'a str> {
    let mut fields = receipt.split_whitespace();
    let invocation = fields.next()?;
    let pid = fields.next()?.parse::<u32>().ok().filter(|p| *p > 1)?;
    let state = fields.next()?;
    if fields.next().is_some() || !matches!(state, "frame" | "empty") {
        return None;
    }
    let parent = fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix("PPid:").map(str::trim).map(str::to_owned))?;
    let invocation_matches = unit
        .lines()
        .any(|l| l.strip_prefix("InvocationID=") == Some(invocation));
    let parent_matches = unit
        .lines()
        .any(|l| l.strip_prefix("MainPID=") == Some(parent.as_str()));
    (parent != "0" && parent_matches && invocation_matches).then_some(state)
}

fn system_state(active: bool, listener: Option<std::net::SocketAddr>) -> &'static str {
    if !active {
        "off"
    } else if listener.is_some() {
        "on"
    } else {
        "starting"
    }
}

async fn systemctl(args: &[&str]) -> bool {
    Command::new("/usr/bin/systemctl")
        .args(args)
        .output()
        .await
        .is_ok_and(|o| o.status.success())
}

fn wayland() -> bool {
    env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland")
}

// The portal binds a remembered grant to this desktop file's application ID.
// Package installs already ship it. Signed host archives only contain binaries.
fn register_portal_app() -> std::io::Result<()> {
    use std::io::Write;
    let data = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/share")
        });
    let directory = data.join("applications");
    let entry = directory.join("dev.sunna.Host.desktop");
    if entry.is_file() || Path::new("/usr/share/applications/dev.sunna.Host.desktop").is_file() {
        return Ok(());
    }
    fs::create_dir_all(&directory)?;
    match fs::File::create_new(&entry) {
        Ok(mut file) => {
            let result = file.write_all(b"[Desktop Entry]\nType=Application\nName=Sunna\nNoDisplay=true\nExec=sunnad --version\n");
            if result.is_err() {
                let _ = fs::remove_file(entry);
            }
            result
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

async fn local_status(path: &Path) -> Sharing {
    if let Ok(output) = Command::new(path)
        .env("PATH", "/usr/bin:/bin")
        .args(["status", "--json"])
        .output()
        .await
    {
        if !output.status.success() {
            return failure(
                off(),
                "Sunna couldn’t read the host settings.",
                "Check sunna-host status.",
            );
        }
        if let Ok(mut status) = serde_json::from_slice::<Sharing>(&output.stdout) {
            status.permissions.clear();
            let packages_missing = status.state == "error"
                && status.reason.as_deref()
                    == Some("The background desktop needs system packages.");
            status.reason = if packages_missing {
                Some("The background desktop needs system packages.".into())
            } else if status.state == "error" {
                Some("Sunna couldn’t read the host settings.".into())
            } else {
                None
            };
            if !packages_missing {
                status.fix = None;
            }
            if let Some(fix) = &mut status.fix {
                fix.label = "Install the desktop packages.".into();
                fix.command = match fix.command.as_deref() {
                    Some("sudo apt install xvfb x11-utils dbus-x11 xfce4") => Some("aptXfce"),
                    Some("sudo apt install xvfb x11-utils dbus-x11 plasma-workspace") => Some("aptPlasma"),
                    Some("sudo apt install xvfb x11-utils dbus-x11 plasma-session-x11") => Some("aptPlasmaX11"),
                    Some("sudo pacman -S xorg-server-xvfb xorg-xdpyinfo dbus xfce4") => Some("pacmanXfce"),
                    Some("sudo pacman -S xorg-server-xvfb xorg-xdpyinfo dbus plasma-x11-session") => Some("pacmanPlasma"),
                    Some("sudo dnf install xorg-x11-server-Xvfb xdpyinfo dbus-x11 @xfce-desktop-environment") => Some("dnfXfce"),
                    Some("sudo dnf install xorg-x11-server-Xvfb xdpyinfo dbus-x11 plasma-workspace-x11") => Some("dnfPlasma"),
                    _ => None,
                }.map(str::to_owned);
            }
            status.managed_by = Some(
                if status.managed_by.as_deref() == Some("app") {
                    "app"
                } else {
                    "cli"
                }
                .into(),
            );
            return status;
        }
        // Older packaged helpers accept extra arguments but print human-readable status.
        if !config().join("sunna/host.env").exists() {
            return off();
        }
        let text = String::from_utf8_lossy(&output.stdout);
        if text.starts_with("Sunna host:") {
            let mut snapshot = off();
            snapshot.managed_by = Some("cli".into());
            snapshot.starts_by_itself = config().join("autostart/sunna-host.desktop").exists()
                || systemctl(&["--user", "is-enabled", "--quiet", UNIT]).await;
            snapshot.mode = text
                .lines()
                .find_map(|l| l.trim().strip_prefix("Mode "))
                .and_then(|m| m.split_whitespace().next())
                .map(String::from);
            if text.lines().next() == Some("Sunna host: active") {
                snapshot.state = "on".into();
                if let Ok(output) = Command::new(path)
                    .env("PATH", "/usr/bin:/bin")
                    .arg("link")
                    .output()
                    .await
                {
                    if !output.status.success() {
                        return snapshot;
                    }
                    snapshot.link = String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .map(str::trim)
                        .find(|s| s.starts_with("sunna://"))
                        .map(String::from);
                    if let Some((address, _)) = snapshot
                        .link
                        .as_deref()
                        .and_then(|s| s.strip_prefix("sunna://"))
                        .and_then(|s| s.split_once('?'))
                    {
                        snapshot.address = Some(address.into());
                        let settings =
                            fs::read_to_string(config().join("sunna/host.env")).unwrap_or_default();
                        snapshot.reach = Some(
                            if settings.lines().any(|l| l == "SUNNA_LISTEN=tailscale") {
                                "tailscale"
                            } else {
                                "lan"
                            }
                            .into(),
                        );
                    }
                }
            }
            return snapshot;
        }
    }
    failure(
        off(),
        "Sunna couldn’t read the host settings.",
        "Check sunna-host status.",
    )
}

pub async fn status() -> Sharing {
    let system = systemctl(&["is-active", "--quiet", UNIT]).await;
    let enabled = systemctl(&["is-enabled", "--quiet", UNIT]).await;
    let system_unit = unit_definition(false).await;
    if system || enabled || system_unit.is_some() {
        let unit = system_unit.unwrap_or_default();
        if !system_unit_matches(&unit) {
            return external();
        }
        let receipt = fs::read_to_string("/run/sunna-host/receipt").unwrap_or_default();
        let unit = Command::new("/usr/bin/systemctl")
            .args(["show", "-p", "InvocationID", "-p", "MainPID", UNIT])
            .output()
            .await
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        let readiness = receipt_state(&receipt, &unit);
        let address = fs::read_to_string("/run/sunna-host/listening")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .filter(|_| readiness.is_some());
        let greeter = fs::read_to_string("/run/sunna-host/shares-greeter")
            .ok()
            .and_then(|s| match s.trim() {
                "1" => Some(true),
                "0" => Some(false),
                _ => None,
            });
        return Sharing {
            state: system_state(system, address).into(),
            address: address.filter(|_| system).map(|a| a.to_string()),
            managed_by: Some("system".into()),
            mode: Some("desktop".into()),
            reason: (system && readiness == Some("empty"))
                .then(|| "There is no desktop to share yet.".into()),
            starts_by_itself: enabled,
            shows_login_screen: greeter,
            fix: Some(Fix {
                label: "An administrator manages whole-machine sharing.".into(),
                command: None,
            }),
            ..off()
        };
    }
    let mut status = match helper().await {
        Err(()) => return external(),
        Ok(Some(p)) => local_status(&p).await,
        Ok(None)
            if from_path("sunnad").is_some()
                || from_path("sunna-host").is_some()
                || config().join("sunna/host.env").exists() =>
        {
            return external()
        }
        Ok(None) => off(),
    };
    if status.state == "on" {
        if let (Some(address), Some(key)) = (
            &status.address,
            status
                .link
                .as_deref()
                .and_then(|link| tauri::Url::parse(link).ok())
                .and_then(|url| {
                    url.query_pairs()
                        .find(|(field, _)| field == "key")
                        .map(|(_, key)| key.into_owned())
                }),
        ) {
            let check = crate::machines::check(status.link.as_deref().unwrap_or(address), &key).await;
            // A journal heartbeat can outlive the viewer. The authenticated
            // probe describes the session at the time of the stop decision.
            if check.state == "ready" || check.state == "not-sharing" {
                status.viewer = None;
            }
            if check.state == "busy" {
                status.viewer = Some(if check.viewer.is_empty() {
                    "Someone".into()
                } else {
                    check.viewer
                });
            }
            if check.state == "not-sharing" {
                status.reason = Some(check.detail);
            }
        }
    } else if wayland() && !status.starts_by_itself {
        let desktop = env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .to_lowercase();
        if !desktop.contains("gnome") && !desktop.contains("kde") {
            status.supported = false;
            return failure(
                status,
                "This Wayland desktop can’t share its screen.",
                "Sign in to GNOME, KDE, or an X11 session.",
            );
        }
        status.permissions.push(Permission {
            id: "portal".into(),
            // setup --desktop explicitly authorizes the session, even after an earlier grant.
            granted: false,
            label: "Choose your screen, allow remote interaction, and remember the selection when your desktop asks.".into(),
        });
    }
    status
}

async fn install() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tokio::task::spawn_blocking(|| {
        let updater = Updater::new("host")?;
        let _daemon = updater.lock("daemon.lock", false)?;
        updater.migrate()?;
        updater.prepare()?;
        let _idle = updater.lock("session.lock", false)?;
        updater.activate()?;
        updater.install_links()?;
        let marker = updater.root.join("installed-by-app");
        if !marker.exists() {
            fs::File::create_new(marker)?;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await?
}

async fn stop_user_host(mode: Option<&str>) -> bool {
    let autostart = config().join("autostart/sunna-host.desktop");
    if fs::remove_file(autostart).is_err_and(|e| e.kind() != std::io::ErrorKind::NotFound) {
        return false;
    }
    // GNOME's user manager generates a service from the login entry. Reload
    // after removal so a later login on the same manager cannot restart it.
    if !systemctl(&["--user", "disable", UNIT]).await
        || !systemctl(&["--user", "daemon-reload"]).await
    {
        return false;
    }
    if !systemctl(&["--user", "stop", UNIT]).await {
        return false;
    }
    if mode == Some("virtual") && !systemctl(&["--user", "stop", "sunna-desktop.service"]).await {
        return false;
    }
    // Require affirmative observations. A failed query is not proof of shutdown.
    user_property_is(UNIT, "ActiveState", "inactive").await
        && user_property_is(UNIT, "UnitFileState", "disabled").await
        && (mode != Some("virtual")
            || user_property_is("sunna-desktop.service", "ActiveState", "inactive").await)
        && matches!(
            config().join("autostart/sunna-host.desktop").try_exists(),
            Ok(false)
        )
}

async fn user_property_is(unit: &str, property: &str, expected: &str) -> bool {
    Command::new("/usr/bin/systemctl")
        .args(["--user", "show", "--value", "-p", property, unit])
        .output()
        .await
        .is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == expected)
}

// Separate launcher windows may belong to different processes. Serialize
// setup and stop there too, including while the portal is awaiting consent.
fn mutation_guard() -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    let directory = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(config)
        .join("sunna");
    fs::create_dir_all(&directory)?;
    let guard = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(directory.join("sharing.lock"))?;
    guard.try_lock()?;
    Ok(guard)
}

pub async fn set(on: bool, progress: &(dyn Fn(Sharing) + Sync)) -> Sharing {
    let _guard = match mutation_guard() {
        Ok(guard) => guard,
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            return failure(
                status().await,
                "Another Sunna window is changing sharing.",
                "Wait for it to finish.",
            )
        }
        Err(_) => {
            return failure(
                status().await,
                "Sunna couldn’t open its sharing settings.",
                "Check that your user settings folder is writable.",
            )
        }
    };
    let mut s = status().await;
    if s.managed_by.as_deref() == Some("system") || !s.supported {
        return s;
    }
    if !on {
        s.state = "stopping".into();
        progress(s.clone());
        if !stop_user_host(s.mode.as_deref()).await {
            let observed = status().await;
            let reason = match (observed.state.as_str(), observed.starts_by_itself) {
                ("on", true) => "The host is still running and set to start at login.",
                ("on", false) => "The host is still running.",
                (_, true) => "The host is still set to start at login.",
                _ => "Sunna couldn’t verify that sharing stopped.",
            };
            return failure(observed, reason, "Check sunna-host status and try again.");
        }
        return status().await;
    }
    if s.state == "on" {
        return s;
    }
    if matches!(helper().await, Ok(None)) {
        s.state = "installing".into();
        progress(s.clone());
        if let Err(error) = install().await {
            // Updater errors may include the configured feed URL. Show only the diagnosis.
            let message = error.to_string().to_lowercase();
            let (reason, action) = if message.contains("signature") || message.contains("sha-256") {
                (
                    "Sunna couldn’t verify the host download.",
                    "Try again when a signed release is available.",
                )
            } else if message.contains("http")
                || message.contains("connection")
                || message.contains("dns")
            {
                (
                    "Sunna couldn’t reach the host download.",
                    "Check your network connection and try again.",
                )
            } else if message.contains("no download for this platform") {
                (
                    "There’s no signed host download for this platform.",
                    "Install the host package for your distribution.",
                )
            } else {
                (
                    "Sunna couldn’t install the verified host.",
                    "Check that your Sunna data folder is writable and try again.",
                )
            };
            return failure(s, reason, action);
        }
    }
    let Ok(Some(path)) = helper().await else {
        return failure(
            s,
            "Sunna couldn’t find the installed host.",
            "Check the host installation.",
        );
    };
    let restore_off = !s.starts_by_itself;
    s.reason = None;
    s.fix = None;
    s.state = "starting".into();
    progress(s.clone());
    if wayland() {
        if register_portal_app().is_err() {
            return failure(
                s,
                "Sunna couldn’t register with the desktop permission dialog.",
                "Check that your applications folder is writable and try again.",
            );
        }
        s.state = "needs-permission".into();
        progress(s.clone());
    }
    let output = Command::new(path)
        .env("PATH", "/usr/bin:/bin")
        .args(["setup", "--desktop"])
        .output()
        .await;
    if output.as_ref().is_ok_and(|out| out.status.success()) {
        return status().await;
    }
    // setup can fail after writing the login entry and starting a restarting
    // unit. Restore the user's previous off choice before reporting failure.
    if restore_off
        && config().join("autostart/sunna-host.desktop").exists()
        && !stop_user_host(Some("desktop")).await
    {
        return failure(
            status().await,
            "The host couldn’t start or turn itself off.",
            "Check sunna-host status before trying again.",
        );
    }
    match output {
        Ok(out)
            if String::from_utf8_lossy(&out.stderr)
                .to_lowercase()
                .contains("address already in use") =>
        {
            failure(
                s,
                "Another program is using the sharing port.",
                "Close the program using the port and try again.",
            )
        }
        Ok(out) if String::from_utf8_lossy(&out.stderr).contains("approved") => failure(
            s,
            "Your desktop didn’t allow sharing.",
            "Turn sharing on again and approve the request.",
        ),
        _ => failure(
            s,
            "The host couldn’t start sharing this desktop.",
            "Check sunna-host status and try again.",
        ),
    }
}

pub fn open_permission(_: &str) -> Result<(), String> {
    Err("Your desktop asks for permission when sharing starts.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_status_requires_a_bound_address() {
        assert_eq!(system_state(true, None), "starting");
        assert_eq!(
            system_state(true, Some("127.0.0.1:48800".parse().unwrap())),
            "on"
        );
        assert_eq!(system_state(false, None), "off");
    }

    #[test]
    fn another_units_command_is_external() {
        let path = Path::new("/usr/bin/sunna-host");
        assert!(!user_command_matches(
            "ExecStart=\"/tmp/sunna-host\" run",
            path
        ));
        assert!(user_command_matches(
            "ExecStart=\"/usr/bin/sunna-host\" run",
            path
        ));
    }
}
