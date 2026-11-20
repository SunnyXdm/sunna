use anyhow::{ensure, Result};
use std::process::Command;
use sunna_update::Updater;
fn retry_lock<T>(mut action: impl FnMut() -> Result<T>) -> Result<T> {
    loop {
        match action() {
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::WouldBlock) =>
            {
                std::thread::sleep(std::time::Duration::from_millis(100))
            }
            result => return result,
        }
    }
}
/// launchd launches the installed bundle. Normally exec replaces this
/// launcher immediately. A boot interrupted during activation has no window
/// supervisor to prove the pending release, so perform its bounded trial here.
#[cfg(target_os = "macos")]
fn launch_mac_host(exe: &std::path::Path, args: &[String]) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let mut binary = exe.with_file_name("Sunna-real");
    if args.iter().any(|a| a == "--agent") && exe.starts_with(sunna_update::data_dir().join("app/versions")) {
        let updater = Updater::new("app")?;
        // An open window owns this lock and validates the host via its
        // private control socket before acknowledging the selected version.
        if let Ok(_supervisor) = updater.lock("daemon.lock", false) {
            let selection = (|| -> Result<_> {
                retry_lock(|| updater.recover())?;
                let state = updater.state()?;
                if !state.healthy && state.previous.is_some() {
                    let _idle = updater.wait_for_mac_host_idle()?;
                    let trial = ["supervise", "app", "host", "--agent", "--verify-start"].map(String::from);
                    supervise(&updater, &trial)?;
                }
                let selected = updater.state()?.current.ok_or_else(|| anyhow::anyhow!("No installed app"))?;
                updater.launch_binary(&selected)
            })();
            binary = match selection {
                Ok(binary) => binary,
                Err(error) => {
                    let Ok(good) = std::fs::read_to_string(updater.root.join("known-good")) else { return Err(error) };
                    let verified = updater.launch_binary(&good)?;
                    eprintln!("Host update storage unavailable: {error:#}. Serving the last confirmed release.");
                    return Err(Command::new(verified).args(args).env("SUNNA_RECOVERY_ONLY","1").exec().into());
                }
            };
        }
    }
    Err(Command::new(binary).args(args).exec().into())
}

