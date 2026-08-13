//! sunna-cli — dev client for the Sunna pipeline.
//!
//! `connect` joins a running sunnad (headless: stats only, no render window yet).
//! `bench` runs host + client in one process over loopback QUIC and prints the
//! end-to-end latency report — the Milestone 0a instrumentation payoff.

use std::net::SocketAddr;
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
    /// Connect to a sunnad host (dev TLS: certificate NOT verified).
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    match Cli::parse().command {
        Command::Connect {
            addr,
            server_name,
            seconds,
        } => {
            tracing::warn!("dev TLS: server certificate is NOT verified");
            let client = connect_insecure(addr, &server_name).await?;
            let report = run_client(
                client.connection,
                "sunna-cli",
                seconds.map(Duration::from_secs),
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
            let report = run_client(
                client.connection,
                "bench-client",
                Some(Duration::from_secs(seconds)),
            )
            .await?;
            host_task.abort();
            println!("{report}");
        }
    }
    Ok(())
}
