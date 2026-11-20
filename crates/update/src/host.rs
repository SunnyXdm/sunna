//! Integration called only after the daemon has bound its listener and built
//! its encoder. Detached worker does blocking I/O outside Tokio's runtime.
use crate::*;
pub fn start() { start_after(|_| Ok(())); }

/// Keep update probation pending until the selected host has exercised its backends.
pub fn start_after(verify: impl FnOnce(&Updater) -> Result<()> + Send + 'static) {
    let Ok(updater) = Updater::new("host") else {
        return;
    };
    if !updater.is_managed_process() {
        return;
    }
    std::thread::spawn(move || {
        let _daemon = if std::env::var_os("SUNNA_SUPERVISED").is_none() {
            updater.lock("daemon.lock", false).ok()
        } else {
            None
        };
        let checking = updater.clone();
        std::thread::spawn(move || {
            if let Err(error) = verify(&checking).and_then(|_| checking.signal_ready()) {
                eprintln!("Sunna startup: {error:#}");
            }
        });
        let mut clock = None;
        loop {
            if let Ok(state) = updater.state() {
                if state.healthy || matches!(state.request.as_deref(), Some("rollback" | "update")) {
                    if let Err(error) = tick(&updater, &mut clock) {
                        if !updater.is_managed_process()
                            && std::env::var_os("SUNNA_SUPERVISED").is_some()
                        {
                            std::process::exit(75);
                        }
                        if !error
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|e| e.kind() == std::io::ErrorKind::WouldBlock)
                        {
                            let _ =
                                updater.report_result(state.request_id, Some(format!("{error:#}")));
                        }
                        eprintln!("Sunna update: {error:#}");
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}
fn tick(updater: &Updater, clock: &mut Option<(u64, std::time::Instant)>) -> Result<()> {
    if updater.state()?.request.as_deref() == Some("rollback") {
        let Ok(_idle) = updater.lock("session.lock", false) else {
            return Ok(());
        };
        updater.rollback()?;
        return restart(updater);
    }
    let requested = updater.state()?.request.as_deref() == Some("update");
    let policy = updater.policy()?;
    let mut state = updater.state()?;
    if policy != "off" && updater.due_in_process(state.last_check, clock) {
        let result = updater.check();
        state = updater.state()?;
        *clock = Some((state.last_check, std::time::Instant::now()));
        result?;
    }
    if state.pending.is_none() && (requested || policy == "auto" && state.available.is_some()) {
        // A bad release must not be downloaded repeatedly every five seconds.
        updater.prepare()?;
        state = updater.state()?;
    }
    if state.pending.is_some() && (requested || policy == "auto") {
        let Ok(_idle) = updater.lock("session.lock", false) else {
            return Ok(());
        };
        updater.activate()?;
        return restart(updater);
    }
    if requested && state.pending.is_none() {
        updater.report_result(state.request_id, None)?;
    }
    Ok(())
}

fn restart(updater: &Updater) -> Result<()> {
    if std::env::var_os("SUNNA_SUPERVISED").is_some() {
        std::process::exit(75);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = Command::new(updater.binary(&updater.root.join("current")))
            .args(std::env::args_os().skip(1))
            .exec();
        return Err(error.into());
    }
    #[cfg(windows)]
    {
        Command::new(updater.binary(&updater.root.join("current")))
            .args(std::env::args_os().skip(1))
            .spawn()?;
        std::process::exit(0);
    }
}
