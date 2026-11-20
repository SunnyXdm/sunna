//! Signed app executable, native host mode. No Tauri builder or web view.
pub mod agent;
pub mod legacy;
mod menu;

use crate::{mac_host_model as model, settings};
use anyhow::{ensure, Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    net::SocketAddr,
    os::unix::{
        fs::{DirBuilderExt, MetadataExt},
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Duration,
};

#[derive(Parser)]
#[command(
    name = "sunna host",
    version,
    about = "Share this Mac until you quit the host"
)]
struct Args {
    #[arg(long, hide = true)]
    agent: bool,
    #[arg(long, hide = true)]
    verify_start: bool,
    #[arg(long, env = "SUNNA_LISTEN")]
    listen: Option<String>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long, default_value_t = 60)]
    fps: u32,
    #[arg(long, default_value = "hevc")]
    codec: String,
    #[arg(long)]
    no_audio: bool,
    #[arg(long)]
    no_clipboard: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub address: SocketAddr,
    pub reach: String,
    pub link: String,
    pub agent: bool,
    pub screen: bool,
    pub input: bool,
    #[serde(default = "unknown_capture")]
    pub capture_reason: Option<String>,
    pub viewer: Option<String>,
    pub executable: PathBuf,
    pub file_id: (u64, u64),
    pub pid: u32,
}
fn unknown_capture() -> Option<String> {
    Some("This host hasn’t reported a screen frame yet.".into())
}

pub fn file_id(path: &std::path::Path) -> Result<(u64, u64)> {
    let m = fs::metadata(path)?;
    Ok((m.dev(), m.ino()))
}
static STATE: OnceLock<Mutex<Snapshot>> = OnceLock::new();

fn local_state() -> Snapshot {
    let mut state = STATE.get().unwrap().lock().unwrap().clone();
    (state.screen, state.input) = permissions();
    state.capture_reason = capture_status(state.screen, state.input).map(str::to_owned);
    state
}

pub fn permissions() -> (bool, bool) {
    (
        sunna_capture::macos::screen_capture_granted(),
        sunna_input::macos::accessibility_granted(),
    )
}
fn unavailable() -> Option<&'static str> {
    let (screen, input) = permissions();
    capture_status(screen, input)
}
// One short capture on demand after grants, with a cooldown after failure.
static HEALTH_CONFIG: OnceLock<sunna_host::HostConfig> = OnceLock::new();
static FRAME: Mutex<model::FrameCheck> = Mutex::new(model::FrameCheck::new());

fn capture_status(screen: bool, input: bool) -> Option<&'static str> {
    let mut check = FRAME.lock().unwrap();
    if let Some(config) = HEALTH_CONFIG.get() {
        if let Some(generation) = check.begin(screen && input, std::time::Instant::now()) {
            std::thread::spawn(move || {
                let success = sunnad::macos::check_frame(config).is_ok();
                FRAME
                    .lock()
                    .unwrap()
                    .complete(generation, success, std::time::Instant::now());
            });
        }
    }
    check.reason(screen, input)
}

pub fn instance_running() -> Result<bool> {
    let path = control_dir()?.join("instance.lock");
    let file = match fs::OpenOptions::new().write(true).open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    match fs2::FileExt::try_lock_exclusive(&file) {
        Ok(()) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(true),
        Err(e) => Err(e.into()),
    }
}

fn viewer_changed(viewer: Option<&str>) {
    STATE.get().unwrap().lock().unwrap().viewer = viewer.map(str::to_owned);
    menu::refresh();
}

fn control_dir() -> Result<PathBuf> {
    let dir = settings::dir().join("mac-host");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;
    let meta = fs::symlink_metadata(&dir)?;
    ensure!(
        meta.is_dir() && meta.uid() == unsafe { libc::geteuid() } && meta.mode() & 0o077 == 0,
        "Make the mac-host directory private to this account."
    );
    Ok(dir)
}

