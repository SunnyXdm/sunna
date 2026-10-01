//! Computers you've added: where they are, their key, and what they last
//! told us about themselves. Stored in ~/.sunna/machines.json, owner-only
//! (the keys are secrets).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub use sunna_client::reach::{check, parse, resolve, Check};

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
