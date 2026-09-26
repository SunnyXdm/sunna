//! Finding Sunna hosts on your Tailscale network, when you ask: the Add
//! Computer sheet's "Find on Tailscale". Nothing is scanned in the
//! background.

use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::machines;

#[derive(Debug, Clone, Serialize)]
pub struct Found {
    pub name: String,
    pub dns_name: String,
    pub ip: String,
    pub os: String,
    pub device: String,
    /// ready | busy | wrong-key | update-needed | not-sharing | offline
    pub state: String,
}

/// One look at the tailnet: who we are, and every other device.
#[derive(Debug, Clone, Serialize)]
pub struct Scan {
    pub this_device: String,
    pub machines: Vec<Found>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Status {
    #[serde(default)]
    peer: std::collections::HashMap<String, Peer>,
    #[serde(rename = "Self")]
    this: Option<Peer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Peer {
    #[serde(default)]
    host_name: String,
    #[serde(rename = "DNSName", default)]
    dns_name: String,
    #[serde(rename = "TailscaleIPs", default)]
    tailscale_ips: Vec<String>,
    #[serde(rename = "OS", default)]
    os: String,
    #[serde(default)]
    online: bool,
}

impl Peer {
    fn ipv4(&self) -> Option<String> {
        self.tailscale_ips
            .iter()
            .find(|ip| ip.parse::<std::net::Ipv4Addr>().is_ok())
            .cloned()
    }
}

/// The Tailscale CLI: apps opened from Finder get a minimal PATH, so look
/// in the usual places too.
fn tailscale() -> String {
    let candidates = [
        "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        "/opt/homebrew/bin/tailscale",
        "/usr/local/bin/tailscale",
        "/usr/bin/tailscale",
    ];
    candidates
        .iter()
        .find(|path| std::path::Path::new(path).exists())
        .map(|path| path.to_string())
        .unwrap_or_else(|| "tailscale".into())
}

fn status() -> Result<Status, String> {
    let output = Command::new(tailscale())
        .args(["status", "--json"])
        .output()
        .map_err(|_| "Tailscale isn't installed on this computer.".to_string())?;
    if !output.status.success() {
        return Err("Tailscale isn't connected. Open Tailscale and sign in.".into());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Unexpected Tailscale output: {error}"))
}

/// This computer's tailnet address, if it's on one.
pub async fn this_ip() -> Option<String> {
    let status = tokio::task::spawn_blocking(status).await.ok()?.ok()?;
    status.this.as_ref().and_then(Peer::ipv4)
}

/// Every Tailscale peer, probed in parallel for a Sunna host that `key`
/// opens. Hosts first, then the rest.
pub async fn scan(key: &str) -> Result<Scan, String> {
    let status = tokio::task::spawn_blocking(status)
        .await
        .map_err(|error| error.to_string())??;
    let this_device = status
        .this
        .as_ref()
        .map(|this| this.host_name.clone())
        .unwrap_or_default();
    let probes = status.peer.into_values().map(|peer| {
        let key = key.to_string();
        tokio::spawn(async move {
            let ip = peer.ipv4().unwrap_or_default();
            let (state, os, device) = if !peer.online || ip.is_empty() {
                ("offline".to_string(), pretty_os(&peer.os), String::new())
            } else {
                let check = machines::check(&ip, &key).await;
                let state = match check.state.as_str() {
                    "unreachable" | "not-found" | "invalid" => "not-sharing".to_string(),
                    other => other.to_string(),
                };
                let os = if check.os.is_empty() {
                    pretty_os(&peer.os)
                } else {
                    check.os
                };
                (state, os, check.device)
            };
            Found {
                name: peer.host_name,
                dns_name: peer.dns_name.trim_end_matches('.').to_string(),
                ip,
                os,
                device,
                state,
            }
        })
    });
    let mut machines = Vec::new();
    for probe in probes.collect::<Vec<_>>() {
        if let Ok(found) = probe.await {
            machines.push(found);
        }
    }
    machines.sort_by(|a, b| {
        rank(&a.state)
            .cmp(&rank(&b.state))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(Scan {
        this_device,
        machines,
    })
}

fn rank(state: &str) -> u8 {
    match state {
        "ready" => 0,
        "busy" => 1,
        "wrong-key" | "update-needed" => 2,
        "not-sharing" => 3,
        _ => 4,
    }
}

fn pretty_os(os: &str) -> String {
    match os {
        "linux" => "Linux".into(),
        "windows" => "Windows".into(),
        "android" => "Android".into(),
        other => other.to_string(),
    }
}
