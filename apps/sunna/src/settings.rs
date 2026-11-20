//! Launcher settings. This computer's key (SUNNA_TOKEN) and the log
//! collector live in ~/.sunna/sunna.env, shared with
//! `scripts/run.sh`; viewer preferences live in ~/.sunna/config.json.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// This computer's key: what `run.sh host` shares it with, and the
    /// key the Add Computer form starts with.
    #[serde(default)]
    pub key: String,
    pub windowed: bool,
    pub stats: bool,
    pub menu_button: bool,
    /// Play the other computer's sound.
    pub audio: bool,
    #[serde(default)]
    pub view: ComputerView,
    /// What's kept of a computer's screen when a session ends.
    #[serde(default)]
    pub pictures: Pictures,
}

/// Sharp keeps a readable picture; Blurred keeps only the screen's colours.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pictures {
    Off,
    Blurred,
    #[default]
    Sharp,
}

/// Persisted locally. Gallery shows every computer and its state at once.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComputerView {
    Devices,
    Wall,
    Space,
    Type,
    #[default]
    Gallery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Prefs {
    windowed: bool,
    stats: bool,
    menu_button: bool,
    audio: bool,
    view: ComputerView,
    pictures: Pictures,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            windowed: false,
            stats: true,
            menu_button: true,
            audio: true,
            view: ComputerView::default(),
            pictures: Pictures::default(),
        }
    }
}

pub fn dir() -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".sunna")
}

fn env_path() -> PathBuf {
    let path = dir().join("sunna.env");
    // Earlier versions called it dogfood.env.
    let old = dir().join("dogfood.env");
    if !path.exists() && old.exists() {
        let _ = std::fs::rename(&old, &path);
    }
    path
}

fn prefs_path() -> PathBuf {
    dir().join("config.json")
}

/// `KEY=value` pairs from sunna.env (comments and blanks skipped).
pub fn env_vars() -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(env_path()) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| {
            (
                key.trim().to_string(),
                value.trim().trim_matches('"').to_string(),
            )
        })
        .collect()
}

pub fn token() -> String {
    env_vars()
        .into_iter()
        .find(|(key, _)| key == "SUNNA_TOKEN")
        .map(|(_, value)| value)
        .unwrap_or_default()
}

/// Create the sharing credential only after the user enables the host. Stage
/// privately and rename so neither readers nor a crash see a partial key.
#[cfg(target_os = "macos")]
pub fn ensure_host_key() -> anyhow::Result<()> {
    use std::io::Write;
    if !token().is_empty() { return Ok(()); }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| anyhow::anyhow!("System randomness unavailable"))?;
    let key: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    std::fs::create_dir_all(dir())?;
    let path = env_path();
    let previous = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let mut file = tempfile::NamedTempFile::new_in(dir())?;
    for line in previous.lines().filter(|l| !l.trim_start().starts_with("SUNNA_TOKEN=")) {
        writeln!(file,"{line}")?;
    }
    writeln!(file,"SUNNA_TOKEN={key}")?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}

pub fn load() -> Settings {
    let prefs: Prefs = std::fs::read(prefs_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    Settings {
        key: token(),
        windowed: prefs.windowed,
        stats: prefs.stats,
        menu_button: prefs.menu_button,
        audio: prefs.audio,
        view: prefs.view,
        pictures: prefs.pictures,
    }
}

pub fn save(settings: &Settings) -> Result<(), String> {
    std::fs::create_dir_all(dir()).map_err(|error| error.to_string())?;
    let prefs = Prefs {
        windowed: settings.windowed,
        stats: settings.stats,
        menu_button: settings.menu_button,
        audio: settings.audio,
        view: settings.view,
        pictures: settings.pictures,
    };
    let json = serde_json::to_vec_pretty(&prefs).map_err(|error| error.to_string())?;
    std::fs::write(prefs_path(), json).map_err(|error| error.to_string())
}

/// Replace (or add) the SUNNA_TOKEN line, keeping everything else in the
/// file as it was; owner-only permissions, as it's a secret.
pub fn write_token(token: &str) -> Result<(), String> {
    if token.contains(['\n', '\r', '=']) {
        return Err("The key can't contain line breaks or '='.".into());
    }
    let path = env_path();
    std::fs::create_dir_all(dir()).map_err(|error| error.to_string())?;
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.to_string()),
    };
    let mut replaced = false;
    let mut lines: Vec<String> = existing
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("SUNNA_TOKEN=") {
                replaced = true;
                format!("SUNNA_TOKEN={token}")
            } else {
                line.to_string()
            }
        })
        .collect();
    if !replaced {
        lines.push(format!("SUNNA_TOKEN={token}"));
    }
    let mut text = lines.join("\n");
    text.push('\n');
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new_in(dir()).map_err(|e| e.to_string())?;
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Environment for a viewer process: the collector settings from
/// sunna.env plus the viewer preferences.
pub fn viewer_env() -> Vec<(String, String)> {
    let settings = load();
    let mut env: Vec<(String, String)> = env_vars()
        .into_iter()
        .filter(|(key, _)| key.starts_with("SUNNA_"))
        .collect();
    env.push((
        "SUNNA_WINDOWED".into(),
        if settings.windowed { "1" } else { "0" }.into(),
    ));
    env.push((
        "SUNNA_STATS".into(),
        if settings.stats { "1" } else { "0" }.into(),
    ));
    env.push((
        "SUNNA_MENU_BUTTON".into(),
        if settings.menu_button { "1" } else { "0" }.into(),
    ));
    env.push((
        "SUNNA_AUDIO".into(),
        if settings.audio { "1" } else { "0" }.into(),
    ));
    env
}

#[cfg(test)]
mod view_tests {
    use super::*;
    #[test]
    fn preference_save_preserves_generated_key() {
        // Isolate HOME from other Rust tests and from the real launcher.
        if std::env::var_os("SUNNA_TEST_PREFS").is_none() {
            let home = tempfile::tempdir().unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "settings::view_tests::preference_save_preserves_generated_key"])
                .env("HOME", home.path()).env("SUNNA_TEST_PREFS", "1").status().unwrap();
            assert!(status.success());
            return;
        }
        let mut settings = load();
        std::fs::create_dir_all(dir()).unwrap();
        write_token("local-fixture").unwrap();
        settings.stats = !settings.stats;
        save(&settings).unwrap();
        assert!(token() == "local-fixture");
    }

    #[test]
    fn old_preferences_keep_their_values_and_default_to_gallery() {
        let prefs: Prefs = serde_json::from_str(r#"{"windowed":true,"audio":false}"#).unwrap();
        assert!(prefs.windowed);
        assert!(!prefs.audio);
        assert!(matches!(prefs.view, ComputerView::Gallery));
        assert!(matches!(prefs.pictures, Pictures::Sharp));
        for name in ["devices", "wall", "space", "type", "gallery"] {
            let value: ComputerView = serde_json::from_value(serde_json::json!(name)).unwrap();
            assert_eq!(serde_json::to_value(value).unwrap(), name);
        }
    }
}
