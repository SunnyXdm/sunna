//! Sunna.app: the launcher (your computers, one-click connect, settings)
//! and, as `sunna viewer`, the native viewer each session runs in.
//!
//! Sessions get their own process: the video window stays pure native
//! (no webview near the frames), a viewer crash can't take the launcher
//! down, and the launcher simply hides until the session ends.

mod discovery;
mod machines;
mod settings;

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
struct Session {
    /// The running viewer's pid (0 while it starts); one session at a time.
    viewer: Mutex<Option<u32>>,
    /// The user cancelled the connection: no error to report.
    cancelled: AtomicBool,
}

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("viewer") {
        std::process::exit(run_viewer(args.collect()));
    }
    tauri::Builder::default()
        .manage(Session::default())
        .invoke_handler(tauri::generate_handler![
            app_info,
            list_machines,
            machine_statuses,
            check_machine,
            add_machine,
            update_machine,
            remove_machine,
            reorder_machines,
            scan_tailscale,
            this_computer,
            connect,
            cancel_connect,
            get_settings,
            set_settings,
            keyboard_access,
            allow_keyboard_access,
            reset_keyboard_access
        ])
        .run(tauri::generate_context!())
        .expect("Sunna failed to start");
}

/// `sunna viewer --addr IP:PORT --name NAME [--os OS] [--no-audio]`, key in SUNNA_TOKEN.
fn run_viewer(args: Vec<String>) -> i32 {
    let _telemetry = sunna_telemetry::init("viewer", sunna_telemetry::Remote::from_env());
    let value = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|index| args.get(index + 1))
            .cloned()
    };
    let Some(addr) = value("--addr").and_then(|addr| addr.parse().ok()) else {
        eprintln!("usage: sunna viewer --addr IP:PORT [--name NAME]");
        return 2;
    };
    let result = sunna_viewer::run(sunna_viewer::ViewerArgs {
        addr,
        server_name: "sunna".into(),
        token: std::env::var("SUNNA_TOKEN").unwrap_or_default(),
        host_name: value("--name").unwrap_or_else(|| addr.ip().to_string()),
        host_os: value("--os").unwrap_or_default(),
        audio: !args.iter().any(|arg| arg == "--no-audio")
            && std::env::var("SUNNA_AUDIO").map_or(true, |value| value != "0"),
    });
    match result {
        Ok(()) => 0,
        Err(error) => {
            tracing::error!("session error: {error:#}");
            sunna_telemetry::flush();
            1
        }
    }
}

#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": sunna_proto::PROTOCOL_VERSION,
        "computer": computer_name(),
        "platform": std::env::consts::OS,
    })
}

#[tauri::command]
fn list_machines() -> Vec<machines::Machine> {
    machines::list()
}

#[tauri::command]
async fn machine_statuses() -> HashMap<String, machines::Check> {
    machines::statuses().await
}

#[tauri::command]
async fn check_machine(address: String, key: String) -> machines::Check {
    machines::check(&address, &key).await
}

#[tauri::command]
fn add_machine(draft: machines::Draft) -> Result<machines::Machine, String> {
    machines::add(draft)
}

#[tauri::command]
fn update_machine(id: String, draft: machines::Draft) -> Result<machines::Machine, String> {
    machines::update(&id, draft)
}

#[tauri::command]
fn remove_machine(id: String) -> Result<(), String> {
    machines::remove(&id)
}

#[tauri::command]
fn reorder_machines(ids: Vec<String>) -> Result<(), String> {
    machines::reorder(&ids)
}

#[tauri::command]
async fn scan_tailscale(key: String) -> Result<discovery::Scan, String> {
    discovery::scan(&key).await
}

#[derive(Serialize)]
struct ThisComputer {
    name: String,
    /// Tailnet address, when on one.
    address: Option<String>,
    key: String,
    /// What a probe of our own host says: ready, busy, wrong-key,
    /// unreachable (not sharing), or unknown (no tailnet address).
    sharing: String,
}

#[tauri::command]
async fn this_computer() -> ThisComputer {
    let key = settings::token();
    let address = discovery::this_ip().await;
    let sharing = match &address {
        Some(ip) => machines::check(ip, &key).await.state,
        None => "unknown".into(),
    };
    ThisComputer {
        name: computer_name(),
        address,
        key,
        sharing,
    }
}

#[tauri::command]
fn get_settings() -> settings::Settings {
    settings::load()
}

#[tauri::command]
fn set_settings(settings: settings::Settings) -> Result<(), String> {
    settings::save(&settings)
}

/// Whether ⌘Tab, ⌘Space and other system shortcuts can go to the other
/// computer: macOS's Accessibility permission. Always yes off macOS.
#[tauri::command]
fn keyboard_access() -> bool {
    #[cfg(target_os = "macos")]
    return sunna_input::macos::accessibility_granted();
    #[cfg(not(target_os = "macos"))]
    true
}

