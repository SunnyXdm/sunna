//! sunna-cli — dev client for the Sunna pipeline.
//!
//! `view` opens a window showing the host's stream (the first testable build).
//! `connect` joins headless (stats only). `bench` runs host + client in one
//! process over loopback QUIC and prints the end-to-end latency report.

use std::net::SocketAddr;
use std::time::Duration;

use clap::{Parser, Subcommand};
use sunna_capture::{FrameSource, SyntheticSource};
use sunna_client::{run_client, ClientOptions};
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
        /// Session token printed by (or given to) sunnad.
        #[arg(long, env = "SUNNA_TOKEN", default_value = "", hide_env_values = true)]
        token: String,
        /// Name shown in the window title (default: the address).
        #[arg(long)]
        name: Option<String>,
        /// Don't play the host's sound.
        #[arg(long)]
        no_audio: bool,
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
        /// Session token printed by (or given to) sunnad.
        #[arg(long, env = "SUNNA_TOKEN", default_value = "", hide_env_values = true)]
        token: String,
        /// Apply the stream options this many seconds into the session.
        #[arg(long)]
        set_stream_after: Option<u64>,
        #[arg(long)]
        codec: Option<String>,
        #[arg(long, value_parser = parse_size)]
        max_size: Option<(u32, u32)>,
        #[arg(long)]
        bitrate_kbps: Option<u32>,
        #[arg(long)]
        fps: Option<u32>,
        #[arg(long, action = clap::ArgAction::Set)]
        fast_lane: Option<bool>,
        /// Share this machine's clipboard with the host.
        #[arg(long)]
        clipboard: bool,
        /// Play the host's sound (SUNNA_AUDIO_OUTPUT=FILE writes raw PCM instead).
        #[arg(long)]
        audio: bool,
        /// With --set-stream-after: then turn sound on or off.
        #[arg(long, action = clap::ArgAction::Set)]
        set_audio: Option<bool>,
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
        /// Codec for the in-process host ("h264": VideoToolbox on macOS, OpenH264 on Linux; or "raw").
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

fn parse_size(value: &str) -> Result<(u32, u32), String> {
    let (width, height) = value.split_once('x').ok_or("expected WxH")?;
    let width = width.parse::<u32>().map_err(|_| "invalid width")?;
    let height = height.parse::<u32>().map_err(|_| "invalid height")?;
    if width < 2 || height < 2 {
        return Err("dimensions must be at least 2".into());
    }
    Ok((width, height))
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let role = match cli.command {
        Command::View { .. } => "viewer",
        Command::Connect { .. } => "connect",
        Command::Bench { .. } => "bench",
    };
    let _telemetry = sunna_telemetry::init(role, sunna_telemetry::Remote::from_env());
    let result = run(cli);
    if let Err(error) = &result {
        tracing::error!("{error:#}");
    }
    result
}

fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Command::View {
            addr,
            server_name,
            token,
            name,
            no_audio,
        } => sunna_viewer::run(sunna_viewer::ViewerArgs {
            addr,
            server_name,
            token,
            host_name: name.unwrap_or_else(|| addr.ip().to_string()),
            host_os: String::new(),
            audio: !no_audio,
        }),
        command => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(run_async(command))
        }
    }
}

async fn run_async(command: Command) -> anyhow::Result<()> {
    match command {
        Command::View { .. } => unreachable!("handled in main"),
        Command::Connect {
            addr,
            server_name,
            seconds,
            token,
            clipboard,
            set_stream_after,
            codec,
            max_size,
            bitrate_kbps,
            fps,
            fast_lane,
            audio,
            set_audio,
        } => {
            tracing::warn!("dev TLS: server certificate is NOT verified");
            let client = connect_insecure(addr, &server_name).await?;
            let (_input_tx, input_rx) = tokio::sync::mpsc::unbounded_channel();
            let settings = sunna_proto::messages::StreamSettings {
                codec, max_size, max_bitrate_kbps: bitrate_kbps, fps, fast_lane, audio: set_audio.or(Some(audio)),
            };
            let (requests, stream_requests) = tokio::sync::watch::channel(None);
            let stream = if let Some(seconds) = set_stream_after {
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(seconds)).await;
                    let _ = requests.send(Some(settings));
                });
                sunna_proto::messages::StreamSettings { audio: Some(audio), ..Default::default() }
            } else {
                settings
            };
            let options = ClientOptions {
                name: "sunna-cli".into(),
                clipboard,
                token,
                stream,
                stream_requests: Some(stream_requests),
                duration: seconds.map(Duration::from_secs),
                live: None,
            };
            let report = run_client(client.connection, options, |_| {}, |_| {}, input_rx).await?;
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
                fast_lane: false,
                simulate_loss,
                token: String::new(),
                clipboard: false,
                about: Default::default(),
                audio: false,
            };
            make_encoder(&codec, width, height, fps, bitrate_bps)?;
            let host_task = tokio::spawn(run_host(
                server,
                config,
                Box::new(move |stream| {
                    Ok(Box::new(SyntheticSource::new(stream.width, stream.height, stream.fps)) as Box<dyn FrameSource>)
                }),
                Box::new(move |stream| make_encoder(&stream.codec, stream.width, stream.height, stream.fps, stream.bitrate_bps)),
                Box::new(|| Box::new(LogInjector) as Box<dyn InputInjector>),
            ));

            let client = connect_trusted(addr, "sunna", &cert).await?;
            let (_input_tx, input_rx) = tokio::sync::mpsc::unbounded_channel();
            let options = ClientOptions {
                name: "bench-client".into(),
                clipboard: false,
                token: String::new(),
                stream: Default::default(),
                stream_requests: None,
                duration: Some(Duration::from_secs(seconds)),
                live: None,
            };
            let report = run_client(client.connection, options, |_| {}, |_| {}, input_rx).await?;
            host_task.abort();
            println!("{report}");
        }
    }
    Ok(())
}
