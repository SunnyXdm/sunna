//! Finding a host and asking it what it is: the address forms people type
//! or paste, the lookup, and one probe that says whether a viewer can
//! connect (ready, busy, wrong key...). Shared by the Sunna app and the
//! Android client.

use std::net::SocketAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The link printed by a Mac host and encoded in its QR code.
pub fn sharing_link(address: impl std::fmt::Display, key: &str, identity: Option<&str>) -> String {
    let mut query = form_urlencoded::Serializer::new(String::new());
    query.append_pair("key", key);
    if let Some(pin) = identity { query.append_pair("id", pin); }
    format!("sunna://{address}?{}", query.finish())
}

/// Read the pin before reducing a sharing link to a socket address.
pub fn link_identity(address: &str) -> Result<Option<String>, String> {
    let Some(link) = address.trim().strip_prefix("sunna://") else { return Ok(None) };
    let query = link.split('#').next().and_then(|v| v.split_once('?')).map(|(_, v)| v).unwrap_or("");
    let mut pins = form_urlencoded::parse(query.as_bytes()).filter(|(field, _)| field == "id");
    let Some((_, pin)) = pins.next() else { return Ok(None) };
    if pins.next().is_some() || !sunna_transport::auth::valid_pin(&pin) { return Err("Invalid computer identity.".into()); }
    Ok(Some(pin.into_owned()))
}

pub const DEFAULT_PORT: u16 = 48800;
// A cold login manager can compete with QUIC for CPU during boot.
const PROBE_TIMEOUT: Duration = Duration::from_secs(4);
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(2);

/// One look at a machine.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Check {
    #[serde(default)]
    pub identity: Option<String>,
    #[serde(default)]
    pub unverified: bool,
    /// Optional JSON update state from a capable host; absent on older builds.
    pub update: Option<serde_json::Value>,
    /// ready | not-sharing | busy | identity-changed | wrong-key | update-needed | unreachable | not-found | invalid
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
    link_identity(address)?;
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
    check_with(address, key, &super::auth::Trust::default()).await
}

pub async fn check_with(address: &str, key: &str, trust: &super::auth::Trust) -> Check {
    let mut trust = trust.clone();
    match link_identity(address) {
        Ok(Some(pin)) if trust.identity.as_ref().is_some_and(|expected| expected != &pin) =>
            return Check::failed("identity-changed", sunna_transport::auth::IDENTITY_CHANGED),
        Ok(Some(pin)) => trust.identity = Some(pin),
        Ok(None) => {}
        Err(e) => return Check::failed("invalid", e),
    }
    let addr = match resolve(address).await {
        Ok(addr) => addr,
        Err(check) => return check,
    };
    let resolved = addr.to_string();
    let result = crate::probe_with(addr, "sunna", key.trim(), PROBE_TIMEOUT, &trust).await;
    let mut check = match result {
        Err(e) if e.to_string() == super::auth::OLD_HOST => Check::failed("update-needed", super::auth::OLD_HOST),
        Err(e) if e.to_string() == super::auth::WRONG_KEY || e.to_string().starts_with("Run sunna-host") => Check::failed("wrong-key", e.to_string()),
        Err(e) if e.to_string() == sunna_transport::auth::IDENTITY_CHANGED || e.to_string().contains("proof didn't match") => Check::failed("identity-changed", sunna_transport::auth::IDENTITY_CHANGED),
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
            } else if probe.not_sharing.is_some() {
                "not-sharing"
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
                "not-sharing" => probe.not_sharing.unwrap_or_default(),
                "busy" if !viewer.is_empty() => format!("{viewer} is connected to it right now."),
                "busy" => "Someone is connected to it right now.".to_string(),
                _ => String::new(),
            };
            let about = probe.about.unwrap_or_default();
            Check {
                unverified: probe.identity.is_none() && matches!(state, "ready" | "busy"),
                identity: probe.identity,
                update: probe.update.and_then(|s| serde_json::from_str(&s).ok()),
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
    fn links_preserve_the_pin_or_fail_closed() {
        let pin = sunna_transport::auth::encode_pin(&[42; 32]);
        let link = sharing_link("[::1]:48810", "test+&", Some(&pin));
        assert!(link.contains("key=test%2B%26&id="));
        assert_eq!(link_identity(&link).unwrap().as_ref(), Some(&pin));
        assert_eq!(parse(&link).unwrap(), ("::1".into(), 48810));
        assert!(link_identity("sunna://host?key=example").unwrap().is_none());
        for tail in ["id", "id=", "id=short", "%69d=short", "id=a&id=b"] {
            assert!(parse(&format!("sunna://host?{tail}")).is_err());
        }
    }

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