pub fn request(command: u8) -> Result<Vec<u8>> {
    let mut socket = UnixStream::connect(control_dir()?.join("control"))?;
    socket.set_read_timeout(Some(Duration::from_secs(if command == b'H' {
        15
    } else {
        2
    })))?;
    socket.write_all(&[command])?;
    let mut response = Vec::new();
    socket.take(65536).read_to_end(&mut response)?;
    Ok(response)
}
pub fn snapshot() -> Option<Snapshot> {
    serde_json::from_slice(&request(b'S').ok()?).ok()
}

fn control(listener: UnixListener, config: sunna_host::HostConfig) {
    for incoming in listener.incoming() {
        let Ok(mut socket) = incoming else { break };
        let _ = socket.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = socket.set_write_timeout(Some(Duration::from_secs(2)));
        let mut command = [0];
        if socket.read_exact(&mut command).is_err() {
            continue;
        }
        match command[0] {
            b'S' => {
                let _ = serde_json::to_writer(&mut socket, &local_state());
            }
            b'P' => dispatch2::DispatchQueue::main().exec_async(|| {
                sunna_capture::macos::ensure_screen_capture_access();
            }),
            b'I' => dispatch2::DispatchQueue::main().exec_async(|| {
                sunna_input::macos::request_accessibility();
            }),
            b'Q' => std::process::exit(0),
            b'H' => {
                let result = sunnad::macos::check_pipeline(&config);
                let answer = result.map_or_else(
                    |e| format!("{e:#}"),
                    |_| format!("ready {}", std::process::id()),
                );
                let _ = socket.write_all(answer.as_bytes());
            }
            _ => {}
        }
    }
}

use sunna_client::reach::sharing_link as link;

pub async fn tailscale_ip() -> Option<std::net::Ipv4Addr> {
    // A stalled local Tailscale service must not hold up the login agent.
    let mut cli = tokio::process::Command::new(crate::discovery::tailscale());
    cli.args(["ip", "-4"]).kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(3), cli.output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let ip = std::str::from_utf8(&output.stdout)
        .ok()?
        .lines()
        .next()?
        .trim()
        .parse()
        .ok()?;
    std::net::UdpSocket::bind((ip, 0)).ok()?;
    Some(ip)
}

