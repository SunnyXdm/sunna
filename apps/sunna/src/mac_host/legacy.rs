//! Migration from the old run.sh terminal host, which has no control socket.
//! Limit discovery to this UID's old binary and its actual local UDP socket.
use super::model;
use crate::{machines, settings, sharing::Sharing};
use anyhow::{ensure, Context, Result};
use std::{
    net::SocketAddr,
    process::Command,
    time::{Duration, Instant},
};

fn listener() -> Result<Option<(i32, SocketAddr)>> {
    let uid = unsafe { libc::geteuid() }.to_string();
    let out = Command::new("/usr/sbin/lsof")
        .args([
            "-nP",
            "-a",
            "-u",
            &uid,
            "-c",
            "/^sunnad$/",
            "-iUDP:48800",
            "-Fpcn",
        ])
        .output()
        .context("Could not check for the old Terminal host")?;
    model::legacy_listener(&String::from_utf8_lossy(&out.stdout)).map_err(anyhow::Error::msg)
}

pub fn status() -> Result<Option<(i32, Sharing)>> {
    let Some((pid, address)) = listener()? else {
        return Ok(None);
    };
    let key = settings::token();
    let check = tauri::async_runtime::block_on(machines::check(&address.to_string(), &key));
    ensure!(matches!(check.state.as_str(),"ready" | "busy" | "not-sharing" | "update-needed"),
        "A Terminal host is already using port 48800. Check this computer’s key or stop it with Control-C.");
    let tail = tauri::async_runtime::block_on(super::tailscale_ip());
    let reach = if tail.map(std::net::IpAddr::V4) == Some(address.ip()) {
        "tailscale"
    } else {
        "lan"
    };
    Ok(Some((
        pid,
        Sharing {
            supported: true,
            state: "on".into(),
            managed_by: Some("cli".into()),
            mode: Some("desktop".into()),
            reach: Some(reach.into()),
            address: Some(address.to_string()),
            link: Some(super::link(address, &key, check.identity.as_deref())),
            viewer: (!check.viewer.is_empty()).then_some(check.viewer),
            reason: (check.state == "not-sharing").then_some(check.detail),
            ..Sharing::default()
        },
    )))
}

pub fn stop(pid: i32) -> Result<()> {
    ensure!(
        listener()?.is_some_and(|(current, _)| current == pid),
        "The Terminal host changed. Refresh and try again."
    );
    if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while listener()?.is_some() {
        ensure!(
            Instant::now() < deadline,
            "The Terminal host is still running. Stop it in its Terminal window."
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
