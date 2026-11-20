//! All launchctl operations are in this user's graphical login domain.
use super::{model, snapshot};
use anyhow::{ensure, Context, Result};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

fn home() -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os("HOME").context("No home directory")?,
    ))
}
pub fn path() -> Result<PathBuf> {
    Ok(home()?.join("Library/LaunchAgents/dev.sunna.host.plist"))
}
pub fn paused() -> bool {
    super::control_dir().is_ok_and(|dir| dir.join("paused").exists())
}
pub fn set_paused(value: bool) -> Result<()> {
    let path = super::control_dir()?.join("paused");
    if value {
        fs::write(path, [])?;
    } else if let Err(e) = fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(e.into());
        }
    }
    Ok(())
}
pub fn enabled() -> bool {
    path().is_ok_and(|p| p.is_file())
}

pub fn executable() -> Result<PathBuf> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let updater = sunna_update::Updater::new("app")?;
    if updater.is_managed_process() {
        // A version-specific path would stop working after old releases are pruned.
        return Ok(updater.binary(&updater.root.join("current")));
    }
    Ok(exe)
}

fn domain() -> String {
    format!("gui/{}", unsafe { libc::geteuid() })
}
fn service() -> String {
    format!("{}/{}", domain(), model::LABEL)
}
fn ctl(args: &[&str]) -> Result<()> {
    let output = Command::new("/bin/launchctl").args(args).output()?;
    ensure!(
        output.status.success(),
        "launchctl {} failed: {}",
        args[0],
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}
pub fn loaded() -> bool {
    ctl(&["print", &service()]).is_ok()
}
fn definition() -> Result<String> {
    let log = home()?.join("Library/Logs/Sunna/host.stderr");
    fs::create_dir_all(log.parent().unwrap())?;
    let exe = executable()?;
    // The bundled launcher's host branch execs the app in place. Pin the
    // installed launcher so it can recover even an interrupted current flip.
    let launcher = if exe.file_name().is_some_and(|n| n == "Sunna-real") {
        exe.canonicalize()?.with_file_name("Sunna")
    } else {
        exe
    };
    Ok(model::plist(
        launcher.to_str().context("App path must be UTF-8")?,
        log.to_str().context("Log path must be UTF-8")?,
    ))
}
fn write_definition(file: &std::path::Path, xml: &str) -> Result<()> {
    fs::create_dir_all(file.parent().unwrap())?;
    let mut staged = tempfile::NamedTempFile::new_in(file.parent().unwrap())?;
    staged.write_all(xml.as_bytes())?;
    staged.as_file().sync_all()?;
    staged.persist(file)?;
    fs::File::open(file.parent().unwrap())?.sync_all()?;
    Ok(())
}
pub fn install() -> Result<()> {
    set_paused(false)?;
    super::clear_start_error()?;
    let file = path()?;
    let xml = definition()?;
    if fs::read_to_string(&file).ok().as_ref() != Some(&xml) {
        write_definition(&file, &xml)?;
        if loaded() {
            ctl(&["bootout", &service()])?;
        }
    }
    if loaded() {
        ctl(&["kickstart", &service()])
    } else {
        ctl(&[
            "bootstrap",
            &domain(),
            file.to_str().context("Invalid LaunchAgent path")?,
        ])
    }
}

pub fn remove() -> Result<()> {
    let file = path()?;
    // bootout can terminate the caller when invoked from its own menu.
    match fs::remove_file(&file) {
        Ok(()) => fs::File::open(file.parent().unwrap())?.sync_all()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    if loaded() {
        ctl(&["bootout", &service()])?;
    }
    Ok(())
}

pub fn wait_started() -> Result<super::Snapshot> {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some(s) = snapshot() {
            return Ok(s);
        }
        if let Some(reason) = super::last_start_error() {
            anyhow::bail!(reason);
        }
        ensure!(Instant::now() < deadline, "Host did not start. Check the network address and port, then see ~/Library/Logs/Sunna/host.stderr.");
        std::thread::sleep(Duration::from_millis(120));
    }
}

/// Some TCC grants become effective only in a new process. Retry once
/// after both grants are visible in the window and a host still lacks one.
pub fn refresh_grants(host: &super::Snapshot) -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static RETRIED: AtomicBool = AtomicBool::new(false);
    if !host.agent || host.screen && host.input || super::permissions() != (true, true) {
        RETRIED.store(false, Ordering::Release);
    } else if host.viewer.is_none() && !RETRIED.load(Ordering::Acquire) {
        let u = sunna_update::Updater::new("app")?;
        if let (Ok(_manage), Ok(_idle)) = (
            u.lock("mac-host-management.lock", false),
            u.lock("session.lock", false),
        ) {
            if enabled() && !paused() && !RETRIED.swap(true, Ordering::AcqRel) {
                ctl(&["kickstart", "-k", &service()])?;
            }
        }
    }
    Ok(())
}

