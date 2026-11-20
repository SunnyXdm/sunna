//! sunnad — the headless Sunna host daemon.
//!
//! `--source screen` streams the real display: on macOS (requires the Screen
//! Recording permission for the process that launches sunnad) or a portal (Wayland) or X11
//! display on Linux. `--source synthetic` streams a test pattern.
//! Input follows the same desktop through Accessibility, the portal, or XTEST.
//!
//! A persistent TLS identity and a connection-bound shared-key proof protect
//! viewer admission. Deployed clients can use the legacy token when allowed.

mod about;
#[cfg(target_os = "linux")]
mod linux_desktop;
#[cfg(target_os = "linux")]
mod linux_machine;
#[cfg(target_os = "linux")]
mod linux_seat;
#[cfg(target_os = "linux")]
mod linux_worker;
#[cfg(target_os = "windows")]
mod console_session;
#[cfg(target_os = "windows")]
mod service;
#[cfg(any(windows, test))]
mod service_args;

use std::io::{IsTerminal, Read, Write};
use std::net::SocketAddr;

use clap::{Parser, ValueEnum};
use sunna_capture::{FrameSource, SyntheticSource};
use sunna_codec::{default_codec_name, make_encoder};
use sunna_host::{run_host_with_desktop, HostConfig};
use sunna_input::make_injector;
use sunna_transport::Server;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum Source {
    Synthetic,
    /// Capture the screen: the main display on macOS, the portal on Wayland,
    /// or the X11 display in $DISPLAY.
    Screen,
}

#[derive(Parser, Debug)]
#[command(name = "sunnad", version, about = "Sunna headless host daemon")]
struct Args {
    /// Share the active local seat from the root-owned system installation.
    #[cfg(target_os = "linux")]
    #[arg(long)]
    system_host: bool,
    /// Approve Wayland desktop sharing at this computer, save permission, and exit.
    #[arg(long)]
    authorize: bool,
    /// Print the text on standard input (a sunna:// link) as a QR code and
    /// exit. It's read from there, not given here, so the key in a link
    /// doesn't show in the list of processes.
    #[arg(long)]
    qr: bool,
    /// Windows, running as SYSTEM (a service): run the host as SYSTEM in
    /// the session at the PC's console, so it can show and control the lock
    /// screen, sign-in and UAC prompts. The other arguments go to that host.
    #[cfg(target_os = "windows")]
    #[arg(long, hide = true)]
    console_session: bool,
    /// Windows: run as the SunnaHost service (started by Windows).
    #[cfg(target_os = "windows")]
    #[arg(long, hide = true)]
    service: bool,
    #[cfg(windows)]
    #[arg(long, hide = true)]
    service_key_store: bool,
    /// Windows: serve the clipboard to the host, as the signed-in user (the
    /// host, as SYSTEM, can't see what they copy). Started by the host.
    #[cfg(target_os = "windows")]
    #[arg(long, hide = true)]
    clipboard_helper: bool,
    /// Windows: register sunnad, with the other arguments, as a service that
    /// starts with Windows, and start it (as administrator).
    #[cfg(target_os = "windows")]
    #[arg(long)]
    install_service: bool,
    /// Windows: stop and remove the service.
    #[cfg(target_os = "windows")]
    #[arg(long)]
    uninstall_service: bool,
    /// Windows: show each viewer Sunna's own display, made at the viewer's
    /// size and refresh rate (needs the Sunna Display driver). For a PC with
    /// no monitor, and sharper than scaling down a monitor's picture.
    #[cfg(target_os = "windows")]
    #[arg(long)]
    virtual_display: bool,
    /// Address to listen on. Loopback by default; beyond loopback, viewers
    /// must present the token (--token).
    #[arg(long, default_value = "127.0.0.1:48800")]
    listen: SocketAddr,
    #[arg(long, value_enum, default_value_t = Source::Synthetic)]
    source: Source,
    /// Stream width. Default: 640 for synthetic, native display width for screen.
    #[arg(long)]
    width: Option<u32>,
    /// Stream height. Default: 360 for synthetic, native display height for screen.
    #[arg(long)]
    height: Option<u32>,
    #[arg(long, default_value_t = 60)]
    fps: u32,
    /// Codec: "hevc" or "h264". VideoToolbox on macOS; on Linux, NVENC with an
    /// NVIDIA GPU, otherwise H.264 on the CPU (OpenH264). "raw" is for tests.
    #[arg(long, env = "SUNNA_CODEC", default_value = default_codec_name())]
    codec: String,
    /// Encoder target bitrate in kilobits per second.
    /// Default: 40000 for screen capture (retina resolutions need it), 20000 synthetic.
    #[arg(long)]
    bitrate_kbps: Option<u32>,
    /// Dev-only: fraction of media datagrams to drop (0.0..1.0) to exercise
    /// FEC and keyframe recovery.
    #[arg(long, default_value_t = 0.0)]
    simulate_loss: f64,
    /// Host name announced to clients.
    #[arg(long, default_value = "sunnad")]
    name: String,
    /// Sharing key. Without one, the daemon creates a private key beside its identity.
    #[arg(long, env = "SUNNA_TOKEN", default_value = "", hide_env_values = true)]
    token: String,
    /// Disable clipboard sharing with the viewer.
    #[arg(long)]
    no_clipboard: bool,
    /// Don't share this machine's sound. SUNNA_AUDIO_SOURCE picks what to
    /// share on Linux (a PulseAudio source; default: what's playing).
    #[arg(long)]
    no_audio: bool,
}

