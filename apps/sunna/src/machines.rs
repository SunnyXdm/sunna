//! Computers you've added: where they are, their key, and what they last
//! told us about themselves. Stored in ~/.sunna/machines.json, owner-only
//! (the keys are secrets).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const DEFAULT_PORT: u16 = 48800;
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(2);

/// Serializes read-modify-write of the store across concurrent commands.
static STORE: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Machine {
    pub id: String,
    pub name: String,
    /// As entered: "100.124.64.79", "archlinux:48800", "[fd7a::1]".
    pub address: String,
    pub key: String,
    /// What the host last said it is; kept while it's offline.
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    /// Unix seconds; 0 for never.
    #[serde(default)]
    pub added: u64,
    #[serde(default)]
    pub last_seen: u64,
    #[serde(default)]
    pub last_connected: u64,
}

/// What the add and edit forms send.
#[derive(Debug, Clone, Deserialize)]
pub struct Draft {
    pub name: String,
    pub address: String,
    pub key: String,
    /// From the form's own check, so a new tile shows the right machine
    /// before the first status round.
    #[serde(default)]
    pub about: Option<Check>,
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    machines: Vec<Machine>,
}

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
}

impl Check {
    fn failed(state: &str, detail: impl Into<String>) -> Self {
        Self {
            state: state.into(),
            detail: detail.into(),
            ..Self::default()
        }
    }
}

