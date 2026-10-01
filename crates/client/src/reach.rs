//! Finding a host and asking it what it is: the address forms people type
//! or paste, the lookup, and one probe that says whether a viewer can
//! connect (ready, busy, wrong key...). Shared by the Sunna app and the
//! Android client.

use std::net::SocketAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const DEFAULT_PORT: u16 = 48800;
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(2);

/// One look at a machine.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Check {
    /// ready | busy | wrong-key | update-needed | unreachable | not-found | invalid
    pub state: String,
    /// The host's own name; only told when the key matched.
    pub name: String,
    pub os: String,
    pub device: String,
    pub model: String,
    pub width: u32,
    pub height: u32,
    pub rtt_ms: Option<f64>,
    /// The socket address it resolved to.
    pub resolved: String,
    /// Why, for the states that need saying.
    pub detail: String,
    /// When busy: who's connected ("Priya's MacBook Air"), if the host says.
    pub viewer: String,
}

impl Check {
    pub fn failed(state: &str, detail: impl Into<String>) -> Self {
        Self {
            state: state.into(),
            detail: detail.into(),
            ..Self::default()
        }
    }
}

/// "host", "host:port", "[v6]:port", a bare IPv6 address, or a
/// `sunna://host[:port]?key=...` link, into host and port.
pub fn parse(address: &str) -> Result<(String, u16), String> {
    let mut rest = address.trim();
    if let Some(stripped) = rest.strip_prefix("sunna://") {
        rest = stripped;
    }
    let rest = rest.split(['?', '/', '#']).next().unwrap_or("").trim();
    if rest.is_empty() {
        return Err("Enter the computer's address.".into());
    }
    let (host, port) = if let Some(inner) = rest.strip_prefix('[') {
        let (host, after) = inner
            .split_once(']')
            .ok_or("That IPv6 address is missing its closing ']'.")?;
        let port = match after.strip_prefix(':') {
            Some(port) => Some(port),
            None if after.is_empty() => None,
            None => return Err("Unexpected text after the address.".into()),
        };
        (host.to_string(), port)
    } else if rest.matches(':').count() > 1 {
        (rest.to_string(), None)
    } else if let Some((host, port)) = rest.split_once(':') {
        (host.to_string(), Some(port))
    } else {
        (rest.to_string(), None)
    };
    if host.is_empty() || host.contains(char::is_whitespace) {
        return Err("That doesn't look like an address.".into());
    }
    let port = match port {
        None => DEFAULT_PORT,
        Some(port) => port
            .parse::<u16>()
            .ok()
            .filter(|port| *port > 0)
            .ok_or("The port must be a number from 1 to 65535.")?,
    };
    Ok((host, port))
}

// The error is the answer to show for the computer, so it's the whole Check.
#[allow(clippy::result_large_err)]
pub async fn resolve(address: &str) -> Result<SocketAddr, Check> {
    let (host, port) = parse(address).map_err(|error| Check::failed("invalid", error))?;
    let lookup = tokio::net::lookup_host((host.as_str(), port));
    let addresses: Vec<SocketAddr> = match tokio::time::timeout(RESOLVE_TIMEOUT, lookup).await {
        Ok(Ok(addresses)) => addresses.collect(),
        _ => {
            return Err(Check::failed(
                "not-found",
                format!("Couldn't find a computer called “{host}”."),
            ))
        }
    };
    // Prefer IPv4: tailnet and LAN hosts listen there.
    addresses
        .iter()
        .find(|address| address.is_ipv4())
        .or_else(|| addresses.first())
        .copied()
        .ok_or_else(|| Check::failed("not-found", format!("Couldn't find “{host}”.")))
}

/// Is a Sunna host there, does the key fit, and what is it?
pub async fn check(address: &str, key: &str) -> Check {
    let addr = match resolve(address).await {
        Ok(addr) => addr,
        Err(check) => return check,
    };
    let resolved = addr.to_string();
    let result = crate::probe(addr, "sunna", key.trim(), PROBE_TIMEOUT).await;
    let mut check = match result {
        Err(_) => Check::failed(
            "unreachable",
            format!("Nothing answered at {resolved}. Is Sunna sharing on that computer?"),
        ),
        Ok(probe) => {
            let ours = sunna_proto::PROTOCOL_VERSION;
            // A session from this computer (left behind by a crash or a
            // lost network) isn't "in use": connecting takes it back.
            let this_one = crate::device_id();
            let yours = probe.viewer.as_ref().is_some_and(|(_, device)| !this_one.is_empty() && *device == this_one);
            let state = if probe.version != ours {
                "update-needed"
            } else if !probe.token_ok {
                "wrong-key"
            } else if probe.busy && !yours {
                "busy"
            } else {
                "ready"
            };
            let viewer = match (&probe.viewer, state) {
                (Some((name, _)), "busy") => name.clone(),
                _ => String::new(),
            };
            let detail = match state {
                "update-needed" if probe.version > ours => {
                    "That computer runs a newer Sunna. Update this one.".to_string()
                }
                "update-needed" => "That computer runs an older Sunna. Update it.".to_string(),
                "wrong-key" => "Sunna is there, but the key doesn't match.".to_string(),
                "busy" if !viewer.is_empty() => format!("{viewer} is connected to it right now."),
                "busy" => "Someone is connected to it right now.".to_string(),
                _ => String::new(),
            };
            let about = probe.about.unwrap_or_default();
            Check {
                state: state.into(),
                name: probe.name,
                os: about.os,
                device: about.device,
                model: about.model,
                width: about.width,
                height: about.height,
                rtt_ms: Some(probe.rtt.as_secs_f64() * 1000.0),
                resolved: String::new(),
                detail,
                viewer,
            }
        }
    };
    check.resolved = resolved;
    check
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses() {
        let parsed = |text: &str| parse(text).unwrap();
        assert_eq!(
            parsed("100.124.64.79"),
            ("100.124.64.79".into(), DEFAULT_PORT)
        );
        assert_eq!(parsed(" archlinux:5000 "), ("archlinux".into(), 5000));
        assert_eq!(parsed("[fd7a::1]:48801"), ("fd7a::1".into(), 48801));
        assert_eq!(
            parsed("fd7a:115c::1"),
            ("fd7a:115c::1".into(), DEFAULT_PORT)
        );
        assert_eq!(
            parsed("sunna://100.124.64.79?key=abc"),
            ("100.124.64.79".into(), DEFAULT_PORT)
        );
        assert!(parse("").is_err());
        assert!(parse("host:0").is_err());
        assert!(parse("host:http").is_err());
        assert!(parse("two words").is_err());
    }
}
