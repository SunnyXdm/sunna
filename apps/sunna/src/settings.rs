//! Launcher settings. This computer's key (SUNNA_TOKEN) and the log
//! collector live in ~/.sunna/sunna.env, shared with
//! `scripts/run.sh`; viewer preferences live in ~/.sunna/config.json.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// This computer's key: what `run.sh host` shares it with, and the
    /// key the Add Computer form starts with.
    pub key: String,
    pub windowed: bool,
    pub stats: bool,
    pub menu_button: bool,
    /// Play the other computer's sound.
    pub audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Prefs {
    windowed: bool,
    stats: bool,
    menu_button: bool,
    audio: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            windowed: false,
            stats: true,
            menu_button: true,
            audio: true,
        }
    }
}

pub fn dir() -> PathBuf {
    let home = std::env::var_os("HOME")
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
    }
}

pub fn save(settings: &Settings) -> Result<(), String> {
    std::fs::create_dir_all(dir()).map_err(|error| error.to_string())?;
    write_token(settings.key.trim())?;
    let prefs = Prefs {
        windowed: settings.windowed,
        stats: settings.stats,
        menu_button: settings.menu_button,
        audio: settings.audio,
    };
    let json = serde_json::to_vec_pretty(&prefs).map_err(|error| error.to_string())?;
    std::fs::write(prefs_path(), json).map_err(|error| error.to_string())
}

/// Replace (or add) the SUNNA_TOKEN line, keeping everything else in the
/// file as it was; owner-only permissions, as it's a secret.
fn write_token(token: &str) -> Result<(), String> {
    if token.contains(['\n', '\r', '=']) {
        return Err("The key can't contain line breaks or '='.".into());
    }
    let path = env_path();
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
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
    std::fs::write(&path, text).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
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