fn path() -> std::path::PathBuf {
    crate::settings::dir().join("machines.json")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn load() -> Store {
    std::fs::read(path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save(store: &Store) -> Result<(), String> {
    std::fs::create_dir_all(crate::settings::dir()).map_err(|error| error.to_string())?;
    let json = serde_json::to_vec_pretty(store).map_err(|error| error.to_string())?;
    // Write then rename, so a crash can't leave half a file.
    let path = path();
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, json).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&temporary, &path).map_err(|error| error.to_string())
}

fn modify<T>(change: impl FnOnce(&mut Store) -> Result<T, String>) -> Result<T, String> {
    let _guard = STORE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut store = load();
    let value = change(&mut store)?;
    save(&store)?;
    Ok(value)
}

pub fn list() -> Vec<Machine> {
    let _guard = STORE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    load().machines
}

pub fn get(id: &str) -> Option<Machine> {
    list().into_iter().find(|machine| machine.id == id)
}

fn new_id() -> String {
    let mut bytes = [0u8; 8];
    if getrandom(&mut bytes).is_err() {
        bytes = now().to_le_bytes();
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn getrandom(bytes: &mut [u8]) -> std::io::Result<()> {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")?.read_exact(bytes)
}

fn validate(draft: &Draft) -> Result<(), String> {
    parse(&draft.address)?;
    if draft.key.contains(['\n', '\r']) {
        return Err("The key can't contain line breaks.".into());
    }
    Ok(())
}

fn learn(machine: &mut Machine, check: &Check) {
    if !check.os.is_empty() {
        machine.os = check.os.clone();
    }
    if !check.device.is_empty() {
        machine.device = check.device.clone();
    }
    if !check.model.is_empty() {
        machine.model = check.model.clone();
    }
    if check.width > 0 && check.height > 0 {
        machine.width = check.width;
        machine.height = check.height;
    }
    if matches!(check.state.as_str(), "ready" | "busy") {
        machine.last_seen = now();
    }
}

pub fn add(draft: Draft) -> Result<Machine, String> {
    validate(&draft)?;
    modify(|store| {
        let mut machine = Machine {
            id: new_id(),
            name: display_name(&draft),
            address: draft.address.trim().to_string(),
            key: draft.key.trim().to_string(),
            os: String::new(),
            device: String::new(),
            model: String::new(),
            width: 0,
            height: 0,
            added: now(),
            last_seen: 0,
            last_connected: 0,
        };
        if let Some(check) = &draft.about {
            learn(&mut machine, check);
        }
        store.machines.push(machine.clone());
        Ok(machine)
    })
}

pub fn update(id: &str, draft: Draft) -> Result<Machine, String> {
    validate(&draft)?;
    modify(|store| {
        let machine = store
            .machines
            .iter_mut()
            .find(|machine| machine.id == id)
            .ok_or("That computer is no longer in your list.")?;
        let moved = machine.address != draft.address.trim();
        machine.name = display_name(&draft);
        machine.address = draft.address.trim().to_string();
        machine.key = draft.key.trim().to_string();
        if moved {
            // A different address may be a different machine.
            machine.os.clear();
            machine.device.clear();
            machine.model.clear();
            machine.width = 0;
            machine.height = 0;
        }
        if let Some(check) = &draft.about {
            learn(machine, check);
        }
        Ok(machine.clone())
    })
}

pub fn remove(id: &str) -> Result<(), String> {
    modify(|store| {
        store.machines.retain(|machine| machine.id != id);
        Ok(())
    })
}

/// Put the machines in this order (ids not listed keep their place at the end).
pub fn reorder(ids: &[String]) -> Result<(), String> {
    modify(|store| {
        let position = |machine: &Machine| {
            ids.iter()
                .position(|id| *id == machine.id)
                .unwrap_or(usize::MAX)
        };
        store.machines.sort_by_key(position);
        Ok(())
    })
}

pub fn mark_connected(id: &str) {
    let _ = modify(|store| {
        if let Some(machine) = store.machines.iter_mut().find(|machine| machine.id == id) {
            machine.last_connected = now();
            machine.last_seen = now();
        }
        Ok(())
    });
}

fn display_name(draft: &Draft) -> String {
    let typed = draft.name.trim();
    if !typed.is_empty() {
        return typed.to_string();
    }
    if let Some(check) = &draft.about {
        if !check.name.is_empty() {
            return check.name.clone();
        }
    }
    parse(&draft.address)
        .map(|(host, _)| host)
        .unwrap_or_else(|_| draft.address.trim().to_string())
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
    let result = sunna_client::probe(addr, "sunna", key.trim(), PROBE_TIMEOUT).await;
    let mut check = match result {
        Err(_) => Check::failed(
            "unreachable",
            format!("Nothing answered at {resolved}. Is Sunna sharing on that computer?"),
        ),
        Ok(probe) => {
            let ours = sunna_proto::PROTOCOL_VERSION;
            let state = if probe.version != ours {
                "update-needed"
            } else if !probe.token_ok {
                "wrong-key"
            } else if probe.busy {
                "busy"
            } else {
                "ready"
            };
            let detail = match state {
                "update-needed" if probe.version > ours => {
                    "That computer runs a newer Sunna. Update this one.".to_string()
                }
                "update-needed" => "That computer runs an older Sunna. Update it.".to_string(),
                "wrong-key" => "Sunna is there, but the key doesn't match.".to_string(),
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
            }
        }
    };
    check.resolved = resolved;
    check
}

/// Check every saved machine at once, remembering what they tell us.
pub async fn statuses() -> HashMap<String, Check> {
    let machines = list();
    let probes = machines.iter().map(|machine| {
        let (id, address, key) = (
            machine.id.clone(),
            machine.address.clone(),
            machine.key.clone(),
        );
        tokio::spawn(async move { (id, check(&address, &key).await) })
    });
    let mut results = HashMap::new();
    for probe in probes.collect::<Vec<_>>() {
        if let Ok((id, check)) = probe.await {
            results.insert(id, check);
        }
    }
    // Keep what we learned, without rewriting the file every few seconds.
    let changed = machines.iter().any(|machine| {
        results.get(&machine.id).is_some_and(|check| {
            let mut learned = machine.clone();
            learn(&mut learned, check);
            learned.os != machine.os
                || learned.device != machine.device
                || learned.model != machine.model
                || (learned.width, learned.height) != (machine.width, machine.height)
                || learned.last_seen >= machine.last_seen + 60
        })
    });
    if changed {
        let _ = modify(|store| {
            for machine in &mut store.machines {
                if let Some(check) = results.get(&machine.id) {
                    learn(machine, check);
                }
            }
            Ok(())
        });
    }
    results
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
