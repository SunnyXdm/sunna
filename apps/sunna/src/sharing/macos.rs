use super::{Fix, Permission, Sharing};
use crate::{
    mac_host::{self, agent},
    mac_host_model as model,
};
use anyhow::{ensure, Result};

fn snapshot() -> Result<Sharing> {
    let host = mac_host::snapshot();
    if host.is_none() {
        if let Some((_, legacy)) = mac_host::legacy::status()? {
            return Ok(legacy);
        }
    }
    let enabled = agent::enabled();
    let (screen, input) = host
        .as_ref()
        .map_or((false, false), |h| (h.screen, h.input));
    let paused = enabled && agent::paused();
    let mut s = Sharing {
        supported: true,
        starts_by_itself: enabled,
        state: model::state(
            host.is_some(),
            enabled && !paused,
            screen,
            input,
            host.as_ref().and_then(|s| s.capture_reason.as_deref()),
        )
        .into(),
        permissions: vec![
            Permission {
                id: "screen".into(),
                granted: screen,
                label: "Screen Recording shares this Mac’s picture and system sound.".into(),
            },
            Permission {
                id: "input".into(),
                granted: input,
                label: "Accessibility lets a viewer use the mouse and keyboard.".into(),
            },
        ],
        ..Sharing::default()
    };
    if host.is_none() && enabled && !paused {
        if let Some(reason) = mac_host::last_start_error() {
            return Ok(error(reason));
        }
    }
    let host_absent = host.is_none();
    if let Some(host) = host {
        agent::refresh_grants(&host)?;
        s.mode = Some("desktop".into());
        s.managed_by = Some(if host.agent { "app" } else { "cli" }.into());
        s.address = Some(host.address.to_string());
        s.reach = Some(host.reach);
        s.viewer = host.viewer;
        s.link = Some(host.link);
        s.reason = host.capture_reason;
        if !screen || !input {
            s.reason = Some(
                "Waiting for permission on this Mac. If Settings lists Sunna as allowed, remove it from that list and add Sunna again."
                    .into(),
            );
            s.fix = Some(Fix {
                label: "Open Privacy & Security".into(),
                command: None,
            });
        }
    }
    if host_absent {
        s.permissions.clear();
    }
    Ok(s)
}

fn error(e: impl std::fmt::Display) -> Sharing {
    Sharing {
        supported: true,
        state: "error".into(),
        starts_by_itself: agent::enabled(),
        reason: Some(e.to_string()),
        ..Sharing::default()
    }
}

pub async fn status() -> Sharing {
    tokio::task::spawn_blocking(snapshot)
        .await
        .map_or_else(error, |r| r.unwrap_or_else(error))
}

fn change(on: bool) -> Result<Sharing> {
    let updater = sunna_update::Updater::new("app")?;
    let _operation = updater.lock("mac-host-management.lock", false)?;
    if on {
        if mac_host::snapshot().is_some() {
            return snapshot();
        }
        if let Some((_, legacy)) = mac_host::legacy::status()? {
            return Ok(legacy);
        }
        crate::settings::ensure_host_key()?;
        agent::install()?;
        agent::wait_started()?;
    } else {
        agent::remove()?;
        if let Some(host) = mac_host::snapshot() {
            if !host.agent {
                mac_host::request(b'Q')?;
            }
        } else if let Some((pid, _)) = mac_host::legacy::status()? {
            mac_host::legacy::stop(pid)?;
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while mac_host::instance_running()?
            || agent::loaded()
            || mac_host::legacy::status()?.is_some()
        {
            ensure!(
                std::time::Instant::now() < deadline,
                "The host is still running."
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        ensure!(
            !agent::enabled(),
            "The host is still set to start at login."
        );
    }
    snapshot()
}
pub async fn set(on: bool, progress: &(dyn Fn(Sharing) + Sync)) -> Sharing {
    let mut before = status().await;
    before.state = if on { "starting" } else { "stopping" }.into();
    progress(before);
    tokio::task::spawn_blocking(move || {
        let result = change(on);
        if !on && result.is_err() {
            if mac_host::instance_running().unwrap_or(false) {
                anyhow::bail!("The host is still running.");
            }
            if agent::enabled() {
                anyhow::bail!("The host is still set to start at login.");
            }
        }
        result
    })
    .await
    .map_or_else(error, |r| r.unwrap_or_else(error))
}

pub fn open_permission(which: &str) -> std::result::Result<(), String> {
    fn open(which: &str) -> Result<()> {
        let pane = match which {
            "screen" => {
                mac_host::request(b'P')?;
                "ScreenCapture"
            }
            "input" => {
                mac_host::request(b'I')?;
                "Accessibility"
            }
            _ => anyhow::bail!("Unknown permission."),
        };
        let status = std::process::Command::new("/usr/bin/open")
            .arg(format!(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_{pane}"
            ))
            .status()?;
        ensure!(
            status.success(),
            "Open System Settings → Privacy & Security."
        );
        Ok(())
    }
    open(which).map_err(|e| e.to_string())
}
