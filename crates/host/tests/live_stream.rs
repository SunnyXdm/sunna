use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sunna_capture::{FrameSource, SyntheticSource};
use sunna_client::{probe, run_client, ClientOptions, LiveStats};
use sunna_codec::{make_encoder, DecodedFrame};
use sunna_host::{run_host, HostConfig, StreamConfig};
use sunna_input::LogInjector;
use sunna_proto::messages::{ControlMessage, StreamSettings};
use sunna_transport::{connect_insecure, ControlChannel, Server};

const TIMEOUT: Duration = Duration::from_secs(10);

struct Host {
    addr: SocketAddr,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
    builds: Arc<Mutex<Vec<StreamConfig>>>,
}

// The synthetic source's tile generator is only a transport fixture.
struct TileSource(SyntheticSource);

impl FrameSource for TileSource {
    fn next_frame(&mut self) -> anyhow::Result<sunna_capture::VideoFrame> {
        self.0.next_frame()
    }
    fn width(&self) -> u32 {
        self.0.width()
    }
    fn height(&self) -> u32 {
        self.0.height()
    }
    fn fps(&self) -> u32 {
        self.0.fps()
    }
    fn supports_tiles(&self) -> bool {
        true
    }
    fn set_tile_sink(&mut self, sink: sunna_capture::TileSink) {
        self.0.set_tile_sink(sink);
    }
}

impl Host {
    fn start() -> Self {
        Self::with_tiles(true)
    }

