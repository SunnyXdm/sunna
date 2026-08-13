//! sunnad — the headless Sunna host daemon.
//!
//! Milestone 0a: streams a synthetic test pattern over the raw passthrough
//! codec and logs (rather than injects) incoming input. No authentication yet:
//! binds localhost by default; do not expose beyond a trusted LAN.

use std::net::SocketAddr;

use clap::Parser;
use sunna_capture::{FrameSource, SyntheticSource};
use sunna_codec::{default_codec_name, make_encoder};
use sunna_host::{HostConfig, run_host};
use sunna_input::{InputInjector, LogInjector};
use sunna_transport::Server;

#[derive(Parser, Debug)]
#[command(name = "sunnad", about = "Sunna headless host daemon (Milestone 0a)")]
struct Args {
    /// Address to listen on. Loopback by default — there is no auth yet.
    #[arg(long, default_value = "127.0.0.1:48800")]
    listen: SocketAddr,
    #[arg(long, default_value_t = 640)]
    width: u32,
    #[arg(long, default_value_t = 360)]
    height: u32,
    #[arg(long, default_value_t = 60)]
    fps: u32,
    /// Codec to encode with ("h264" on macOS, "raw" fallback).
    #[arg(long, default_value = default_codec_name())]
    codec: String,
    /// Encoder target bitrate in kilobits per second.
    #[arg(long, default_value_t = 20_000)]
    bitrate_kbps: u32,
    /// Dev-only: fraction of media datagrams to drop (0.0..1.0) to exercise
    /// FEC and keyframe recovery.
    #[arg(long, default_value_t = 0.0)]
    simulate_loss: f64,
    /// Host name announced to clients.
    #[arg(long, default_value = "sunnad")]
    name: String,
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

    let server = Server::bind(args.listen)?;
    tracing::info!(
        addr = %server.local_addr()?,
        source = format!("synthetic {}x{}@{}", args.width, args.height, args.fps),
        codec = %args.codec,
        "sunnad listening"
    );
    if !args.listen.ip().is_loopback() {
        tracing::warn!("listening beyond loopback with NO authentication (dev TLS only)");
    }

    let bitrate_bps = args.bitrate_kbps.saturating_mul(1000);
    let config = HostConfig {
        name: args.name.clone(),
        width: args.width,
        height: args.height,
        fps: args.fps,
        codec: args.codec.clone(),
        max_bitrate_bps: bitrate_bps,
        simulate_loss: args.simulate_loss,
    };
    let (width, height, fps) = (args.width, args.height, args.fps);
    let codec = args.codec.clone();
    // Fail fast on an unbuildable codec instead of per-connection.
    make_encoder(&codec, width, height, fps, bitrate_bps)?;

    tokio::select! {
        result = run_host(
            server,
            config,
            Box::new(move || Box::new(SyntheticSource::new(width, height, fps)) as Box<dyn FrameSource>),
            Box::new(move || {
                make_encoder(&codec, width, height, fps, bitrate_bps)
                    .expect("encoder was validated at startup")
            }),
            Box::new(|| Box::new(LogInjector) as Box<dyn InputInjector>),
        ) => result,
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("shutting down");
            Ok(())
        }
    }
}