// Caller holds the management lock. Refreshing a paused plist never loads
// the job, including when a foreground host happens to be serving.
fn refresh_paused() -> Result<bool> {
    if !enabled() {
        return Ok(true);
    }
    if !paused() {
        return Ok(false);
    }
    let file = path()?;
    let desired = definition()?;
    if model::paused_update(&fs::read_to_string(&file)?, &desired, true) {
        write_definition(&file, &desired)?;
    }
    Ok(true)
}

// Only at window launch / update. The session lock also gates admission so
// a viewer cannot arrive between observing an empty slot and bootout.
pub fn reconcile(candidate: bool, window: bool) -> Result<()> {
    if !enabled() {
        return Ok(());
    }
    let file_changed = fs::read_to_string(path()?)? != definition()?;
    let current = executable()?.canonicalize()?;
    let state = snapshot();
    if paused() {
        let updater = sunna_update::Updater::new("app")?;
        let _operation = updater.lock("mac-host-management.lock", false)?;
        if refresh_paused()? {
            return Ok(());
        }
    }
    if state.as_ref().is_some_and(|s| !s.agent) {
        return Ok(());
    }
    let id = super::file_id(&current)?;
    let matches = |s: &super::Snapshot| s.executable == current && s.file_id == id;
    let changed = file_changed || !state.as_ref().is_some_and(matches);
    let viewer = state.as_ref().is_some_and(|s| s.viewer.is_some());
    match model::restart(changed, viewer) {
        model::Restart::Keep if !candidate => return Ok(()),
        model::Restart::Wait if window && !candidate => {
            // Closing the window must not discard a pending bundle move.
            Command::new(&current)
                .arg("--reconcile-host")
                .env_remove("SUNNA_READY_FILE")
                .spawn()?;
            return Ok(());
        }
        _ => {}
    }
    let updater = sunna_update::Updater::new("app")?;
    let _idle = updater.wait_for_mac_host_idle()?;
    if !enabled() {
        return Ok(());
    }
    let _operation = updater.lock("mac-host-management.lock", false)?;
    if refresh_paused()? {
        return Ok(());
    }
    let file_changed = fs::read_to_string(path()?)? != definition()?;
    if file_changed || !snapshot().as_ref().is_some_and(matches) {
        if !file_changed && loaded() {
            ctl(&["bootout", &service()])?;
        }
        install()?;
    }
    let s = wait_started()?;
    ensure!(matches(&s), "Host started from another bundle");
    if candidate {
        ensure!(
            super::request(b'H')? == format!("ready {}", s.pid).as_bytes(),
            "Updated host could not capture a frame and initialize input."
        );
    }
    Ok(())
}

/// Invoked by the already-running updater for a windowless installation.
/// Stay alive until the supervisor has committed our PID/version receipt.
pub fn validate_update() -> Result<()> {
    let candidate = std::env::var_os("SUNNA_READY_FILE").is_some();
    reconcile(candidate, false)?;
    confirm_update()
}

pub fn confirm_update() -> Result<()> {
    if std::env::var_os("SUNNA_READY_FILE").is_some() {
        let updater = sunna_update::Updater::new("app")?;
        updater.signal_ready()?;
        while !updater.state()?.healthy {
            ensure!(
                updater.is_managed_process(),
                "Selected app changed during verification"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(())
}