    fn with_tiles(supports_tiles: bool) -> Self {
        let server = Server::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = server.local_addr().unwrap();
        let builds = Arc::new(Mutex::new(Vec::new()));
        let built = Arc::clone(&builds);
        let task = tokio::spawn(run_host(
            server,
            HostConfig {
                clipboard: false,
                name: "test-host".into(),
                token: "secret".into(),
                width: 320,
                height: 180,
                fps: 20,
                codec: "raw".into(),
                max_bitrate_bps: 2_000_000,
                fast_lane: false,
                simulate_loss: 0.0,
                about: Default::default(),
            },
            Box::new(move |config| {
                // Exercise a source failure after validation has succeeded.
                anyhow::ensure!(config.fps != 13, "test source failure");
                built.lock().unwrap().push(config.clone());
                let source = SyntheticSource::new(config.width, config.height, config.fps);
                if supports_tiles {
                    Ok(Box::new(TileSource(source)) as Box<dyn FrameSource>)
                } else {
                    Ok(Box::new(source) as Box<dyn FrameSource>)
                }
            }),
            Box::new(|config| {
                anyhow::ensure!(config.bitrate_bps != 123_000, "test encoder failure");
                #[cfg(target_os = "linux")]
                if config.codec == "h264" {
                    // This test must exercise software decoding on Linux without a GPU.
                    return Ok(Box::new(sunna_codec::openh264_codec::OpenH264Encoder::new(
                        config.fps,
                        config.bitrate_bps,
                    )?));
                }
                make_encoder(
                    &config.codec,
                    config.width,
                    config.height,
                    config.fps,
                    config.bitrate_bps,
                )
            }),
            Box::new(|| Box::new(LogInjector)),
        ));
        Self { addr, task, builds }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn wait_until(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(TIMEOUT, async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("condition timed out");
}

async fn frame_at(
    frames: &mut tokio::sync::mpsc::UnboundedReceiver<DecodedFrame>,
    width: u32,
    height: u32,
) -> u64 {
    tokio::time::timeout(TIMEOUT, async {
        loop {
            let frame = frames.recv().await.expect("decoder exited");
            if (frame.width, frame.height) == (width, height) {
                assert_eq!(
                    frame.data.to_cpu().unwrap().len(),
                    (width * height * 4) as usize
                );
                return frame.frame_id;
            }
        }
    })
    .await
    .expect("no decoded frame at requested size")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_resolution_codec_failures_and_fast_lane() {
    let host = Host::start();
    let client = connect_insecure(host.addr, "sunna").await.unwrap();
    let (requests, stream_requests) = tokio::sync::watch::channel(None);
    let live = Arc::new(Mutex::new(LiveStats::default()));
    let (frames_tx, mut frames) = tokio::sync::mpsc::unbounded_channel();
    let tiles = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let received_tiles = Arc::clone(&tiles);
    let (_input, input_rx) = tokio::sync::mpsc::unbounded_channel();
    let options = ClientOptions {
        token: "secret".into(),
        stream: StreamSettings {
            codec: Some("raw".into()),
            max_size: Some((320, 180)),
            ..Default::default()
        },
        stream_requests: Some(stream_requests),
        live: Some(Arc::clone(&live)),
        ..Default::default()
    };
    let connection = client.connection.clone();
    let running = tokio::spawn(run_client(
        connection,
        options,
        move |frame| {
            let _ = frames_tx.send(frame);
        },
        move |_| {
            received_tiles.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        },
        input_rx,
    ));
    let first = frame_at(&mut frames, 320, 180).await;
    requests
        .send(Some(StreamSettings {
            max_size: Some((160, 90)),
            ..Default::default()
        }))
        .unwrap();
    wait_until(|| live.lock().unwrap().epoch == 1).await;
    let resized = frame_at(&mut frames, 160, 90).await;
    assert!(resized > first);
    requests
        .send(Some(StreamSettings {
            codec: Some("h264".into()),
            ..Default::default()
        }))
        .unwrap();
    wait_until(|| live.lock().unwrap().epoch == 2).await;
    while frames.try_recv().is_ok() {}
    let encoded = frame_at(&mut frames, 160, 90).await;
    assert!(encoded > resized);
    assert_eq!(live.lock().unwrap().codec, "h264");

    for (settings, reason) in [
        (
            StreamSettings {
                codec: Some("bogus".into()),
                ..Default::default()
            },
            "unknown codec",
        ),
        (
            StreamSettings {
                fps: Some(13),
                ..Default::default()
            },
            "test source failure",
        ),
        (
            StreamSettings {
                max_bitrate_kbps: Some(123),
                ..Default::default()
            },
            "test encoder failure",
        ),
    ] {
        requests.send(Some(settings)).unwrap();
        wait_until(|| {
            live.lock()
                .unwrap()
                .last_stream_error
                .as_deref()
                .is_some_and(|error| error.contains(reason))
        })
        .await;
        assert_eq!(live.lock().unwrap().epoch, 2);
        while frames.try_recv().is_ok() {}
        frame_at(&mut frames, 160, 90).await;
    }

    requests
        .send(Some(StreamSettings {
            fps: Some(25),
            max_bitrate_kbps: Some(1500),
            fast_lane: Some(true),
            ..Default::default()
        }))
        .unwrap();
    wait_until(|| live.lock().unwrap().epoch == 3).await;
    wait_until(|| tiles.load(std::sync::atomic::Ordering::Relaxed) > 0).await;
    assert!(live.lock().unwrap().fast_lane);
    assert!(live.lock().unwrap().last_stream_error.is_none());
    let built = host.builds.lock().unwrap().last().unwrap().clone();
    assert_eq!(
        (built.width, built.height, built.fps, built.bitrate_bps),
        (160, 90, 25, 1_500_000)
    );
    requests
        .send(Some(StreamSettings {
            fast_lane: Some(false),
            ..Default::default()
        }))
        .unwrap();
    wait_until(|| live.lock().unwrap().epoch == 4).await;
    assert!(!live.lock().unwrap().fast_lane);
    // Let already-delivered tile batches drain before checking the disabled lane.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let before = tiles.load(std::sync::atomic::Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(tiles.load(std::sync::atomic::Ordering::Relaxed), before);
    requests
        .send(Some(StreamSettings {
            fast_lane: Some(true),
            ..Default::default()
        }))
        .unwrap();
    wait_until(|| live.lock().unwrap().epoch == 5).await;
    wait_until(|| tiles.load(std::sync::atomic::Ordering::Relaxed) > before).await;
    frame_at(&mut frames, 160, 90).await;
    client.connection.close(0u32.into(), b"test done");
    let report = tokio::time::timeout(TIMEOUT, running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(report.decode_errors, 0);
    assert_eq!(report.info.epoch, 5);
    assert_eq!(report.info.fps, 25);
}

async fn hello(
    addr: SocketAddr,
) -> (
    sunna_transport::ClientConnection,
    ControlChannel,
    ControlMessage,
) {
    let client = connect_insecure(addr, "sunna").await.unwrap();
    let mut control = ControlChannel::open(&client.connection).await.unwrap();
    control
        .send(&ControlMessage::Hello {
            version: sunna_proto::PROTOCOL_VERSION,
            name: "test-client".into(),
            token: "secret".into(),
            stream: StreamSettings::default(),
        })
        .await
        .unwrap();
    let response = tokio::time::timeout(TIMEOUT, control.recv())
        .await
        .unwrap()
        .unwrap();
    (client, control, response)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn probe_auth_busy_refusal_and_slot_release() {
    let host = Host::start();
    let idle = probe(host.addr, "sunna", "secret", TIMEOUT).await.unwrap();
    assert!(idle.token_ok);
    assert!(!idle.busy);
    assert_eq!(idle.name, "test-host");
    assert_eq!(idle.version, sunna_proto::PROTOCOL_VERSION);
    // The right token also learns what the host is.
    let about = idle.about.expect("host info after a good probe");
    assert_eq!((about.width, about.height), (320, 180));
    let wrong = probe(host.addr, "sunna", "wrong!", TIMEOUT).await.unwrap();
    assert!(!wrong.token_ok);
    assert!(wrong.name.is_empty());
    assert!(!wrong.busy);
    assert!(wrong.about.is_none());
    assert!(host.builds.lock().unwrap().is_empty());

    let (_client, mut control, ack) = hello(host.addr).await;
    assert!(matches!(
        ack,
        ControlMessage::HelloAck {
            fast_lane: false,
            ..
        }
    ));
    let busy = probe(host.addr, "sunna", "secret", TIMEOUT).await.unwrap();
    assert!(busy.busy && busy.token_ok);
    // A wrong token learns nothing more while a session runs.
    let wrong_busy = probe(host.addr, "sunna", "wrong!", TIMEOUT).await.unwrap();
    assert_eq!(
        (
            wrong_busy.name,
            wrong_busy.busy,
            wrong_busy.token_ok,
            wrong_busy.about
        ),
        (
            wrong.name.clone(),
            wrong.busy,
            wrong.token_ok,
            wrong.about.clone()
        )
    );
    let (_second, _second_control, refused) = hello(host.addr).await;
    assert_eq!(
        refused,
        ControlMessage::Refused {
            reason: "busy: another viewer is connected".into()
        }
    );
    assert_eq!(host.builds.lock().unwrap().len(), 1);

    control.send(&ControlMessage::Bye).await.unwrap();
    tokio::time::timeout(TIMEOUT, async {
        while probe(host.addr, "sunna", "secret", TIMEOUT)
            .await
            .unwrap()
            .busy
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let (_third, mut third_control, ack) = hello(host.addr).await;
    assert!(matches!(ack, ControlMessage::HelloAck { .. }));
    third_control.send(&ControlMessage::Bye).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsupported_source_reports_fast_lane_off() {
    let host = Host::with_tiles(false);
    let client = connect_insecure(host.addr, "sunna").await.unwrap();
    let mut control = ControlChannel::open(&client.connection).await.unwrap();
    control
        .send(&ControlMessage::Hello {
            version: sunna_proto::PROTOCOL_VERSION,
            name: "test-client".into(),
            token: "secret".into(),
            stream: StreamSettings {
                fast_lane: Some(true),
                ..Default::default()
            },
        })
        .await
        .unwrap();
    let ack = tokio::time::timeout(TIMEOUT, control.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        ack,
        ControlMessage::HelloAck {
            fast_lane: false,
            ..
        }
    ));
    control
        .send(&ControlMessage::SetStream(StreamSettings {
            fast_lane: Some(true),
            fps: Some(25),
            ..Default::default()
        }))
        .await
        .unwrap();
    let changed = tokio::time::timeout(TIMEOUT, async {
        loop {
            let message = control.recv().await.unwrap();
            if matches!(message, ControlMessage::StreamChanged { .. }) {
                break message;
            }
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        changed,
        ControlMessage::StreamChanged {
            fast_lane: false,
            fps: 25,
            ..
        }
    ));
    control.send(&ControlMessage::Bye).await.unwrap();
}