/// Ask for it: macOS adds Sunna to the Accessibility list (and may show its
/// prompt), and System Settings opens at that list.
#[tauri::command]
fn allow_keyboard_access() {
    #[cfg(target_os = "macos")]
    if !sunna_input::macos::request_accessibility() {
        open_accessibility_settings();
    }
}

/// When the list says Sunna is allowed but it still isn't (an entry left
/// from a differently signed build): forget Sunna's entry and ask afresh.
#[tauri::command]
fn reset_keyboard_access() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("/usr/bin/tccutil")
            .args(["reset", "Accessibility", "dev.sunna.app"])
            .status()
            .map_err(|error| error.to_string())?;
        if !status.success() {
            open_accessibility_settings();
            return Err("Couldn't reset it. In the list that opened, select Sunna, remove it with −, then allow it again.".into());
        }
        allow_keyboard_access();
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_accessibility_settings() {
    let _ = Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .status();
}

/// Open a session in a viewer process. The launcher stays up with the
/// connection's progress until the viewer's window opens, hides while the
/// session runs, and comes back when it ends (with why, if it failed).
#[tauri::command]
async fn connect(app: AppHandle, session: State<'_, Session>, id: String) -> Result<(), String> {
    {
        let mut viewer = session.viewer.lock().unwrap();
        if viewer.is_some() {
            return Err("A session is already open.".into());
        }
        *viewer = Some(0);
    }
    session.cancelled.store(false, Ordering::SeqCst);
    let release = || *session.viewer.lock().unwrap() = None;
    let Some(machine) = machines::get(&id) else {
        release();
        return Err("That computer is no longer in your list.".into());
    };
    let addr = match machines::resolve(&machine.address).await {
        Ok(addr) => addr,
        Err(check) => {
            release();
            return Err(check.detail);
        }
    };
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(error) => {
            release();
            return Err(error.to_string());
        }
    };
    let mut args = vec![
        "viewer".to_string(),
        "--addr".into(),
        addr.to_string(),
        "--name".into(),
        machine.name.clone(),
    ];
    // Lets the session menu offer the host's own shortcuts.
    if !machine.os.is_empty() {
        args.extend(["--os".to_string(), machine.os.clone()]);
    }
    let spawned = Command::new(exe)
        .args(&args)
        .envs(settings::viewer_env())
        // This machine's key, over sunna.env's.
        .env("SUNNA_TOKEN", &machine.key)
        // How the host lists this Mac while it's connected.
        .env("SUNNA_VIEWER_NAME", computer_name())
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => {
            release();
            return Err(format!("Couldn't start the viewer: {error}"));
        }
    };
    *session.viewer.lock().unwrap() = Some(child.id());
    let stderr = child.stderr.take();
    let mut log = session_log(&machine.name);
    std::thread::spawn(move || {
        let progress = |phase: &str| {
            let _ = app.emit(
                "session-progress",
                serde_json::json!({ "id": id, "phase": phase }),
            );
        };
        // The viewer logs to stderr: follow its progress, keep the last error.
        let mut last_error = None;
        let mut opened = false;
        if let Some(stderr) = stderr {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let line = strip_ansi(&line);
                if let Some(file) = log.as_mut() {
                    use std::io::Write;
                    let _ = writeln!(file, "{line}");
                }
                if line.contains("session established") {
                    machines::mark_connected(&id);
                    progress("video");
                } else if !opened && line.contains("viewer window") {
                    opened = true;
                    progress("open");
                    hide(&app);
                }
                if let Some(reason) = failure_reason(&line) {
                    last_error = Some(reason);
                }
            }
        }
        let status = child.wait();
        let session = app.state::<Session>();
        let reason = match status {
            _ if session.cancelled.load(Ordering::SeqCst) => None,
            Ok(status) if status.success() => None,
            _ => Some(last_error.unwrap_or_else(|| "The session ended unexpectedly.".into())),
        };
        *session.viewer.lock().unwrap() = None;
        show(&app);
        let _ = app.emit(
            "session-ended",
            serde_json::json!({ "id": id, "reason": reason, "opened": opened }),
        );
    });
    Ok(())
}

/// Each session's log, kept on this computer (the last 30): on macOS in
/// ~/Library/Logs/Sunna, where Console finds it too.
fn session_log(name: &str) -> Option<std::fs::File> {
    let home = std::path::PathBuf::from(std::env::var_os("HOME")?);
    let dir = if cfg!(target_os = "macos") {
        home.join("Library/Logs/Sunna")
    } else {
        home.join(".sunna/logs")
    };
    std::fs::create_dir_all(&dir).ok()?;
    let mut old: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "log"))
        .collect();
    old.sort();
    for path in old.iter().rev().skip(29) {
        let _ = std::fs::remove_file(path);
    }
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' }).collect();
    std::fs::File::create(dir.join(format!("{} {safe}.log", utc_stamp()))).ok()
}

