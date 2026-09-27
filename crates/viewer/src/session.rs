//! One viewer session: connect, stream into the window, exit when done.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use sunna_client::{run_client, ClientOptions};
use sunna_transport::connect_insecure;

use crate::viewer;

/// What to connect to.
#[derive(Debug, Clone)]
pub struct ViewerArgs {
    pub addr: SocketAddr,
    /// SNI name expected by the host's self-signed certificate.
    pub server_name: String,
    /// Session token the host expects.
    pub token: String,
    /// Shown in the window title.
    pub host_name: String,
    /// "macOS 26.0", "Arch Linux"; empty when unknown. Picks the shortcuts
    /// the menu offers.
    pub host_os: String,
    /// Play the host's sound.
    pub audio: bool,
}

/// Largest stream this machine can show 1:1 in a window: most of the main
/// display, leaving room for the menu bar, Dock and title bar.
fn viewer_max_size() -> Option<(u32, u32)> {
    // SUNNA_VIEW_SCALE (0.25-1.0) asks for a smaller stream: less to encode
    // and send per frame, at some cost in sharpness.
    let scale = std::env::var("SUNNA_VIEW_SCALE")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .map_or(1.0, |value| value.clamp(0.25, 1.0));
    // Full screen (the default) can show the whole display; windowed mode
    // leaves room for the menu bar, Dock and title bar. Asking for the full
    // screen lets a host with an equal or smaller display send its native
    // pixels, shown 1:1 — a 3360x2100 M1 screen was being scaled to
    // 2846x1778 on the host and back up on the viewer, blurring text twice.
    let (fraction_w, fraction_h) = if viewer::windowed() {
        (0.9, 0.8)
    } else {
        (1.0, 1.0)
    };
    #[cfg(target_os = "macos")]
    {
        let (width, height) = sunna_capture::macos::main_display_pixel_size();
        Some((
            (width as f64 * fraction_w * scale) as u32,
            (height as f64 * fraction_h * scale) as u32,
        ))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (scale, fraction_w, fraction_h);
        None
    }
}

/// Why a session ended that nobody here ended, in words the app shows.
fn why_it_ended(reason: Option<sunna_transport::quinn::ConnectionError>) -> String {
    use sunna_transport::quinn::ConnectionError;
    match reason {
        Some(ConnectionError::TimedOut) => "lost the connection: nothing came back for 10 seconds".into(),
        Some(ConnectionError::ApplicationClosed(close)) if close.reason.starts_with(b"replaced") => {
            "replaced: a newer session from this Mac took over".into()
        }
        _ => "the host ended the session".into(),
    }
}

/// Connect and run a viewer window until the session ends. Owns the main
/// thread (winit requires it on macOS); the network session runs on its own
/// tokio runtime in a background thread and wakes the event loop per frame.
pub fn run(args: ViewerArgs) -> anyhow::Result<()> {
    let ViewerArgs {
        addr,
        server_name,
        token,
        host_name,
        host_os,
        audio,
    } = args;
    tracing::warn!("dev TLS: server certificate is NOT verified");
    let event_loop = viewer::create_event_loop()?;
    let proxy = event_loop.create_proxy();
    let shared = Arc::new(viewer::SharedFrame::default());

    // Establish the session first so the window opens at the stream's size.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let client = runtime.block_on(connect_insecure(addr, &server_name))?;
    let connection = client.connection.clone();

    let network_shared = Arc::clone(&shared);
    let live = Arc::clone(&shared.live);
    let tile_shared = Arc::clone(&shared);
    let tile_proxy = event_loop.create_proxy();
    let cursor_proxy = event_loop.create_proxy();
    let (input_tx, input_rx) = tokio::sync::mpsc::unbounded_channel();
    let (size_tx, size_rx) = std::sync::mpsc::channel::<(u32, u32)>();
    // The in-session menu's Video choices, sent to the host as SetStream.
    let (stream_tx, stream_rx) = tokio::sync::watch::channel(None);
    // Set when the window goes (disconnect, ⌘Q, closed): the host is told.
    let (leave_tx, leave_rx) = tokio::sync::watch::channel(false);
    let max_size = viewer_max_size();
    // To tell a session the user ended from one that ended by itself.
    let asked_to_leave = leave_rx.clone();
    let watched = connection.clone();
    std::thread::spawn(move || {
        let endpoint = client.endpoint;
        let mut announced = false;
        let options = ClientOptions {
            clipboard: true,
            // How the host lists this viewer ("in use by …").
            name: std::env::var("SUNNA_VIEWER_NAME").unwrap_or_else(|_| "Sunna viewer".into()),
            device: sunna_client::device_id(),
            leave: Some(leave_rx),
            // A new pointer shape: wake the window to put it on.
            wake: Some(sunna_client::Wake(Arc::new(move || {
                let _ = cursor_proxy.send_event(viewer::FrameReady);
            }))),
            token,
            stream: sunna_proto::messages::StreamSettings {
                max_size,
                audio: Some(audio),
                ..Default::default()
            },
            stream_requests: Some(stream_rx),
            duration: None,
            live: Some(live),
        };
        let result = runtime.block_on(run_client(
            connection,
            options,
            move |frame| {
                if !announced {
                    announced = true;
                    let _ = size_tx.send((frame.width, frame.height));
                }
                *network_shared.latest.lock().unwrap() = Some(frame);
                let _ = proxy.send_event(viewer::FrameReady);
            },
            move |batch| {
                tile_shared.tiles.lock().unwrap().push(batch);
                let _ = tile_proxy.send_event(viewer::FrameReady);
            },
            input_rx,
        ));
        // A failed session exits non-zero: the app reports the reason. So
        // does one that ended without being asked to: the connection was
        // lost, or the host ended it.
        let code = match result {
            Ok(report) if *asked_to_leave.borrow() => {
                tracing::info!(report = %report, "session ended");
                println!("{report}");
                0
            }
            Ok(report) => {
                tracing::info!(report = %report, "session ended");
                tracing::error!("session error: {}", why_it_ended(watched.close_reason()));
                1
            }
            Err(error) => {
                tracing::error!("session error: {error:#}");
                1
            }
        };
        // Let the connection's close reach the host (so it's free at once),
        // but don't hang on a dead network.
        runtime.block_on(async {
            let _ = tokio::time::timeout(Duration::from_millis(500), endpoint.wait_idle()).await;
        });
        // The event loop has no reason to outlive the session; exit skips
        // destructors, so ship remaining telemetry first.
        sunna_telemetry::flush();
        std::process::exit(code);
    });

    // Wait briefly for the first frame to learn the stream size.
    let (width, height) = size_rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap_or((1280, 720));
    let result = viewer::run_viewer(
        event_loop,
        shared,
        input_tx,
        viewer::StreamControl {
            requests: stream_tx,
            max_size,
            host_name: host_name.clone(),
            host_os,
        },
        format!("Sunna — {host_name}"),
        width,
        height,
    );
    // The window is gone: leave, and give the session thread a moment to
    // tell the host (it exits the process when done).
    let _ = leave_tx.send(true);
    std::thread::sleep(Duration::from_secs(1));
    result
}
