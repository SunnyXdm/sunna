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
    #[serde(default)]
    pub upgraded: bool,
    #[serde(default)]
    pub identity: Option<String>,
    #[serde(default)]
    pub allow_older: bool,
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
    pub kind: String,
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
    #[serde(default)]
    pub from_link: bool,
    #[serde(default)]
    pub identity: Option<String>,
    #[serde(default)]
    pub allow_older: bool,
    pub name: String,
    pub address: String,
    pub key: String,
    /// From the form's own check, so a new tile shows the right machine
    /// before the first status round.
    #[serde(default)]
    pub about: Option<Check>,
    #[serde(default)]
    pub kind: String,
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
    let mut temporary = tempfile::NamedTempFile::new_in(crate::settings::dir()).map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut temporary, &json).map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary.persist(&path).map_err(|e| e.to_string())?;
    Ok(())
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

fn edit_record(from_link: bool, change: impl FnOnce(&mut Store) -> Result<Machine, String>) -> Result<Machine, String> {
    let _guard = STORE.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = load();
    let machine = change(&mut store)?;
    let records = serde_json::to_string_pretty(&store).map_err(|e| e.to_string())?;
    machine.trust().save_records(&records, from_link).map_err(|e| e.to_string())?;
    Ok(machine)
}

pub fn list() -> Vec<Machine> {
    let _guard = STORE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut machines = load().machines;
    for machine in &mut machines {
        if let Ok(Some(pin)) = machine.trust().pin() {
            machine.upgraded = true;
            machine.identity = Some(pin);
            machine.allow_older = false;
        }
    }
    machines
}

pub fn get(id: &str) -> Option<Machine> {
    list().into_iter().find(|machine| machine.id == id)
}

fn new_id() -> Result<String, String> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

impl Machine {
    pub fn trust(&self) -> sunna_client::auth::Trust {
        sunna_client::auth::Trust { upgraded: self.upgraded, identity: self.identity.clone(), allow_older: self.allow_older, record: Some(self.id.clone()) }
    }
}

fn validate(draft: &Draft) -> Result<(), String> {
    parse(&draft.address)?;
    if let Some(pin) = &draft.identity {
        sunna_client::auth::Trust { identity: Some(pin.clone()), ..Default::default() }.pin().map_err(|e| e.to_string())?;
    }
    if !["", "laptop", "desktop", "mini", "server", "vm"].contains(&draft.kind.as_str()) {
        return Err("Choose a computer kind from the list.".into());
    }
    if draft.key.contains(['\n', '\r']) {
        return Err("The key can't contain line breaks.".into());
    }
    Ok(())
}

fn learn(machine: &mut Machine, check: &Check) {
    if machine.identity.is_none() && check.identity.is_some() {
        machine.upgraded = true;
        machine.identity = check.identity.clone();
        machine.allow_older = false;
    }
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
    edit_record(draft.from_link, |store| {
        if let Some(pin) = draft.identity.as_ref().filter(|_| draft.from_link) {
            if let Some(saved) = store.machines.iter_mut().find(|m| m.address == draft.address.trim()) {
                saved.upgraded = true;
                saved.identity = Some(pin.clone());
                saved.allow_older = false;
                saved.key = draft.key.trim().to_string();
                saved.name = display_name(&draft);
                if let Some(check) = &draft.about { learn(saved, check); }
                return Ok(saved.clone());
            }
        }
        let mut machine = Machine {
            id: new_id()?,
            upgraded: draft.identity.is_some(),
            identity: draft.identity.clone(),
            allow_older: false,
            name: display_name(&draft),
            address: draft.address.trim().to_string(),
            key: draft.key.trim().to_string(),
            os: String::new(),
            device: String::new(),
            kind: draft.kind.clone(),
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
    edit_record(draft.from_link, |store| {
        let machine = store
            .machines
            .iter_mut()
            .find(|machine| machine.id == id)
            .ok_or("That computer is no longer in your list.")?;
        let moved = machine.address != draft.address.trim();
        machine.name = display_name(&draft);
        if let Some(pin) = &draft.identity {
            if !draft.from_link && machine.trust().pin().map_err(|e| e.to_string())?.as_ref().is_some_and(|known| known != pin) {
                return Err("The saved identity changed. Open Edit again.".into());
            }
            machine.upgraded = true;
            machine.identity = Some(pin.clone());
        }
        machine.allow_older = !machine.upgraded && !moved && machine.trust().pin().map_err(|e| e.to_string())?.is_none() && draft.allow_older;
        machine.kind = draft.kind.clone();
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
        if store.machines.iter().any(|machine| machine.id == id) {
            let picture = picture_path(id);
            for path in [picture.with_extension("jpg.tmp"), picture] {
                match std::fs::remove_file(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.to_string()),
                }
            }
        }
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
        let trust = machine.trust();
        tokio::spawn(async move { (id, sunna_client::reach::check_with(&address, &key, &trust).await) })
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
            learned.identity != machine.identity || learned.os != machine.os
                || learned.device != machine.device
                || learned.model != machine.model
                || (learned.width, learned.height) != (machine.width, machine.height)
                || learned.last_seen >= machine.last_seen + 60
        })
    });
    if changed {
        let _ = modify(|store| {
            for machine in &mut store.machines {
                if !machines.iter().any(|prior| prior.id == machine.id && prior.address == machine.address && prior.key == machine.key && prior.identity == machine.identity) { continue; }
                if let Some(check) = results.get(&machine.id) {
                    learn(machine, check);
                }
            }
            Ok(())
        });
    }
    results
}

