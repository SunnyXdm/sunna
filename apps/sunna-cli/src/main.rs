//! sunna-cli — dev client for the Sunna pipeline.
//!
//! `view` opens a window showing the host's stream (the first testable build).
//! `connect` joins headless (stats only). `bench` runs host + client in one
//! process over loopback QUIC and prints the end-to-end latency report.

mod keymap;
mod viewer;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use sunna_capture::{FrameSource, SyntheticSource};
use sunna_client::run_client;
use sunna_codec::{default_codec_name, make_encoder};
use sunna_host::{HostConfig, run_host};
use sunna_input::{InputInjector, LogInjector};
use sunna_transport::{connect_insecure, connect_trusted, Server};

#[derive(Parser, Debug)]
#[command(name = "sunna-cli", about = "Sunna dev client")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Open a viewer window on a sunnad host (dev TLS: certificate NOT verified).
    View {
        addr: SocketAddr,
        /// SNI name expected by the host's self-signed certificate.
        #[arg(long, default_value = "sunna")]
        server_name: String,
    },
    /// Connect headless: stats only (dev TLS: certificate NOT verified).
    Connect {
        addr: SocketAddr,
        /// SNI name expected by the host's self-signed certificate.
        #[arg(long, default_value = "sunna")]
        server_name: String,
        /// Disconnect after this many seconds (runs until closed if omitted).
        #[arg(long)]
        seconds: Option<u64>,
    },
    /// In-process loopback benchmark: host + client, one report.
    Bench {
        #[arg(long, default_value_t = 5)]
        seconds: u64,
        #[arg(long, default_value_t = 480)]
        width: u32,
        #[arg(long, default_value_t = 270)]
        height: u32,
        #[arg(long, default_value_t = 60)]
        fps: u32,
        /// Codec for the in-process host ("h264" on macOS, "raw" fallback).
        #[arg(long, default_value = default_codec_name())]
        codec: String,
        /// Encoder target bitrate in kilobits per second.
        #[arg(long, default_value_t = 20_000)]
        bitrate_kbps: u32,
        /// Fraction of media datagrams to drop (0.0..1.0) to exercise FEC
        /// and keyframe recovery.
        #[arg(long, default_value_t = 0.0)]
        simulate_loss: f64,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    match Cli::parse().command {
        Command::View { addr, server_name } => view(addr, server_name),
        command => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(run_async(command))
        }
    }
}

/// The viewer owns the main thread (winit requirement on macOS); the network
/// session runs on its own tokio runtime in a background thread and wakes the
/// event loop per frame.
fn view(addr: SocketAddr, server_name: String) -> anyhow::Result<()> {
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
    let (input_tx, input_rx) = tokio::sync::mpsc::unbounded_channel();
    let (size_tx, size_rx) = std::sync::mpsc::channel::<(String, u32, u32)>();
    std::thread::spawn(move || {
        let _endpoint_guard = client.endpoint;
        let mut announced = false;
        let result = runtime.block_on(run_client(
            connection,
            "sunna-viewer",
            None,
            move |frame| {
                if !announced {
                    announced = true;
                    let _ = size_tx.send(("sunna".to_string(), frame.width, frame.height));
                }
                *network_shared.latest.lock().unwrap() = Some(frame);
                let _ = proxy.send_event(viewer::FrameReady);
            },
            input_rx,
        ));
        match result {
            Ok(report) => println!("{report}"),
            Err(error) => eprintln!("session error: {error}"),
        }
        // The event loop has no reason to outlive the session.
        std::process::exit(0);
    });

    // Wait briefly for the first frame to learn the stream size.
    let (title, width, height) = size_rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap_or(("sunna".to_string(), 1280, 720));
    viewer::run_viewer(
        event_loop,
        shared,
        input_tx,
        format!("Sunna — {title}"),
        width,
        height,
    )
}

async fn run_async(command: Command) -> anyhow::Result<()> {
    match command {
        Command::View { .. } => unreachable!("handled in main"),
        Command::Connect {
            addr,
            server_name,
            seconds,
        } => {
            tracing::warn!("dev TLS: server certificate is NOT verified");
            let client = connect_insecure(addr, &server_name).await?;
            let (_input_tx, input_rx) = tokio::sync::mpsc::unbounded_channel();
            let report = run_client(
                client.connection,
                "sunna-cli",
                seconds.map(Duration::from_secs),
                |_| {},
                input_rx,
            )
            .await?;
            println!("{report}");
        }
        Command::Bench {
            seconds,
            width,
            height,
            fps,
            codec,
            bitrate_kbps,
            simulate_loss,
        } => {
            let server = Server::bind("127.0.0.1:0".parse().expect("valid loopback addr"))?;
            let addr = server.local_addr()?;
            let cert = server.cert.clone();
            let bitrate_bps = bitrate_kbps.saturating_mul(1000);
            let config = HostConfig {
                name: "bench-host".into(),
                width,
                height,
                fps,
                codec: codec.clone(),
                max_bitrate_bps: bitrate_bps,
                simulate_loss,
            };
            make_encoder(&codec, width, height, fps, bitrate_bps)?;
            let host_task = tokio::spawn(run_host(
                server,
                config,
                Box::new(move || {
                    Box::new(SyntheticSource::new(width, height, fps)) as Box<dyn FrameSource>
                }),
                Box::new(move || {
                    make_encoder(&codec, width, height, fps, bitrate_bps)
                        .expect("encoder was validated at startup")
                }),
                Box::new(|| Box::new(LogInjector) as Box<dyn InputInjector>),
            ));

            let client = connect_trusted(addr, "sunna", &cert).await?;
            let (_input_tx, input_rx) = tokio::sync::mpsc::unbounded_channel();
            let report = run_client(
                client.connection,
                "bench-client",
                Some(Duration::from_secs(seconds)),
                |_| {},
                input_rx,
            )
            .await?;
            host_task.abort();
            println!("{report}");
        }
    }
    Ok(())
}
