//! Connect once, save decoded pictures and optionally drive the fixture
//! through the same input messages a viewer uses. Test passwords use stdin.
use std::io::{Read, Write};
use std::time::{Duration, Instant};

use anyhow::{ensure, Context};
use sunna_client::{run_client, ClientOptions};
use sunna_proto::messages::{InputEvent, MouseButton, StreamSettings};

fn save(path: &str, frame: &sunna_codec::DecodedFrame) -> anyhow::Result<()> {
    ensure!(frame.format == sunna_capture::PixelFormat::I420);
    let pixels = frame.data.to_cpu()?;
    ensure!(pixels.len() == frame.width as usize * frame.height as usize * 3 / 2);
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    write!(
        file,
        "YUV4MPEG2 W{} H{} F1:1 Ip A1:1 C420jpeg\nFRAME\n",
        frame.width, frame.height
    )?;
    file.write_all(&pixels)?;
    file.flush()?;
    Ok(())
}

fn stamp() -> String {
    std::fs::read_to_string("/proc/uptime")
        .unwrap_or_default()
        .split_whitespace()
        .next()
        .unwrap_or("?")
        .to_string()
}

fn marker(frame: &sunna_codec::DecodedFrame) -> bool {
    if frame.format != sunna_capture::PixelFormat::I420 {
        return false;
    }
    let Ok(pixels) = frame.data.to_cpu() else {
        return false;
    };
    let (w, h) = (frame.width as usize, frame.height as usize);
    let v = w * h * 5 / 4;
    [5usize, 7, 9, 11].iter().enumerate().all(|(i, x)| {
        pixels
            .get(v + (h * 5 / 12 / 2) * (w / 2) + (w * x / 16 / 2))
            .is_some_and(|value| {
                if i % 2 == 0 {
                    *value > 175
                } else {
                    *value < 85
                }
            })
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let addr = args.next().context("address required")?;
    let token = std::env::var("SUNNA_TOKEN").unwrap_or_default();
    let output = args.next().context("supply a Y4M output path or 'check'")?;
    let trust = sunna_client::auth::Trust {
        record: Some(format!("login-test-{}", addr.parse::<std::net::SocketAddr>()?.port())),
        allow_older: std::env::var("SUNNA_TEST_ALLOW_OLDER").as_deref() == Ok("1"),
        ..Default::default()
    };
    trust.set_allow_older(trust.allow_older)?;
    if output == "check" {
        let check = sunna_client::reach::check_with(&addr, &token, &trust).await;
        println!("{} {}", check.state, if check.unverified { "not fully verified" } else { &check.detail });
        return Ok(());
    }
    let mut script = String::new();
    if args.next().as_deref() == Some("--steps") {
        std::io::stdin().read_to_string(&mut script)?;
    }
    let start = Instant::now();
    let client = trust.connect(addr.parse()?, "sunna").await?;
    let (input, rx) = tokio::sync::mpsc::unbounded_channel();
    let (tx, mut picture) = tokio::sync::watch::channel(None);
    let (leave, leaving) = tokio::sync::watch::channel(false);
    let options = ClientOptions {
        trust: trust.clone(),
        name: "login-test".into(),
        device: "login-test".into(),
        token,
        stream: StreamSettings {
            codec: Some("h264".into()),
            max_size: Some((640, 480)),
            fps: Some(5),
            fast_lane: Some(false),
            audio: Some(false),
            ..Default::default()
        },
        leave: Some(leaving),
        ..Default::default()
    };
    let watched = client.connection.clone();
    let reconnect = options.clone();
    let session = run_client(
        client.connection,
        options,
        move |frame| {
            tx.send_replace(Some(frame));
        },
        |_| {},
        rx,
    );
    tokio::pin!(session);
    let drive = async {
        tokio::time::timeout(Duration::from_secs(45), picture.changed()).await??;
        {
            let image = picture.borrow();
            let frame = image.as_ref().context("no frame")?;
            println!(
                "{}x{} first frame in {} ms",
                frame.width,
                frame.height,
                start.elapsed().as_millis()
            );
            save(&output, frame)?;
        }
        for line in script.lines() {
            let (op, args) = line.split_once(' ').context("missing step argument")?;
            match op {
                "handoff" => std::future::pending::<()>().await,
                "wait" => tokio::time::sleep(Duration::from_millis(args.parse()?)).await,
                "save" => save(args, picture.borrow().as_ref().context("no picture")?)?,
                "click" => {
                    let (x, y) = args.split_once(' ').context("click X Y")?;
                    input.send(InputEvent::MouseMoveAbs {
                        x: x.parse()?,
                        y: y.parse()?,
                    })?;
                    for pressed in [true, false] {
                        input.send(InputEvent::MouseButton {
                            button: MouseButton::Left,
                            pressed,
                        })?;
                    }
                }
                "keydown" => {
                    input.send(InputEvent::Key {
                        scancode: u16::from_str_radix(args, 16)?, pressed: true, repeat: false,
                    })?;
                }
                "key" => {
                    println!("{} key step at {} ms", stamp(), start.elapsed().as_millis());
                    for event in sunna_client::keys::press(&[u16::from_str_radix(args, 16)?]) {
                        input.send(event)?;
                    }
                }
                "type" => {
                    let (keys, skipped) = sunna_client::keys::type_text(args);
                    ensure!(skipped == 0, "unsupported fixture character");
                    for chord in keys {
                        for event in chord {
                            input.send(event)?;
                        }
                        tokio::time::sleep(Duration::from_millis(40)).await;
                    }
                }
                _ => anyhow::bail!("unknown fixture operation"),
            }
        }
        Ok::<(), anyhow::Error>(())
    };
    let handoff = script
        .lines()
        .find_map(|line| line.strip_prefix("handoff "));
    tokio::select! {
        result = &mut session => {
            if !sunna_client::session_changed(&watched) || handoff.is_none() {
                result?;
                anyhow::bail!("session ended before fixture steps finished: {:?}", watched.close_reason());
            }
        },
        result = drive => {
            result?;
            leave.send(true)?;
            session.await?;
            return Ok(());
        },
    }
    println!(
        "{} session changed at {} ms",
        stamp(),
        start.elapsed().as_millis()
    );
    loop {
        let next = sunna_client::reconnect_for_handoff(addr.parse()?, "sunna", &trust).await?;
        let watched = next.connection.clone();
        let (_input, rx) = tokio::sync::mpsc::unbounded_channel();
        let path = handoff.unwrap().to_string();
        let (done, mut saved) = tokio::sync::oneshot::channel();
        let mut done = Some(done);
        let mut first = true;
        let measure = std::env::var_os("SUNNA_MEASURE_LOGIN").is_some();
        let fresh = run_client(
            next.connection,
            reconnect.clone(),
            move |frame| {
                if first {
                    println!("{} first decoded frame from new worker", stamp());
                    first = false;
                }
                if measure && !marker(&frame) {
                    return;
                }

                if let Some(done) = done.take() {
                    let _ = done.send(save(&path, &frame));
                }
            },
            |_| {},
            rx,
        );
        tokio::pin!(fresh);
        tokio::select! {
            result = &mut fresh => {
                if sunna_client::session_changed(&watched) || result.as_ref().is_err_and(sunna_client::handoff_pending) {
                    continue;
                }
                result?;
                anyhow::bail!("no new session frame");
            },
            result = &mut saved => result??,
        }
        println!(
            "{} new session frame at {} ms",
            stamp(),
            start.elapsed().as_millis()
        );
        leave.send(true)?;
        fresh.await?;
        return Ok(());
    }
}