fn main() -> Result<()> {
    if let Some(result) = sunna_update::publication_probe() {
        return result;
    }
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if cfg!(target_os = "macos")
        && std::env::current_exe()?.canonicalize()?
            .file_name()
            .is_some_and(|n| n == "Sunna")
    {
        #[cfg(target_os = "macos")]
        {
            let exe = std::env::current_exe()?.canonicalize()?;
            if args.first().is_some_and(|arg| arg == "host") {
                return launch_mac_host(&exe, &args);
            }
            if !exe.starts_with(sunna_update::data_dir().join("app/versions")) {
                use std::os::unix::process::CommandExt;
                return Err(Command::new(exe.with_file_name("Sunna-real")).args(&args).exec().into());
            }
        }
        args.splice(0..0, ["supervise".to_string(), "app".to_string()]);
    }
    if args.first().map(String::as_str) == Some("--version") {
        println!("sunna-update {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("verify") {
        ensure!(args.len() == 3, "verify MANIFEST SIGNATURE");
        sunna_update::verify_manifest(&std::fs::read(&args[1])?, &std::fs::read(&args[2])?)?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    if args.first().map(String::as_str) == Some("validate-listen") {
        let choice = args.get(1).ok_or_else(|| anyhow::anyhow!("missing listen choice"))?;
        if !matches!(choice.as_str(), "lan" | "tailscale") {
            let addr: std::net::SocketAddr = choice.parse()?;
            ensure!(addr.port() != 0, "missing listen port");
        }
        let port: u16 = args.get(2).ok_or_else(|| anyhow::anyhow!("missing port"))?.parse()?;
        ensure!(port != 0, "invalid port");
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    if args.first().map(String::as_str) == Some("system-serve") {
        let choice = std::env::var("SUNNA_LISTEN_CHOICE")?;
        let port: u16 = std::env::var("SUNNA_PORT")?.parse()?;
        let ip = match choice.as_str() {
            "tailscale" => {
                let output = Command::new("tailscale").args(["ip", "-4"]).output()?;
                ensure!(output.status.success(), "waiting for Tailscale");
                String::from_utf8(output.stdout)?.lines().next().unwrap_or("").to_owned()
            }
            "lan" => {
                let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
                socket.connect("1.1.1.1:53")?;
                socket.local_addr()?.ip().to_string()
            }
            _ => String::new(),
        };
        let addr: std::net::SocketAddr = if matches!(choice.as_str(), "tailscale" | "lan") {
            std::net::SocketAddr::new(ip.parse()?, port)
        } else { choice.parse()? };
        args[0] = "supervise".into();
        args.extend(["--listen".into(), addr.to_string()]);
    }
    let component = args.get(1).map(String::as_str).unwrap_or("host");
    let updater = Updater::new(component)?;
    match args.first().map(String::as_str).unwrap_or("check") {
        "adopt" => {
            #[cfg(target_os = "macos")]
            let _window = updater.lock("daemon.lock", false)
                .map_err(|_| anyhow::anyhow!("Close the Sunna window and retry. The background host may keep running."))?;
            updater.adopt(std::path::Path::new(args.get(2).ok_or_else(|| anyhow::anyhow!("staging directory required"))?))?; }
        #[cfg(target_os = "linux")]
        "install-system" => {
            ensure!(component == "host" && updater.root == std::path::Path::new("/opt/sunna/host")
                && unsafe { libc::geteuid() } == 0, "invalid system installation");
            let _daemon = updater.lock("daemon.lock", false)?;
            sunna_update::system_install::validate(&updater.root)?;
            updater.prepare()?;
            let _idle = updater.lock("session.lock", false)?;
            updater.activate()?;
            updater.install_recovery()?;
            if args.iter().any(|a| a == "--auto-update") { updater.set_policy("auto")?; }
        }
        #[cfg(target_os = "linux")]
        mode @ ("run-user" | "run-desktop") => {
            use std::os::unix::process::CommandExt;
            let selected = retry_lock(|| updater.recover()).and_then(|_| {
                updater.state()?.current.ok_or_else(|| anyhow::anyhow!("missing installed host"))
            }).or_else(|error| std::fs::read_to_string(updater.root.join("known-good")).map_err(|_| error))?;
            let binary = updater.launch_binary(&selected)?;
            if mode == "run-desktop" {
                let helper = binary.parent().unwrap().parent().unwrap().join("share/sunna/linux-desktop.sh");
                return Err(Command::new(helper).args(&args[2..]).exec().into());
            }
            return Err(Command::new(binary.with_file_name("sunna-host")).arg("run").exec().into());
        }
        "install" => {
            let _daemon = updater.lock("daemon.lock", false).map_err(|_| anyhow::anyhow!(if component == "host" { "Host is running; use sunna-host update." } else { "Sunna is running; quit the app before reinstalling." }))?;
            updater.migrate()?;
            updater.prepare()?;
            #[cfg(target_os = "macos")]
            let _idle = updater.wait_for_mac_host_idle()?;
            #[cfg(not(target_os = "macos"))]
            let _idle = updater.lock("session.lock", false)?;
            updater.activate()?; updater.install_links()?;
            if args.iter().any(|a| a == "--auto-update") { updater.set_policy("auto")?; }
            #[cfg(unix)]
            println!("Installed launcher: {}", std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/bin").join(if component == "host" { "sunna-host" } else { "sunna" }).display());
            println!("Installed {}. {}", component, if component == "host" { "Run sunna-host setup to choose a desktop." } else { "Run sunna to open the app." });
        }
        "running" => { ensure!(updater.state()?.healthy, "host is starting"); }
        "check" => { updater.check()?; println!("{}", serde_json::to_string(&updater.state()?)?); }
        "update" => {
            let daemon = updater.lock("daemon.lock", false);
            if args.iter().any(|a| a == "--check") { updater.check()?; }
            else if daemon.is_err() {
                ensure!(component == "host", "Quit Sunna before using this command, or use Restart to update in the app.");
                if args.iter().any(|a| a == "--rollback") { updater.request_rollback()?; } else { updater.request()?; } println!("Update queued; it will restart when the session ends.");
            } else {
                ensure!(updater.root.join("current").exists(), "Use your system package manager (apt/dnf/pacman), or install a signed download first.");
                #[cfg(target_os = "macos")]
                let _idle = updater.wait_for_mac_host_idle()?;
                #[cfg(not(target_os = "macos"))]
                let _idle = updater.lock("session.lock", false)?;
                if args.iter().any(|a| a == "--rollback") { updater.rollback()?; }
                else { updater.prepare()?; updater.activate()?; }
            }
            println!("{}", serde_json::to_string(&updater.state()?)?);
        }
        "supervise" => {
            if args.get(2).map(String::as_str) == Some("--version") {
                let exe = std::env::current_exe()?;
                let here = exe.parent().unwrap();
                let binary = if cfg!(target_os = "macos") && component == "app" && here.ends_with("Contents/MacOS") {
                    here.join("Sunna-real")
                } else { updater.binary(here.parent().unwrap()) };
                let status = Command::new(binary).arg("--version").status()?;
                ensure!(status.success(), "version check failed"); return Ok(());
            }
            let mut daemon = None;
            for _ in 0..100 {
                if let Ok(lock) = updater.lock("daemon.lock", false) { daemon = Some(lock); break; }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let _daemon = daemon.ok_or_else(|| anyhow::anyhow!("another instance is running"))?;
            return supervise(&updater, &args);
        }
        _ => anyhow::bail!("usage: sunna-update install|check|update|supervise host|app [--check|--yes|--rollback|--auto-update]"),
    }
    // The same trusted supervisor validates a switched bundle even when a
    // source or command-line install explicitly leaves the window closed.
    #[cfg(target_os = "macos")]
    if component == "app" && matches!(args.first().map(String::as_str), Some("adopt" | "install" | "update"))
        && !args.iter().any(|a| a == "--check")
        && std::env::var_os("HOME").is_some_and(|h| std::path::PathBuf::from(h).join("Library/LaunchAgents/dev.sunna.host.plist").is_file())
    {
        let _window = updater.lock("daemon.lock", false)?;
        let selected = updater.state()?.current;
        supervise(&updater, &["supervise".into(),"app".into(),"--reconcile-host".into()])?;
        ensure!(updater.state()?.current == selected, "The updated host failed its startup checks. The previous release was restored.");
    }
    Ok(())
}

fn serve_known_good(updater: &Updater, args: &[String], error: anyhow::Error) -> Result<()> {
    if updater.component != "host" {
        return Err(error);
    }
    let good = match std::fs::read_to_string(updater.root.join("known-good")) {
        Ok(version) => version,
        Err(_) => return Err(error),
    };
    let binary = updater.launch_binary(&good)?;
    eprintln!("Update storage unavailable ({error}). Serving known-good {good} with updates disabled for this run.");
    let status = Command::new(binary)
        .args(args.iter().skip(2))
        .env("SUNNA_SUPERVISED", "1")
        .env("SUNNA_RECOVERY_ONLY", "1")
        .env_remove("SUNNA_READY_FILE")
        .status()?;
    ensure!(status.success(), "recovery host exited");
    Ok(())
}

fn supervise(updater: &Updater, args: &[String]) -> Result<()> {
    if updater.component == "app" && args.get(2).is_none_or(|a| a != "host") && updater.policy()? == "auto" {
        #[cfg(target_os = "macos")]
        if updater.state()?.pending.is_some() {
            let _idle = updater.wait_for_mac_host_idle()?;
            retry_lock(|| updater.activate())?;
        }
        #[cfg(not(target_os = "macos"))]
        retry_lock(|| updater.activate())?;
    }
    loop {
        if let Err(error) = retry_lock(|| updater.starting()) {
            return serve_known_good(updater, args, error);
        }
        let state = updater.state()?;
        let selected = state
            .current
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("no installed version"))?;

        let ready = match (!state.healthy)
            .then(|| tempfile::tempdir_in(&updater.root))
            .transpose()
        {
            Ok(ready) => ready,
            Err(error) => return serve_known_good(updater, args, error.into()),
        };
        let signal = ready.as_ref().map(|d| d.path().join("ready"));
        let child = updater.launch_binary(selected).and_then(|binary| {
            let mut command = Command::new(binary);
            command
                .args(args.iter().skip(2))
                .env("SUNNA_SUPERVISED", "1")
                .env("SUNNA_SELECTED", selected)
                .env_remove("SUNNA_READY_FILE");
            if let Some(path) = &signal {
                command.env("SUNNA_READY_FILE", path);
            }
            Ok(command.spawn()?)
        });
        let mut confirmed = state.healthy;
        let status = if let Ok(mut child) = child {
            let timeout = if cfg!(debug_assertions) {
                std::env::var("SUNNA_TEST_START_MS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60000)
            } else {
                60000
            };
            let interval = std::time::Duration::from_millis(timeout);
            let mut deadline = std::time::Instant::now() + interval;
            let mut last_wait = None;
            let expected = format!("{} {}", child.id(), selected);
            loop {
                if confirmed {
                    break Some(child.wait()?);
                }
                if let Some(status) = child.try_wait()? {
                    break Some(status);
                }
                let signal_text = signal.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
                if cfg!(target_os = "linux") && updater.component == "host" {
                    let progress = signal_text.as_deref().and_then(|s|
                        s.strip_prefix(&format!("{expected} waiting "))).and_then(|s| s.parse::<u64>().ok());
                    if state.previous.is_none() && progress.is_some() && progress > last_wait {
                        deadline = std::time::Instant::now() + interval;
                        last_wait = progress;
                    }
                }
                if signal_text.as_ref() == Some(&expected) {
                    match updater.healthy(selected) {
                        Ok(()) => confirmed = true,
                        // The child is serving. A full disk or a concurrent check must
                        // never cause us to kill it while persisting confirmation.
                        Err(_) => std::thread::sleep(std::time::Duration::from_millis(250)),
                    }
                } else if std::time::Instant::now() >= deadline {
                    child.kill()?;
                    child.wait()?;
                    break None;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        } else {
            None
        };
        if confirmed && status.is_some_and(|s| s.success()) {
            return Ok(());
        }
        #[cfg(target_os = "macos")]
        if updater.component == "app" && confirmed && status.is_some_and(|s| s.code() == Some(75)) {
            let _idle = updater.wait_for_mac_host_idle()?;
            retry_lock(|| updater.activate())?;
        }
        if status.and_then(|s| s.code()) == Some(75)
            && (confirmed || updater.state()?.current.as_deref() != Some(selected)) { continue; }
        if confirmed {
            anyhow::bail!("child exited");
        }
        ensure!(
            updater.state()?.previous.is_some() || updater.state()?.starts < 3,
            "initial installation failed to start"
        );
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