/// "2026-09-27 18.40.05": sorts in time order, fine in a file name.
fn utc_stamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    utc_stamp_at(seconds)
}

fn utc_stamp_at(seconds: u64) -> String {
    let (days, rest) = ((seconds / 86_400) as i64, seconds % 86_400);
    // Civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}.{:02}.{:02}", rest / 3600, rest / 60 % 60, rest % 60)
}

/// Stop a connection that hasn't opened yet.
#[tauri::command]
fn cancel_connect(session: State<'_, Session>) {
    if let Some(pid) = *session.viewer.lock().unwrap() {
        if pid != 0 {
            session.cancelled.store(true, Ordering::SeqCst);
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
    }
}

/// A readable reason from a viewer error line, if it is one.
fn failure_reason(line: &str) -> Option<String> {
    if line.contains("panicked at") {
        return Some("The viewer crashed. Its log has the details.".into());
    }
    let (_, message) = line.split_once("session error: ")?;
    let message = message.trim();
    let friendly = if message.starts_with("lost the connection") {
        "Lost the connection: nothing came back from that computer for 10 seconds. Its network dropped, or it went to sleep.".to_string()
    } else if message.starts_with("the host ended the session") {
        "That computer ended the session: it restarted, or stopped sharing.".to_string()
    } else if message.starts_with("replaced") {
        "This session moved to a newer one from this Mac.".to_string()
    } else if message.contains("wrong session token") {
        "The key doesn't match. Edit the computer and paste its key.".to_string()
    } else if let Some(who) = message.split_once("busy: ").and_then(|(_, rest)| rest.strip_suffix(" is connected")) {
        format!("{who} is connected to that computer right now.")
    } else if message.contains("busy") {
        "Someone else is connected to that computer.".to_string()
    } else if message.contains("no decoder") {
        format!("This computer can't play that computer's video ({message}).")
    } else if message.contains("protocol version mismatch") {
        "That computer runs a different version of Sunna. Update both.".to_string()
    } else if message.contains("timed out") || message.contains("connect") {
        format!("Couldn't reach that computer ({message}).")
    } else {
        message.to_string()
    };
    Some(friendly)
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn command_output(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

fn computer_name() -> String {
    #[cfg(target_os = "macos")]
    let name = command_output("/usr/sbin/scutil", &["--get", "ComputerName"]);
    #[cfg(not(target_os = "macos"))]
    let name = std::fs::read_to_string("/etc/hostname")
        .map(|name| name.trim().to_string())
        .unwrap_or_default();
    if name.is_empty() {
        command_output("hostname", &[])
    } else {
        name
    }
}

fn hide(app: &AppHandle) {
    // One Dock icon while a session runs, the session's own: the launcher
    // leaves the Dock (and the menu bar) until the session ends.
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    #[cfg(target_os = "macos")]
    let _ = app.hide();
    #[cfg(not(target_os = "macos"))]
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

fn show(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    #[cfg(target_os = "macos")]
    let _ = app.show();
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_are_friendly() {
        let line = strip_ansi(
            "\u{1b}[2m2026\u{1b}[0m ERROR sunna: session error: host refused the session: wrong session token",
        );
        assert_eq!(
            failure_reason(&line).unwrap(),
            "The key doesn't match. Edit the computer and paste its key."
        );
        assert!(failure_reason("INFO sunna_client: window fps=60").is_none());
        let lost = failure_reason("ERROR sunna_viewer: session error: lost the connection: nothing came back for 10 seconds");
        assert!(lost.unwrap().starts_with("Lost the connection"));
        let ended = failure_reason("ERROR sunna_viewer: session error: the host ended the session");
        assert!(ended.unwrap().starts_with("That computer ended the session"));
        let busy = failure_reason("ERROR x: session error: host refused the session: busy: Priya's Mac is connected");
        assert_eq!(busy.unwrap(), "Priya's Mac is connected to that computer right now.");
    }

    #[test]
    fn log_names_carry_the_utc_time() {
        assert_eq!(utc_stamp_at(0), "1970-01-01 00.00.00");
        assert_eq!(utc_stamp_at(951_782_400), "2000-02-29 00.00.00");
        assert_eq!(utc_stamp_at(1_790_523_869), "2026-09-27 15.44.29");
        assert_eq!(utc_stamp_at(4_102_444_800), "2100-01-01 00.00.00");
    }
}