/// IDs originate in the local store; callers still cannot supply path components.
pub fn picture_path(id: &str) -> std::path::PathBuf {
    let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    crate::settings::dir()
        .join("pictures")
        .join(format!("{safe}.jpg"))
}

/// Forget every kept picture.
pub fn clear_pictures() {
    let _ = std::fs::remove_dir_all(crate::settings::dir().join("pictures"));
}

pub fn picture(id: &str) -> Option<Vec<u8>> {
    get(id)?;
    std::fs::read(picture_path(id)).ok()
}

#[cfg(all(test, unix))]
mod replacement_tests {
    use super::*;
    #[test]
    fn failed_save_keeps_previous_pin() {
        if std::env::var_os("SUNNA_REPLACE_CHILD").is_none() {
            let home = tempfile::tempdir().unwrap();
            assert!(std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "machines::replacement_tests::failed_save_keeps_previous_pin"])
                .env("SUNNA_REPLACE_CHILD", "1").env("HOME", home.path()).status().unwrap().success());
            return;
        }
        let draft = |pin: &str| Draft { identity: Some(pin.into()), from_link: true,
            address: "localhost".into(), key: "fixture".into(), name: "Office".into(),
            kind: String::new(), about: None, allow_older: false };
        let first = "A".repeat(43);
        let next = format!("B{}", "A".repeat(42));
        let record = add(draft(&first)).unwrap();
        let backup = std::fs::read(path()).unwrap();
        let sidecar = crate::settings::dir().join("identities").join(&record.id);
        let old_policy = std::fs::read(&sidecar).unwrap();
        // Make replacement of machines.json fail while its existing contents remain readable.
        let stage = crate::settings::dir().join(format!(".records-{}.tmp", record.id));
        // The directory collision also forces a deterministic write failure
        // for a privileged test runner that can bypass directory modes.
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir(&stage).unwrap();
        std::fs::set_permissions(crate::settings::dir(), std::fs::Permissions::from_mode(0o500)).unwrap();
        let failed = add(draft(&next)).is_err();
        std::fs::set_permissions(crate::settings::dir(), std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::remove_dir(stage).unwrap();
        assert!(failed);
        assert_eq!(record.trust().pin().unwrap(), Some(first.clone()));
        let rotated = add(draft(&next)).unwrap();
        assert_eq!(record.id, rotated.id);
        std::fs::write(path(), backup).unwrap();
        assert!(get(&record.id).unwrap().trust().pin().is_err());
        std::fs::remove_file(&sidecar).unwrap();
        assert_eq!(get(&record.id).unwrap().trust().pin().unwrap(), Some(first.clone()));
        std::fs::write(sidecar, old_policy).unwrap();
        assert_eq!(get(&record.id).unwrap().trust().pin().unwrap(), Some(first));
    }
}