fn resolve_dimensions(args: &Args) -> anyhow::Result<(u32, u32)> {
    match args.source {
        Source::Synthetic => Ok((args.width.unwrap_or(640), args.height.unwrap_or(360))),
        Source::Screen => {
            #[cfg(target_os = "macos")]
            {
                anyhow::ensure!(
                    sunna_capture::macos::ensure_screen_capture_access(),
                    "Screen Recording permission is not granted. Enable it in System Settings → \
                     Privacy & Security → Screen Recording for the app that launched sunnad \
                     (e.g. your terminal), then run again."
                );
                for line in sunna_capture::macos::describe_displays() {
                    tracing::info!("{line}");
                }
                if !sunna_input::macos::request_accessibility() {
                    tracing::error!(
                        "Accessibility is NOT granted: the viewer's mouse and keyboard will be \
                         ignored. macOS should have opened a prompt; enable your terminal in \
                         System Settings → Privacy & Security → Accessibility, then quit and \
                         reopen the terminal and run again."
                    );
                }
                let (native_w, native_h) = sunna_capture::macos::main_display_pixel_size();
                // Even dimensions for the encoder; must match what ScreenSource uses.
                Ok((
                    args.width.unwrap_or(native_w).max(2) & !1,
                    args.height.unwrap_or(native_h).max(2) & !1,
                ))
            }
            #[cfg(target_os = "linux")]
            {
                let (native_w, native_h) = if sunna_portal::use_portal() {
                    sunna_portal::session::display_size().unwrap_or((1920, 1080))
                } else {
                    sunna_capture::linux::main_display_pixel_size()?
                };
                Ok((
                    args.width.unwrap_or(native_w).max(2) & !1,
                    args.height.unwrap_or(native_h).max(2) & !1,
                ))
            }
            #[cfg(target_os = "windows")]
            {
                if args.virtual_display {
                    anyhow::ensure!(
                        sunna_capture::windows_display::available(),
                        "--virtual-display needs the Sunna Display driver (drivers/windows/sunna-display)"
                    );
                    // Each viewer gets a display of its own size; this is
                    // the size for viewers that don't say.
                    return Ok((
                        args.width.unwrap_or(1920).max(640) & !1,
                        args.height.unwrap_or(1080).max(480) & !1,
                    ));
                }
                for info in sunna_capture::windows::displays()? {
                    tracing::info!(display = ?info, "found");
                }
                let (native_w, native_h) = sunna_capture::windows::main_display_pixel_size()?;
                Ok((
                    args.width.unwrap_or(native_w).max(2) & !1,
                    args.height.unwrap_or(native_h).max(2) & !1,
                ))
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
            {
                anyhow::bail!("--source screen is only supported on macOS, Linux and Windows so far")
            }
        }
    }
}

/// `virtual_display`: Windows' `--virtual-display` (ignored elsewhere).
fn make_source_factory(source: Source, virtual_display: bool) -> sunna_host::SourceFactory {
    #[cfg(not(target_os = "windows"))]
    let _ = virtual_display;
    match source {
        Source::Synthetic => Box::new(move |stream| {
            Ok(Box::new(SyntheticSource::new(
                stream.width,
                stream.height,
                stream.fps,
            )) as Box<dyn FrameSource>)
        }),
        Source::Screen => {
            #[cfg(target_os = "macos")]
            {
                // CGDisplayStream scales on the GPU to the session's size.
                Box::new(move |stream| {
                    Ok(Box::new(sunna_capture::macos::ScreenSource::new(
                        Some(stream.width),
                        Some(stream.height),
                        stream.fps,
                        // Raw video also needs CPU-readable BGRA pixels.
                        stream.fast_lane || stream.codec == "raw",
                    )?) as Box<dyn FrameSource>)
                })
            }
            #[cfg(target_os = "linux")]
            {
                if sunna_portal::use_portal() {
                    return Box::new(move |stream| {
                        Ok(Box::new(sunna_portal::PortalSource::new(
                            Some(stream.width),
                            Some(stream.height),
                            stream.fps,
                        )?) as Box<dyn FrameSource>)
                    });
                }
                Box::new(move |stream| {
                    Ok(Box::new(sunna_capture::linux::ScreenSource::new(
                        Some(stream.width),
                        Some(stream.height),
                        stream.fps,
                    )?) as Box<dyn FrameSource>)
                })
            }
            #[cfg(target_os = "windows")]
            {
                Box::new(move |stream| {
                    if virtual_display {
                        show_virtual_display(stream.width, stream.height, stream.fps)?;
                    }
                    // NVENC takes the capture's NV12 textures; everything
                    // else (raw video, the fast lane, software H.264) needs
                    // BGRA in memory.
                    let gpu = !stream.fast_lane
                        && match stream.codec.as_str() {
                            "hevc" => sunna_codec::nvenc::supports(sunna_codec::Codec::Hevc),
                            "h264" => {
                                std::env::var("SUNNA_NVENC").as_deref() != Ok("0")
                                    && sunna_codec::nvenc::supports(sunna_codec::Codec::H264)
                            }
                            _ => false,
                        };
                    Ok(Box::new(sunna_capture::windows::ScreenSource::new(
                        Some(stream.width),
                        Some(stream.height),
                        stream.fps,
                        !gpu,
                    )?) as Box<dyn FrameSource>)
                })
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
            {
                unreachable!("rejected in resolve_dimensions")
            }
        }
    }
}

fn make_injector_factory(_source: Source) -> sunna_host::InjectorFactory {
    #[cfg(target_os = "linux")]
    if _source == Source::Screen && sunna_portal::use_portal() {
        return Box::new(|| match sunna_portal::PortalInjector::new() {
            Ok(injector) => Box::new(injector),
            Err(error) => {
                tracing::warn!(%error, "portal input unavailable");
                Box::new(sunna_input::LogInjector)
            }
        });
    }
    Box::new(make_injector)
}

/// The host identity belongs to the account or system installation.
fn identity_path(system: bool) -> anyhow::Result<std::path::PathBuf> {
    use std::path::PathBuf;
    if system { return Ok(PathBuf::from("/etc/sunna/host.identity")); }
    #[cfg(windows)]
    let dir = PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or_else(|| anyhow::anyhow!("Missing local application directory."))?).join("Sunna");
    #[cfg(target_os = "macos")]
    let dir = PathBuf::from(std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("Missing home directory."))?)
        .join("Library/Application Support/Sunna");
    #[cfg(all(unix, not(target_os = "macos")))]
    let dir = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or_else(|| anyhow::anyhow!("Missing host configuration directory."))?.join("sunna");
    Ok(dir.join("host.identity"))
}

