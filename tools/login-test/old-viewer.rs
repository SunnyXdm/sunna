//! Compile this driver against the deployed protocol-8 client library.
use anyhow::{ensure, Context};
use std::time::Duration;
use sunna_client::{run_client, ClientOptions};
use sunna_proto::messages::StreamSettings;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().context("probe, hold or watch")?;
    let addr = args.next().context("address")?.parse()?;
    let token = std::env::var("SUNNA_TOKEN")?;
    if mode == "probe" {
        let p = sunna_client::probe(addr, "sunna", &token, Duration::from_secs(5)).await?;
        ensure!(p.token_ok && p.version == 8);
        println!("probe ok");
        return Ok(());
    }
    let duration = match mode.as_str() {
        "watch" => None,
        "hold" => Some(Duration::from_secs(
            args.next().context("seconds")?.parse()?,
        )),
        _ => anyhow::bail!("unknown fixture operation"),
    };
    let first = args.next();
    let client = sunna_transport::connect_insecure(addr, "sunna").await?;
    let (_input, rx) = tokio::sync::mpsc::unbounded_channel();
    let options = ClientOptions {
        token,
        duration,
        stream: StreamSettings {
            codec: Some("h264".into()),
            max_size: Some((320, 240)),
            fps: Some(3),
            audio: Some(false),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut first = first;
    let report = run_client(
        client.connection,
        options,
        move |_| {
            if let Some(path) = first.take() {
                std::fs::write(path, b"decoded").unwrap();
            }
        },
        |_| {},
        rx,
    )
    .await?;
    if mode == "watch" {
        ensure!(report.frames_completed > 0);
    }
    println!("protocol-8 client ended its session");
    Ok(())
}
