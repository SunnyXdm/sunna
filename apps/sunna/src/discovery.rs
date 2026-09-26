//! Finding machines: Tailscale peers, each probed for a Sunna host.

use std::net::{IpAddr, SocketAddr};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const HOST_PORT: u16 = 48800;
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Serialize)]
pub struct Machine {
    pub name: String,
    pub dns_name: String,
    pub ip: String,
    pub os: String,
    /// ready | busy | wrong-token | update-needed | not-hosting | offline
    pub state: &'static str,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Status {
    #[serde(default)]
    peer: std::collections::HashMap<String, Peer>,
    #[serde(rename = "Self")]
    this: Option<Peer>,
    current_tailnet: Option<Tailnet>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Tailnet {
    #[serde(default)]
    name: String,
}

/// One look at the tailnet: who we are, and every other device.
#[derive(Debug, Clone, Serialize)]
pub struct Scan {
    pub this_device: String,
    pub tailnet: String,
    pub machines: Vec<Machine>,
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

/// The Tailscale CLI: apps opened from Finder get a minimal PATH, so look
/// in the usual places too.
fn tailscale() -> Option<String> {
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
        .or_else(|| Some("tailscale".into()))
}

fn status() -> Result<Status, String> {
    let cli = tailscale().ok_or("Tailscale isn't installed")?;
    let output = Command::new(&cli)
        .args(["status", "--json"])
        .output()
        .map_err(|error| format!("Couldn't run Tailscale ({error}). Is it installed?"))?;
    if !output.status.success() {
        return Err("Tailscale isn't connected. Open Tailscale and sign in.".into());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Unexpected Tailscale output: {error}"))
}

/// Every Tailscale peer, probed in parallel for a Sunna host.
pub async fn scan(token: &str) -> Result<Scan, String> {
    let status = tokio::task::spawn_blocking(status)
        .await
        .map_err(|error| error.to_string())??;
    let this_device = status
        .this
        .as_ref()
        .map(|this| this.host_name.clone())
        .unwrap_or_default();
    let tailnet = status
        .current_tailnet
        .map(|tailnet| tailnet.name)
        .unwrap_or_default();
    let peers: Vec<Peer> = status.peer.into_values().collect();
    let probes = peers.into_iter().map(|peer| {
        let token = token.to_string();
        async move {
            let ip = peer
                .tailscale_ips
                .iter()
                .find(|ip| ip.parse::<std::net::Ipv4Addr>().is_ok())
                .cloned()
                .unwrap_or_default();
            let state = if !peer.online || ip.is_empty() {
                "offline"
            } else {
                probe(&ip, &token).await
            };
            Machine {
                name: peer.host_name,
                dns_name: peer.dns_name.trim_end_matches('.').to_string(),
                ip,
                os: pretty_os(&peer.os),
                state,
            }
        }
    });
    let mut machines: Vec<Machine> = futures_join_all(probes).await;
    machines.sort_by(|a, b| {
        rank(a.state)
            .cmp(&rank(b.state))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(Scan {
        this_device,
        tailnet,
        machines,
    })
}

async fn probe(ip: &str, token: &str) -> &'static str {
    let Ok(ip) = ip.parse::<IpAddr>() else {
        return "not-hosting";
    };
    let addr = SocketAddr::new(ip, HOST_PORT);
    match sunna_client::probe(addr, "sunna", token, PROBE_TIMEOUT).await {
        Ok(result) if result.version != sunna_proto::PROTOCOL_VERSION => "update-needed",
        Ok(result) if !result.token_ok => "wrong-token",
        Ok(result) if result.busy => "busy",
        Ok(_) => "ready",
        Err(_) => "not-hosting",
    }
}

fn rank(state: &str) -> u8 {
    match state {
        "ready" => 0,
        "busy" => 1,
        "wrong-token" | "update-needed" => 2,
        "not-hosting" => 3,
        _ => 4,
    }
}

fn pretty_os(os: &str) -> String {
    match os {
        "macOS" => "macOS".into(),
        "linux" => "Linux".into(),
        "windows" => "Windows".into(),
        "iOS" => "iOS".into(),
        "android" => "Android".into(),
        other => other.to_string(),
    }
}

/// Run futures concurrently and collect their results in order.
async fn futures_join_all<F: std::future::Future + Send + 'static>(
    futures: impl IntoIterator<Item = F>,
) -> Vec<F::Output>
where
    F::Output: Send + 'static,
{
    let handles: Vec<_> = futures.into_iter().map(tokio::spawn).collect();
    let mut out = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Ok(value) = handle.await {
            out.push(value);
        }
    }
    out
}