fn main() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some("check-x11") {
        let (width, height) = linux_desktop::check(None, None).map_err(anyhow::Error::msg)?;
        println!("{width}x{height}");
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some("--session-worker") {
        return linux_worker::run(&std::env::args().nth(2).ok_or_else(|| anyhow::anyhow!("missing session"))?);
    }
    if std::env::args().nth(1).as_deref() == Some("validate-identity") {
        let path = std::env::args_os().nth(2).ok_or_else(|| anyhow::anyhow!("Missing identity file."))?;
        sunna_transport::HostIdentity::load(std::path::Path::new(&path))?;
        return Ok(());
    }
    if let Some(action @ ("identity" | "new-identity")) = std::env::args().nth(1).as_deref() {
        let path = identity_path(std::env::args().nth(2).as_deref() == Some("--system-host"))?;
        if action == "new-identity" {
            sunna_transport::HostIdentity::rotate(&path)?;
            println!("Every device must add this computer again with its new link.");
        } else {
            println!("{}", sunna_transport::HostIdentity::load_or_create(&path)?.pin()?);
        }
        return Ok(());
    }
    if let Some(result) = sunna_update::publication_probe() { return result; }
    if std::env::args().nth(1).as_deref() == Some("update") {
        let helper = std::env::current_exe()?.with_file_name(format!("sunna-update{}", std::env::consts::EXE_SUFFIX));
        anyhow::ensure!(helper.exists(), "This source or system-package install has no updater. Use apt/dnf/pacman, or install a signed download first.");
        let status = std::process::Command::new(helper).args(["update", "host"]).args(std::env::args_os().skip(2)).status()?;
        anyhow::ensure!(status.success(), "update did not complete");
        return Ok(());
    }
    #[cfg(unix)]
    if std::env::var_os("SUNNA_SUPERVISED").is_none()
        && !std::env::args().any(|a| matches!(a.as_str(), "--version" | "--help" | "--qr" | "--authorize"))
    {
        if let Some(updater) = sunna_update::Updater::new("host").ok().filter(|u| u.is_managed_process()) {
            use std::os::unix::process::CommandExt;
            return Err(std::process::Command::new(updater.root.join("current/bin/sunna-update"))
                .args(["supervise", "host"]).args(std::env::args_os().skip(1)).exec().into());
        }
    }
    let args = Args::parse();
    if args.qr {
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text)?;
        let text = text.trim();
        anyhow::ensure!(
            !text.is_empty(),
            "nothing on standard input to make a QR code of"
        );
        return print_qr(text);
    }
    #[cfg(target_os = "windows")]
    {
        // The arguments for the host itself, without the launcher's own.
        let rest = || -> Vec<std::ffi::OsString> {
            std::env::args_os()
                .skip(1)
                .filter(|arg| {
                    !["--console-session", "--service", "--install-service", "--uninstall-service"]
                        .contains(&arg.to_string_lossy().as_ref())
                })
                .collect()
        };
        if args.clipboard_helper {
            let _telemetry = sunna_telemetry::init("clipboard", sunna_telemetry::Remote::from_env());
            return sunna_clipboard::serve_helper();
        }
        if args.install_service {
            return service::install(&rest());
        }
        if args.uninstall_service {
            return service::uninstall();
        }
        if args.service || args.console_session {
            let _telemetry = sunna_telemetry::init("launcher", sunna_telemetry::Remote::from_env());
            if args.service {
                return service::run(rest());
            }
            return console_session::run(rest(), &std::sync::atomic::AtomicBool::new(false));
        }
    }
    if args.authorize {
        #[cfg(target_os = "linux")]
        return sunna_portal::authorize();
        #[cfg(not(target_os = "linux"))]
        anyhow::bail!("--authorize is for Linux desktop portals");
    }
    let _telemetry = sunna_telemetry::init("host", sunna_telemetry::Remote::from_env());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(run(args));
    if let Err(error) = &result {
        tracing::error!("{error:#}");
    }
    result
}

