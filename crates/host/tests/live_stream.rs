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

const TEST_KEY: &str = "36363636363636363636363636363636";
const TIMEOUT: Duration = Duration::from_secs(10);

struct Host {
    addr: SocketAddr,
    cert: sunna_transport::quinn::rustls::pki_types::CertificateDer<'static>,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
    builds: Arc<Mutex<Vec<StreamConfig>>>,
}

// The synthetic source's tile generator is only a transport fixture.
struct TileSource {
    source: SyntheticSource,
    no_picture: bool,
}

impl FrameSource for TileSource {
    fn next_frame(&mut self) -> anyhow::Result<sunna_capture::VideoFrame> {
        if self.no_picture {
            std::thread::sleep(Duration::from_millis(30));
            return Err(sunna_capture::NoPicture.into());
        }
        self.source.next_frame()
    }
    fn width(&self) -> u32 {
        self.source.width()
    }
    fn height(&self) -> u32 {
        self.source.height()
    }
    fn fps(&self) -> u32 {
        self.source.fps()
    }
    fn supports_tiles(&self) -> bool {
        true
    }
    fn set_tile_sink(&mut self, sink: sunna_capture::TileSink) {
        self.source.set_tile_sink(sink);
    }
}

impl Host {
    async fn legacy(&self) -> sunna_transport::ClientConnection {
        use sunna_transport::quinn::{self, rustls};
        let mut roots = rustls::RootCertStore::empty();
        roots.add(self.cert.clone()).unwrap();
        let mut tls = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(&[&rustls::version::TLS13]).unwrap()
            .with_root_certificates(roots).with_no_client_auth();
        tls.alpn_protocols = vec![sunna_proto::ALPN.to_vec()];
        let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap();
        let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(crypto)));
        let connection = endpoint.connect(self.addr, "sunna").unwrap().await.unwrap();
        sunna_transport::ClientConnection { endpoint, connection }
    }

    fn start() -> Self {
        Self::with_tiles(true)
    }

    fn with_tiles(supports_tiles: bool) -> Self {
        Self::with_options(supports_tiles, false)
    }

    fn with_options(supports_tiles: bool, audio: bool) -> Self {
        Self::with_source(supports_tiles, audio, false, false)
    }

    fn with_source(supports_tiles: bool, audio: bool, no_picture: bool, require_proof: bool) -> Self {
        let server = Server::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let cert = server.cert.clone();
        let addr = server.local_addr().unwrap();
        let builds = Arc::new(Mutex::new(Vec::new()));
        let built = Arc::clone(&builds);
        let task = tokio::spawn(run_host(
            server,
            HostConfig {
                require_proof,
                clipboard: false,
                name: "test-host".into(),
                token: TEST_KEY.into(),
                width: 320,
                height: 180,
                fps: 20,
                codec: "raw".into(),
                max_bitrate_bps: 2_000_000,
                fast_lane: false,
                simulate_loss: 0.0,
                about: Default::default(),
                audio,
                display_follows_viewer: false,
            },
            Box::new(move |config| {
                // Exercise a source failure after validation has succeeded.
                anyhow::ensure!(config.fps != 13, "test source failure");
                built.lock().unwrap().push(config.clone());
                let source = SyntheticSource::new(config.width, config.height, config.fps);
                if supports_tiles {
                    Ok(Box::new(TileSource { source, no_picture }) as Box<dyn FrameSource>)
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
        Self { addr, cert, task, builds }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn idle_handshake_closes_and_leaves_capacity_for_an_authenticated_probe() {
    let host = Host::start();
    let idle = connect_insecure(host.addr, "sunna").await.unwrap();
    let close = tokio::time::timeout(Duration::from_secs(12), idle.connection.closed()).await.unwrap();
    assert!(matches!(close, sunna_transport::quinn::ConnectionError::ApplicationClosed(reason)
        if reason.reason.as_ref() == b"sign-in timeout"));
    assert!(probe(host.addr, "sunna", TEST_KEY, TIMEOUT).await.unwrap().token_ok);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_stuck_without_a_picture_releases_the_slot() {
    let host = Host::with_source(true, false, true, false);
    let client = host.legacy().await;
    let mut control = ControlChannel::open(&client.connection).await.unwrap();
    control.send(&ControlMessage::Hello {
        version: sunna_proto::PROTOCOL_VERSION, name: "waiting".into(), token: TEST_KEY.into(),
        stream: Default::default(), device: String::new(),
    }).await.unwrap();
    assert!(matches!(control.recv().await.unwrap(), ControlMessage::HelloAck { .. }));
    let close = tokio::time::timeout(Duration::from_secs(35), client.connection.closed()).await.unwrap();
    assert!(matches!(close, sunna_transport::quinn::ConnectionError::ApplicationClosed(_)));
    let state = probe(host.addr, "sunna", TEST_KEY, TIMEOUT).await.unwrap();
    assert!(!state.busy);
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
                // Software decoders hand over I420 planes; others BGRA.
                let bytes_per_4_pixels = match frame.format {
                    sunna_capture::PixelFormat::I420 => 6,
                    _ => 16,
                };
                assert_eq!(
                    frame.data.to_cpu().unwrap().len(),
                    (width * height / 4 * bytes_per_4_pixels) as usize
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
        device: String::new(),
        leave: None,
        token: TEST_KEY.into(),
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
    host: &Host,
) -> (
    sunna_transport::ClientConnection,
    ControlChannel,
    ControlMessage,
) {
    hello_from(host, "test-client", "").await
}

/// A Hello from the viewer called `name` on computer `device`.
async fn hello_from(
    host: &Host,
    name: &str,
    device: &str,
) -> (
    sunna_transport::ClientConnection,
    ControlChannel,
    ControlMessage,
) {
    let client = host.legacy().await;
    let mut control = ControlChannel::open(&client.connection).await.unwrap();
    control
        .send(&ControlMessage::Hello {
            version: sunna_proto::PROTOCOL_VERSION,
            name: name.into(),
            token: TEST_KEY.into(),
            stream: StreamSettings::default(),
            device: device.into(),
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
    let idle = probe(host.addr, "sunna", TEST_KEY, TIMEOUT).await.unwrap();
    assert!(idle.token_ok);
    assert!(!idle.busy);
    assert_eq!(idle.name, "test-host");
    assert_eq!(idle.version, sunna_proto::PROTOCOL_VERSION);
    // The right token also learns what the host is.
    let about = idle.about.expect("host info after a good probe");
    assert_eq!((about.width, about.height), (320, 180));
    let wrong = probe(host.addr, "sunna", &"00".repeat(16), TIMEOUT).await.unwrap_err();
    assert_eq!(wrong.to_string(), sunna_client::auth::WRONG_KEY);
    assert!(host.builds.lock().unwrap().is_empty());

    let (_client, mut control, ack) = hello(&host).await;
    assert!(matches!(
        ack,
        ControlMessage::HelloAck {
            fast_lane: false,
            ..
        }
    ));
    let busy = probe(host.addr, "sunna", TEST_KEY, TIMEOUT).await.unwrap();
    assert!(busy.busy && busy.token_ok);
    // A wrong token learns nothing more while a session runs.
    let wrong_busy = probe(host.addr, "sunna", &"00".repeat(16), TIMEOUT).await.unwrap_err();
    assert_eq!(wrong_busy.to_string(), sunna_client::auth::WRONG_KEY);

    let (_second, _second_control, refused) = hello(&host).await;
    assert_eq!(
        refused,
        ControlMessage::Refused {
            reason: "busy: test-client is connected".into()
        }
    );
    assert_eq!(host.builds.lock().unwrap().len(), 1);

    control.send(&ControlMessage::Bye).await.unwrap();
    tokio::time::timeout(TIMEOUT, async {
        while probe(host.addr, "sunna", TEST_KEY, TIMEOUT)
            .await
            .unwrap()
            .busy
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let (_third, mut third_control, ack) = hello(&host).await;
    assert!(matches!(ack, ControlMessage::HelloAck { .. }));
    third_control.send(&ControlMessage::Bye).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsupported_source_reports_fast_lane_off() {
    let host = Host::with_tiles(false);
    let client = host.legacy().await;
    let mut control = ControlChannel::open(&client.connection).await.unwrap();
    control
        .send(&ControlMessage::Hello {
            version: sunna_proto::PROTOCOL_VERSION,
            name: "test-client".into(),
            token: TEST_KEY.into(),
            stream: StreamSettings {
                fast_lane: Some(true),
                ..Default::default()
            },
            device: String::new(),
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

/// Audio datagrams that arrive within `window` and decode.
async fn sound_heard(
    connection: &sunna_transport::quinn::Connection,
    decoder: &mut sunna_audio::Decoder,
    window: Duration,
) -> usize {
    use sunna_proto::media::{parse_audio, MediaHeader, FLAG_AUDIO};
    let mut heard = 0;
    let mut pcm = [0i16; sunna_audio::FRAME_LEN];
    let deadline = tokio::time::Instant::now() + window;
    while let Ok(Ok(datagram)) = tokio::time::timeout_at(deadline, connection.read_datagram()).await {
        let Some((header, payload)) = MediaHeader::parse(&datagram) else { continue };
        if header.flags & FLAG_AUDIO == 0 {
            continue;
        }
        let (current, _previous) = parse_audio(payload).expect("well-formed audio payload");
        decoder.decode(Some(current), &mut pcm).expect("decodable Opus");
        heard += 1;
    }
    heard
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sound_flows_and_switches_off_and_on() {
    // The test tone instead of this machine's sound server.
    std::env::set_var("SUNNA_AUDIO_SOURCE", "synthetic");
    let host = Host::with_options(true, true);
    let client = host.legacy().await;
    let mut control = ControlChannel::open(&client.connection).await.unwrap();
    control
        .send(&ControlMessage::Hello {
            version: sunna_proto::PROTOCOL_VERSION,
            name: "test-client".into(),
            token: TEST_KEY.into(),
            stream: StreamSettings { audio: Some(true), ..Default::default() },
            device: String::new(),
        })
        .await
        .unwrap();
    let ack = tokio::time::timeout(TIMEOUT, control.recv()).await.unwrap().unwrap();
    assert!(matches!(ack, ControlMessage::HelloAck { audio: true, .. }), "{ack:?}");

    let mut decoder = sunna_audio::Decoder::new().unwrap();
    // 10 ms packets: about 50 in half a second.
    let heard = sound_heard(&client.connection, &mut decoder, Duration::from_millis(500)).await;
    assert!(heard >= 30, "heard only {heard} packets");

    control.send(&ControlMessage::SetAudio(false)).await.unwrap();
    sound_heard(&client.connection, &mut decoder, Duration::from_millis(200)).await;
    let while_off = sound_heard(&client.connection, &mut decoder, Duration::from_millis(300)).await;
    assert_eq!(while_off, 0, "sound kept flowing after SetAudio(false)");

    control.send(&ControlMessage::SetAudio(true)).await.unwrap();
    let again = sound_heard(&client.connection, &mut decoder, Duration::from_millis(500)).await;
    assert!(again >= 30, "heard only {again} packets after switching back on");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_viewer_that_wants_no_sound_gets_none() {
    std::env::set_var("SUNNA_AUDIO_SOURCE", "synthetic");
    let host = Host::with_options(true, true);
    let client = host.legacy().await;
    let mut control = ControlChannel::open(&client.connection).await.unwrap();
    control
        .send(&ControlMessage::Hello {
            version: sunna_proto::PROTOCOL_VERSION,
            name: "test-client".into(),
            token: TEST_KEY.into(),
            stream: StreamSettings { audio: Some(false), ..Default::default() },
            device: String::new(),
        })
        .await
        .unwrap();
    let ack = tokio::time::timeout(TIMEOUT, control.recv()).await.unwrap().unwrap();
    assert!(matches!(ack, ControlMessage::HelloAck { audio: false, .. }), "{ack:?}");
    let mut decoder = sunna_audio::Decoder::new().unwrap();
    assert_eq!(sound_heard(&client.connection, &mut decoder, Duration::from_millis(300)).await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_viewer_takes_back_its_own_session() {
    let host = Host::start();
    let (first, _first_control, ack) = hello_from(&host, "Sunny's MacBook Air", "mac-1").await;
    assert!(matches!(ack, ControlMessage::HelloAck { .. }), "{ack:?}");

    // Probes say who's connected.
    let busy = probe(host.addr, "sunna", TEST_KEY, TIMEOUT).await.unwrap();
    assert!(busy.busy);
    assert_eq!(busy.viewer, Some(("Sunny's MacBook Air".into(), "mac-1".into())));

    // Another computer is refused, by name.
    let (_other, _other_control, refused) = hello_from(&host, "Priya's Mac", "mac-2").await;
    assert_eq!(refused, ControlMessage::Refused { reason: "busy: Sunny's MacBook Air is connected".into() });

    // The same computer (its old session left behind) takes over at once.
    let started = std::time::Instant::now();
    let (_again, mut again_control, ack) = hello_from(&host, "Sunny's MacBook Air", "mac-1").await;
    assert!(matches!(ack, ControlMessage::HelloAck { .. }), "{ack:?}");
    assert!(started.elapsed() < Duration::from_secs(3), "took {:?}", started.elapsed());
    tokio::time::timeout(TIMEOUT, first.connection.closed()).await.expect("the old session is ended");
    again_control.send(&ControlMessage::Bye).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_viewer_that_leaves_frees_the_host_at_once() {
    let host = Host::start();
    let client = connect_insecure(host.addr, "sunna").await.unwrap();
    let (leave, leave_rx) = tokio::sync::watch::channel(false);
    let (_input, input_rx) = tokio::sync::mpsc::unbounded_channel();
    let options = ClientOptions {
        name: "leaver".into(),
        device: "mac-1".into(),
        leave: Some(leave_rx),
        token: TEST_KEY.into(),
        stream: StreamSettings { codec: Some("raw".into()), max_size: Some((320, 180)), ..Default::default() },
        ..Default::default()
    };
    let session = tokio::spawn(run_client(client.connection.clone(), options, |_| {}, |_| {}, input_rx));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(probe(host.addr, "sunna", TEST_KEY, TIMEOUT).await.unwrap().busy);

    leave.send(true).unwrap();
    tokio::time::timeout(TIMEOUT, session).await.unwrap().unwrap().unwrap();
    // Another computer gets in straight away: no waiting for a timeout.
    let started = std::time::Instant::now();
    let (_next, mut next_control, ack) = hello_from(&host, "Priya's Mac", "mac-2").await;
    assert!(matches!(ack, ControlMessage::HelloAck { .. }), "{ack:?}");
    assert!(started.elapsed() < Duration::from_secs(2), "took {:?}", started.elapsed());
    next_control.send(&ControlMessage::Bye).await.unwrap();
}

#[tokio::test]
async fn probe_dispatches_update_after_busy_viewer_disappears() {
    let server = Server::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = server.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let connection = server.endpoint.accept().await.unwrap().await.unwrap();
        let mut control = ControlChannel::accept(&connection).await.unwrap();
        let ControlMessage::AuthProof { nonce, proof } = control.recv().await.unwrap() else { panic!("Expected proof") };
        let (nonce, proof) = sunna_transport::auth::answer(&connection, TEST_KEY, &nonce, &proof.0).unwrap();
        control.send(&ControlMessage::AuthProofAck { nonce: nonce.to_vec(), proof: sunna_proto::messages::Proof(proof) }).await.unwrap();
        assert!(matches!(control.recv().await.unwrap(), ControlMessage::Probe { token } if token.is_empty()));
        control.send(&ControlMessage::ProbeAck { name: "fixture".into(), version: 8, busy: true, token_ok: true }).await.unwrap();
        control.send(&ControlMessage::HostInfo(Default::default())).await.unwrap();
        control.send(&ControlMessage::UpdateInfo("available".into())).await.unwrap();
        let _ = control.finish().await;
        connection.closed().await;
    });
    let result = probe(addr, "sunna", TEST_KEY, TIMEOUT).await.unwrap();
    assert_eq!(result.update.as_deref(), Some("available"));
    task.await.unwrap();
}

#[tokio::test]
async fn rejected_proofs_use_the_sign_in_budget() {
    let host = Host::start();
    for len in [31, 33, 32] {
        let client = connect_insecure(host.addr, "sunna").await.unwrap();
        let mut channel = ControlChannel::open(&client.connection).await.unwrap();
        channel.send(&ControlMessage::AuthProof {
            nonce: vec![0; len],
            proof: sunna_proto::messages::Proof(vec![0; 32]),
        }).await.unwrap();
        assert!(matches!(channel.recv().await.unwrap(), ControlMessage::Refused { .. }));
    }
    assert!(connect_insecure(host.addr, "sunna").await.is_err());
}

#[tokio::test]
async fn peer_certificate_alert_preserves_the_sign_in_budget() {
    let host = Host::start();
    let wrong = sunna_transport::auth::encode_pin(&[0; 32]);
    for _ in 0..3 {
        assert!(sunna_transport::connect_peer(host.addr, "sunna", Some(&wrong)).await.is_err());
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert!(probe(host.addr, "sunna", TEST_KEY, TIMEOUT).await.unwrap().token_ok);
}

#[tokio::test]
async fn older_viewers_do_not_lock_out_a_proof_on_the_same_address() {
    let host = Host::with_source(true, false, false, true);
    for _ in 0..3 {
        let client = host.legacy().await;
        let mut control = ControlChannel::open(&client.connection).await.unwrap();
        control.send(&ControlMessage::Probe { token: TEST_KEY.into() }).await.unwrap();
        assert!(matches!(control.recv().await.unwrap(), ControlMessage::Refused { .. }));
    }
    assert!(probe(host.addr, "sunna", TEST_KEY, TIMEOUT).await.unwrap().token_ok);
}

#[tokio::test]
async fn negotiated_proof_is_required_even_in_compatibility_mode() {
    let host = Host::start();
    let client = connect_insecure(host.addr, "sunna").await.unwrap();
    assert!(sunna_transport::auth::supports_proof(&client.connection));
    let mut channel = ControlChannel::open(&client.connection).await.unwrap();
    channel.send(&ControlMessage::Probe { token: TEST_KEY.into() }).await.unwrap();
    assert!(matches!(channel.recv().await.unwrap(), ControlMessage::Refused { .. }));
}

#[tokio::test]
async fn legacy_update_requires_the_same_key_as_a_session() {
    let host = Host::start();
    for token in ["incorrect", TEST_KEY] {
        let client = host.legacy().await;
        let mut control = ControlChannel::open(&client.connection).await.unwrap();
        control.send(&ControlMessage::UpdateRequest { token: token.into(), action: "identity-test".into() }).await.unwrap();
        let ControlMessage::Refused { reason } = control.recv().await.unwrap() else { panic!("Expected refusal"); };
        if token == TEST_KEY {
            assert!(!reason.contains("configured host key"));
        } else {
            assert!(reason.contains("configured host key"));
        }
    }
}

#[tokio::test]
async fn bad_host_proof_stops_the_probe_before_it_sends_an_operation() {
    let server = Server::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = server.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let connection = server.accept().await.unwrap().unwrap();
        let mut control = ControlChannel::accept(&connection).await.unwrap();
        assert!(matches!(control.recv().await.unwrap(), ControlMessage::AuthProof { .. }));
        control.send(&ControlMessage::AuthProofAck {
            nonce: vec![0; 32],
            proof: sunna_proto::messages::Proof(vec![0; 32]),
        }).await.unwrap();
        assert!(tokio::time::timeout(TIMEOUT, control.recv()).await.unwrap().is_err());
    });
    let result = sunna_client::reach::check(&addr.to_string(), TEST_KEY).await;
    assert_eq!(result.state, "identity-changed");
    assert_eq!(result.detail, sunna_transport::auth::IDENTITY_CHANGED);
    task.await.unwrap();
}

#[tokio::test]
async fn verified_update_sends_an_empty_token_on_each_control_stream() {
    let server = Server::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = server.local_addr().unwrap();
    let task = tokio::spawn(async move {
        for is_update in [false, true] {
            let connection = server.accept().await.unwrap().unwrap();
            let mut control = ControlChannel::accept(&connection).await.unwrap();
            let ControlMessage::AuthProof { nonce, proof } = control.recv().await.unwrap() else {
                panic!("Expected an authentication request");
            };
            let (nonce, proof) = sunna_transport::auth::answer(&connection, TEST_KEY, &nonce, &proof.0).unwrap();
            control.send(&ControlMessage::AuthProofAck {
                nonce: nonce.to_vec(),
                proof: sunna_proto::messages::Proof(proof),
            }).await.unwrap();
            let message = control.recv().await.unwrap();
            if is_update {
                assert!(matches!(message, ControlMessage::UpdateRequest { token, action } if token.is_empty() && action == "status"));
            } else {
                assert!(matches!(message, ControlMessage::Probe { token } if token.is_empty()));
                control.send(&ControlMessage::ProbeAck { name: "fixture".into(), version: 8, busy: false, token_ok: true }).await.unwrap();
                control.send(&ControlMessage::HostInfo(Default::default())).await.unwrap();
            }
            control.send(&ControlMessage::UpdateInfo("{}".into())).await.unwrap();
            control.finish().await.unwrap();
        }
    });
    assert_eq!(sunna_client::host_update(addr, TEST_KEY, "status").await.unwrap(), "{}");
    task.await.unwrap();
}

#[tokio::test]
async fn a_proof_from_another_connection_counts_as_a_failed_sign_in() {
    let host = Host::start();
    let first = connect_insecure(host.addr, "sunna").await.unwrap();
    let (nonce, proof) = sunna_transport::auth::client_proof(&first.connection, TEST_KEY).unwrap();
    drop(first);
    for _ in 0..3 {
        let client = host.legacy().await;
        let mut control = ControlChannel::open(&client.connection).await.unwrap();
        control.send(&ControlMessage::AuthProof {
            nonce: nonce.to_vec(),
            proof: sunna_proto::messages::Proof(proof.clone()),
        }).await.unwrap();
        assert!(matches!(control.recv().await.unwrap(), ControlMessage::Refused { .. }));
    }
    assert!(connect_insecure(host.addr, "sunna").await.is_err());
}
