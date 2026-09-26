//! Sunna.app: the launcher (machines on your tailnet, one-click connect,
//! settings) and, as `sunna viewer`, the native viewer each session runs in.
//!
//! Sessions get their own process: the video window stays pure native
//! (no webview near the frames), a viewer crash can't take the launcher
//! down, and the launcher simply hides until the session ends.

mod discovery;
mod settings;

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
struct Session {
    /// A viewer is running; only one at a time.
    active: Mutex<bool>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("viewer") {
        std::process::exit(run_viewer(args.collect()));
    }
    tauri::Builder::default()
        .manage(Session::default())
        .invoke_handler(tauri::generate_handler![
            scan,
            app_info,
            connect,
            get_settings,
            set_settings
        ])
        .run(tauri::generate_context!())
        .expect("Sunna failed to start");
}

/// `sunna viewer --addr IP:PORT --name NAME`, token in SUNNA_TOKEN.
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
async fn scan() -> Result<discovery::Scan, String> {
    discovery::scan(&settings::token()).await
}

#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": sunna_proto::PROTOCOL_VERSION,
        // The window is transparent over the system's sidebar material.
        "vibrancy": cfg!(target_os = "macos"),
    })
}

#[tauri::command]
fn get_settings() -> settings::Settings {
    settings::load()
}

#[tauri::command]
fn set_settings(settings: settings::Settings) -> Result<(), String> {
    settings::save(&settings)
}

/// Open a session in a viewer process; the launcher hides until it ends,
/// then reports why if it failed.
#[tauri::command]
fn connect(
    app: AppHandle,
    session: State<'_, Session>,
    ip: String,
    name: String,
) -> Result<(), String> {
    {
        let mut active = session.active.lock().unwrap();
        if *active {
            return Err("A session is already open.".into());
        }
        *active = true;
    }
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let addr = format!("{ip}:{}", discovery::HOST_PORT);
    let spawned = Command::new(exe)
        .args(["viewer", "--addr", &addr, "--name", &name])
        .envs(settings::viewer_env())
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => {
            *session.active.lock().unwrap() = false;
            return Err(format!("Couldn't start the viewer: {error}"));
        }
    };
    hide(&app);
    let stderr = child.stderr.take();
    std::thread::spawn(move || {
        // The viewer logs to stderr; keep the last error for the banner.
        let mut last_error = None;
        if let Some(stderr) = stderr {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if let Some(reason) = failure_reason(&line) {
                    last_error = Some(reason);
                }
            }
        }
        let status = child.wait();
        let reason = match status {
            Ok(status) if status.success() => None,
            _ => Some(last_error.unwrap_or_else(|| "The session ended unexpectedly.".into())),
        };
        *app.state::<Session>().active.lock().unwrap() = false;
        show(&app);
        let _ = app.emit("session-ended", serde_json::json!({ "reason": reason }));
    });
    Ok(())
}

/// A readable reason from a viewer error line, if it is one.
fn failure_reason(line: &str) -> Option<String> {
    let line = strip_ansi(line);
    if line.contains("panicked at") {
        return Some("The viewer crashed. Its log has the details.".into());
    }
    let (_, message) = line.split_once("session error: ")?;
    let message = message.trim();
    let friendly = if message.contains("wrong session token") {
        "That machine uses a different session token. Check Settings.".to_string()
    } else if message.contains("busy") {
        "Someone else is connected to that machine.".to_string()
    } else if message.contains("no decoder") {
        format!("This computer can't play that machine's video ({message}).")
    } else if message.contains("protocol version mismatch") {
        "That machine runs a different Sunna version. Update both.".to_string()
    } else if message.contains("timed out") || message.contains("connect") {
        format!("Couldn't reach that machine ({message}).")
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

fn hide(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.hide();
    #[cfg(not(target_os = "macos"))]
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

fn show(app: &AppHandle) {
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
        let line = "\u{1b}[2m2026\u{1b}[0m ERROR sunna: session error: host refused the session: wrong session token";
        assert_eq!(
            failure_reason(line).unwrap(),
            "That machine uses a different session token. Check Settings."
        );
        assert!(failure_reason("INFO sunna_client: window fps=60").is_none());
    }
}
