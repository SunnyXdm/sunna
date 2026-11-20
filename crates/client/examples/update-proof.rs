//! Loopback release and compatibility fixture. Credentials come from the environment.
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use sunna_client::{auth::Trust, ClientOptions};
use sunna_proto::messages::StreamSettings;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let addr: std::net::SocketAddr = args[1].parse()?;
    let key = std::env::var("SUNNA_TOKEN")?;
    let trust = Trust {
        upgraded: std::env::var("SUNNA_TEST_UPGRADED").as_deref() == Ok("1"),
        identity: std::env::var("SUNNA_TEST_PIN").ok(),
        record: Some("release-fixture".into()),
        allow_older: std::env::var("SUNNA_TEST_ALLOW_OLDER").as_deref() == Ok("1"),
    };
    if args[0] == "add-link" {
        trust.pin_from_link(trust.identity.as_deref().ok_or_else(|| anyhow::anyhow!("Missing link pin."))?)?;
        return Ok(());
    }
    if args[0] == "policy" {
        return trust.set_allow_older(trust.allow_older);
    }
    // A viewer subprocess can have an older policy snapshot than Edit. The
    // parent fixture releases this barrier after committing another decision.
    if std::env::var_os("SUNNA_TEST_SNAPSHOT_BARRIER").is_some() {
        use std::io::Write;
        println!("snapshot loaded");
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
    } else {
        trust.set_allow_older(trust.allow_older)?;
    }
    if args[0] == "check-state" {
        let check = sunna_client::reach::check_with(&addr.to_string(), &key, &trust).await;
        println!("{} {}", check.state, check.unverified);
        return Ok(());
    }
    if args[0] == "hold" {
        let client = trust.connect(addr, "sunna").await?;
        let (_input, rx) = tokio::sync::mpsc::unbounded_channel();
        let options = ClientOptions {
            trust,
            token: key,
            name: "local proof".into(),
            device: sunna_client::device_id(),
            duration: Some(Duration::from_secs(
                args.get(2).and_then(|s| s.parse().ok()).unwrap_or(15),
            )),
            stream: StreamSettings {
                codec: Some("raw".into()),
                max_size: Some((64, 64)),
                fps: Some(1),
                audio: Some(false),
                ..Default::default()
            },
            ..Default::default()
        };
        let announced = AtomicBool::new(false);
        let report = sunna_client::run_client(
            client.connection,
            options,
            move |_| {
                if !announced.swap(true, Ordering::Relaxed) {
                    println!("connected");
                }
            },
            |_| {},
            rx,
        )
        .await?;
        let _ = tokio::time::timeout(Duration::from_millis(500), client.endpoint.wait_idle()).await;
        anyhow::ensure!(report.frames_completed > 0, "no frames");
    } else if args[0] == "probe" {
        let p =
            sunna_client::probe_with(addr, "sunna", &key, Duration::from_secs(4), &trust).await?;
        anyhow::ensure!(p.token_ok && p.version == 8, "incompatible host");
        println!(
            "{}",
            if p.identity.is_some() {
                "probe ok"
            } else {
                "not fully verified"
            }
        );
    } else {
        println!(
            "{}",
            sunna_client::host_update_with(addr, &key, &args[0], &trust).await?
        );
    }
    Ok(())
}
