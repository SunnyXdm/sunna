//! sunnad — the headless Sunna host daemon.
//!
//! `--source screen` streams the real display (macOS only for now; requires
//! the Screen Recording permission for the process that launches sunnad).
//! `--source synthetic` streams a test pattern. Input is logged, not injected,
//! until the injection backend lands. No authentication yet: binds localhost
//! by default; do not expose beyond a trusted LAN.

use std::net::SocketAddr;

use clap::{Parser, ValueEnum};
use sunna_capture::{FrameSource, SyntheticSource};
use sunna_codec::{default_codec_name, make_encoder};
use sunna_host::{HostConfig, run_host};
use sunna_input::make_injector;
use sunna_transport::Server;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum Source {
    Synthetic,
    /// Capture the main display (macOS only for now).
    Screen,
}

#[derive(Parser, Debug)]
#[command(name = "sunnad", about = "Sunna headless host daemon")]
struct Args {
    /// Address to listen on. Loopback by default — there is no auth yet.
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
    /// Codec to encode with ("h264" on macOS, "raw" fallback).
    #[arg(long, default_value = default_codec_name())]
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
                let (native_w, native_h) = sunna_capture::macos::main_display_pixel_size();
                // Even dimensions for the encoder; must match what ScreenSource uses.
                Ok((
                    args.width.unwrap_or(native_w).max(2) & !1,
                    args.height.unwrap_or(native_h).max(2) & !1,
                ))
            }
            #[cfg(not(target_os = "macos"))]
            {
                anyhow::bail!("--source screen is only supported on macOS so far (research/06 M0c)")
            }
        }
    }
}

fn make_source_factory(
    source: Source,
    width: u32,
    height: u32,
    fps: u32,
) -> Box<dyn Fn() -> Box<dyn FrameSource> + Send + Sync> {
    match source {
        Source::Synthetic => Box::new(move || {
            Box::new(SyntheticSource::new(width, height, fps)) as Box<dyn FrameSource>
        }),
        Source::Screen => {
            #[cfg(target_os = "macos")]
            {
                Box::new(move || {
                    Box::new(
                        sunna_capture::macos::ScreenSource::new(Some(width), Some(height), fps)
                            .expect("screen capture was validated at startup"),
                    ) as Box<dyn FrameSource>
                })
            }
            #[cfg(not(target_os = "macos"))]
            {
                unreachable!("rejected in resolve_dimensions")
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args = Args::parse();

    let (width, height) = resolve_dimensions(&args)?;
    let server = Server::bind(args.listen)?;
    tracing::info!(
        addr = %server.local_addr()?,
        source = ?args.source,
        stream = format!("{}x{}@{}", width, height, args.fps),
        codec = %args.codec,
        "sunnad listening"
    );
    if !args.listen.ip().is_loopback() {
        tracing::warn!("listening beyond loopback with NO authentication (dev TLS only)");
    }

    let bitrate_kbps = args.bitrate_kbps.unwrap_or(match args.source {
        Source::Screen => 40_000,
        Source::Synthetic => 20_000,
    });
    let bitrate_bps = bitrate_kbps.saturating_mul(1000);
    let config = HostConfig {
        name: args.name.clone(),
        width,
        height,
        fps: args.fps,
        codec: args.codec.clone(),
        max_bitrate_bps: bitrate_bps,
        simulate_loss: args.simulate_loss,
    };
    let fps = args.fps;
    let codec = args.codec.clone();
    // Fail fast on an unbuildable codec instead of per-connection.
    make_encoder(&codec, width, height, fps, bitrate_bps)?;

    tokio::select! {
        result = run_host(
            server,
            config,
            make_source_factory(args.source, width, height, fps),
            Box::new(move || {
                make_encoder(&codec, width, height, fps, bitrate_bps)
                    .expect("encoder was validated at startup")
            }),
            Box::new(make_injector),
        ) => result,
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("shutting down");
            Ok(())
        }
    }
}