pub fn last_start_error() -> Option<String> {
    fs::read_to_string(control_dir().ok()?.join("start-error")).ok()
}
pub fn record_start_error(error: &anyhow::Error) {
    if let Ok(dir) = control_dir() {
        let _ = fs::write(dir.join("start-error"), format!("{error:#}"));
    }
}
pub fn clear_start_error() -> Result<()> {
    match fs::remove_file(control_dir()?.join("start-error")) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

fn identity_path() -> Result<PathBuf> {
    Ok(PathBuf::from(std::env::var_os("HOME").context("Missing home directory.")?).join("Library/Application Support/Sunna/host.identity"))
}

fn lock_instance() -> Result<fs::File> {
    let instance = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(control_dir()?.join("instance.lock"))?;
    fs2::FileExt::try_lock_exclusive(&instance)
        .context("Another Sunna host is already running.")?;
    Ok(instance)
}

pub fn run() -> Result<()> {
    if std::env::args().nth(2).as_deref() == Some("new-identity") {
        let _instance = lock_instance()?;
        sunna_transport::HostIdentity::rotate(&identity_path()?)?;
        println!("Every device must add this computer again with its new link.");
        return Ok(());
    }
    let args = Args::parse_from(std::env::args_os().skip(1));
    if args.verify_start && std::env::var_os("SUNNA_READY_FILE").is_none() {
        // Supervisor rolled back to an already confirmed version.
        return Ok(());
    }
    let _instance = lock_instance()?;
    let _log = sunna_telemetry::init("host", None);
    if args.agent {
        agent::set_paused(false)?;
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let tail = runtime.block_on(tailscale_ip());
    // UDP connect only asks the routing table which local interface it would use.
    let lan = (|| {
        let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
        socket.connect("192.0.2.1:9").ok()?;
        socket.local_addr().ok()?.ip().to_string().parse().ok()
    })();
    let setting = args.listen.or_else(|| {
        settings::env_vars()
            .into_iter()
            .find(|(name, _)| name == "SUNNA_LISTEN")
            .map(|(_, v)| v)
    });
    let (address, reach) =
        model::listen_setting(setting.as_deref(), tail, lan).map_err(anyhow::Error::msg)?;
    if !args.agent && std::env::var_os("SUNNA_TOKEN").is_none() {
        settings::ensure_host_key()?;
    }
    let token = if args.agent {
        settings::token()
    } else {
        std::env::var("SUNNA_TOKEN").unwrap_or_else(|_| settings::token())
    };
    let config = sunnad::macos::config(
        args.name.unwrap_or_else(crate::computer_name),
        token.clone(),
        args.codec,
        args.fps,
        !args.no_audio,
        !args.no_clipboard,
    )?;
    let _ = HEALTH_CONFIG.set(config.clone());
    let _enter = runtime.enter();
    let host_identity = sunna_transport::HostIdentity::load_or_create(&identity_path()?)?;
    let identity = host_identity.pin()?;
    let server = sunna_transport::Server::with_identity(address, host_identity).with_context(|| {
        format!(
            "Cannot listen on {address}. Check that the address is available and the port is free."
        )
    })?;
    let address = server.local_addr()?;
    let path = control_dir()?.join("control");
    if args.agent {
        clear_start_error()?;
    }
    if path.exists() {
        fs::remove_file(&path)?;
    }
    let control_socket = UnixListener::bind(&path)?;
    STATE
        .set(Mutex::new(Snapshot {
            address,
            reach: reach.into(),
            link: link(address, &token, Some(&identity)),
            capture_reason: None,
            viewer: None,
            agent: args.agent,
            screen: false,
            input: false,
            file_id: file_id(&std::env::current_exe()?)?,
            executable: std::env::current_exe()?.canonicalize()?,
            pid: std::process::id(),
        }))
        .ok()
        .context("Host already initialized")?;
    sunna_host::macos::register(sunna_host::macos::DesktopEvents {
        capture_failed: || {
            FRAME.lock().unwrap().failed(std::time::Instant::now());
        },
        viewer_changed,
        unavailable,
    });
    let handle = runtime.handle().clone();
    menu::run(move || {
        if args.verify_start {
            let trial = config.clone();
            std::thread::spawn(move || {
                let result =
                    sunnad::macos::check_pipeline(&trial).and_then(|_| agent::confirm_update());
                if let Err(e) = result {
                    eprintln!("Host validation failed: {e:#}");
                    std::process::exit(1);
                }
                std::process::exit(0);
            });
        }
        let cli = PathBuf::from(crate::discovery::tailscale());
        let installed = cli.is_file()
            || std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .any(|dir| dir.join(&cli).is_file());
        if model::retry_tail(setting.as_deref(), reach, installed) {
            let endpoint = server.endpoint.clone();
            handle.spawn(async move {
                let mut delay = 5;
                loop {
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                    delay = (delay * 2).min(120);
                    let Some(ip) = tailscale_ip().await else {
                        continue;
                    };
                    let Ok(updater) = sunna_update::Updater::new("app") else {
                        continue;
                    };
                    let Ok(_idle) = updater.lock("session.lock", false) else {
                        continue;
                    };
                    let next = SocketAddr::new(ip.into(), address.port());
                    let Ok(socket) = std::net::UdpSocket::bind(next) else {
                        continue;
                    };
                    if endpoint.rebind(socket).is_ok() {
                        let mut state = STATE.get().unwrap().lock().unwrap();
                        state.address = next;
                        state.reach = "tailscale".into();
                        state.link = link(next, &token, Some(&identity));
                        break;
                    }
                }
            });
        }
        let health = config.clone();
        std::thread::spawn(move || control(control_socket, health));
        handle.spawn(async move {
            if let Err(error) = sunnad::macos::serve(server, config).await {
                tracing::error!(%error, "host stopped");
            }
            std::process::exit(1);
        });
        tracing::info!(%address, "Mac host listening");
    })
}