fn print_qr(text: &str) -> anyhow::Result<()> {
    let code = qrcode::QrCode::new(text.as_bytes())?;
    let width = code.width();
    let quiet = 2;
    let size = width + 2 * quiet;
    let dark = |x, y| {
        x >= quiet
            && y >= quiet
            && x < width + quiet
            && y < width + quiet
            && code[(x - quiet, y - quiet)] == qrcode::Color::Dark
    };
    let mut stdout = std::io::stdout().lock();
    let terminal = stdout.is_terminal();
    for y in (0..size).step_by(2) {
        if terminal {
            // Explicit colors keep the white quiet zone on dark themes too.
            write!(stdout, "\x1b[38;2;0;0;0;48;2;255;255;255m")?;
        }
        for x in 0..size {
            let block = match (dark(x, y), dark(x, y + 1)) {
                (false, false) => ' ',
                (true, false) => '▀',
                (false, true) => '▄',
                (true, true) => '█',
            };
            write!(stdout, "{block}")?;
        }
        if terminal {
            write!(stdout, "\x1b[0m")?;
        }
        writeln!(stdout)?;
    }
    Ok(())
}

async fn run(mut args: Args) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    if args.system_host { return linux_machine::serve(args).await; }
    #[cfg(windows)]
    if args.service_key_store { args.token.clear(); }
    if args.token.is_empty() {
        let path = identity_path(false)?.with_file_name("host.key");
        args.token = sunna_transport::load_or_create_key(&path)?;
        eprintln!("Using the private sharing key in {}.", path.display());
    }

    let fixed_size = || -> anyhow::Result<_> {
        Ok(tokio::sync::watch::channel(Ok(resolve_dimensions(&args)?)).1)
    };
    #[cfg(target_os = "linux")]
    let desktop = if args.source == Source::Screen && sunna_portal::use_portal() {
        let size = resolve_dimensions(&args)?;
        linux_desktop::monitor_portal(size)
    } else if args.source == Source::Screen {
        let state = linux_desktop::check(args.width, args.height);
        linux_desktop::monitor(move || linux_desktop::check(args.width, args.height), state)
    } else {
        fixed_size()?
    };
    #[cfg(not(target_os = "linux"))]
    let desktop = fixed_size()?;
    // This fallback is for constructing the encoder before a screen exists.
    // The listener negotiates each real stream from the current desktop size.
    let (width, height) = match *desktop.borrow() {
        Ok(size) => size,
        Err(sunna_host::PORTAL_PERMISSION) => resolve_dimensions(&args)?,
        Err(_) => (640, 360),
    };
    let server = Server::with_identity(args.listen, sunna_transport::HostIdentity::load_or_create(&identity_path(false)?)?)?;
    tracing::info!(
        addr = %server.local_addr()?,
        source = ?args.source,
        stream = format!("{}x{}@{}", width, height, args.fps),
        codec = %args.codec,
        "sunnad listening"
    );
    if !args.listen.ip().is_loopback() {
        tracing::warn!("listening beyond loopback: session key required");
    }

    let bitrate_kbps = args.bitrate_kbps.unwrap_or(match args.source {
        Source::Screen => 40_000,
        Source::Synthetic => 20_000,
    });
    let bitrate_bps = bitrate_kbps.saturating_mul(1000);
    // Probe the actual backend: H.264 can use NVENC or fall back to OpenH264.
    let encoder = make_encoder(&args.codec, width, height, args.fps, bitrate_bps)?;
    let fast_lane = sunna_capture::fast_lane_enabled(args.codec == "h264" && encoder.is_software());
    drop(encoder);
    #[cfg(target_os = "windows")]
    let display_follows_viewer = args.virtual_display;
    #[cfg(not(target_os = "windows"))]
    let display_follows_viewer = false;
    let config = HostConfig {
            require_proof: std::env::var("SUNNA_REQUIRE_PROOF").map_or(false, |v| v == "1"),
        display_follows_viewer,
        clipboard: !args.no_clipboard,
        audio: !args.no_audio,
        name: args.name.clone(),
        width,
        height,
        fps: args.fps,
        codec: args.codec.clone(),
        max_bitrate_bps: bitrate_bps,
        fast_lane,
        simulate_loss: args.simulate_loss,
        token: args.token.clone(),
        about: about::detect(),
    };

    let source = args.source;
    let fps = args.fps;
    let codec = args.codec.clone();
    let verify = move || -> anyhow::Result<()> {
        if std::env::var_os("SUNNA_READY_FILE").is_none() { return Ok(()); }
        let stream = sunna_host::StreamConfig { width, height, fps, codec, bitrate_bps, fast_lane };
        let mut capture = make_source_factory(source, display_follows_viewer)(&stream)?;
        loop {
            match capture.next_frame() {
                Ok(_) => break,
                Err(error) if error.is::<sunna_capture::NoPicture>() => continue,
                Err(error) => return Err(error),
            }
        }
        if source == Source::Screen {
            #[cfg(target_os = "linux")]
            if sunna_portal::use_portal() {
                sunna_portal::PortalInjector::new()?;
            } else {
                sunna_input::linux::X11Injector::new()?;
            }
            #[cfg(target_os = "macos")]
            anyhow::ensure!(sunna_input::macos::accessibility_granted(), "accessibility permission required");
            #[cfg(not(target_os = "linux"))]
            drop(make_injector_factory(source)());
        }
        Ok(())
    };
    #[cfg(target_os = "linux")]
    if std::env::var_os("SUNNA_READY_FILE").is_some()
        && !sunna_update::Updater::new("host").is_ok_and(|u| u.is_managed_process())
    {
        // The standalone preflight also runs before a candidate is installed.
        verify()?;
    } else {
        let observed = desktop.clone();
        sunna_update::host::start_after(move |updater| {
            if source == Source::Screen && std::env::var_os("SUNNA_READY_FILE").is_some() {
                for sequence in 0.. {
                    let waiting = if sunna_portal::use_portal() {
                        matches!(*observed.borrow(), Err(sunna_host::PORTAL_PERMISSION | "Its desktop isn't running."))
                    } else {
                        linux_desktop::absent()
                    };
                    if !waiting { break; }
                    updater.waiting_for_desktop(sequence)?;
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }
            verify()
        });
    }
    #[cfg(not(target_os = "linux"))]
    {
        verify()?;
        sunna_update::host::start();
    }
    tokio::select! {
        result = run_host_with_desktop(
            server,
            config,
            make_source_factory(args.source, display_follows_viewer),
            Box::new(move |stream| make_encoder(&stream.codec, stream.width, stream.height, stream.fps, stream.bitrate_bps)),
            make_injector_factory(args.source),
            desktop,
            None,
        ) => result,
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("shutting down");
            Ok(())
        }
    }
}

/// Make sure Sunna's virtual display is showing at this mode, and send
/// capture and the pointer to it. A different mode gets a new display, added
/// before the old one goes so the desktop's windows move across to it.
#[cfg(target_os = "windows")]
fn show_virtual_display(width: u32, height: u32, refresh_hz: u32) -> anyhow::Result<()> {
    use sunna_capture::windows_display::VirtualDisplay;
    static CURRENT: std::sync::Mutex<Option<VirtualDisplay>> = std::sync::Mutex::new(None);
    let mut current = CURRENT.lock().unwrap();
    if current
        .as_ref()
        .is_some_and(|shown| (shown.width, shown.height, shown.refresh_hz) == (width, height, refresh_hz))
    {
        return Ok(());
    }
    let display = VirtualDisplay::create(width, height, refresh_hz)?;
    sunna_capture::windows::prefer_display(Some(display.name.clone()));
    sunna_input::windows::target_display(Some(display.name.clone()));
    // The old one (if any) is removed here.
    *current = Some(display);
    Ok(())
}
