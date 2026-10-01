//! The Android app's native side (`dev.sunna.app.Native` in Kotlin):
//! checking hosts, and running sessions whose picture the phone's decoder
//! draws straight onto the app's SurfaceView.
//!
//! Kotlin asks and this answers; nothing here calls back into Java. During a
//! session the app polls [`changes`](Java_dev_sunna_app_Native_changes) once
//! a frame (one atomic read) and reads the status when it moves.

mod keymap;
#[cfg(target_os = "android")]
mod logcat;

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use jni::objects::{JClass, JObject, JObjectArray, JString};
use jni::sys::{jboolean, jbyteArray, jfloat, jint, jlong, jobjectArray, jstring, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;
use serde_json::json;
use sunna_client::{reach, run_client, ClientOptions, LiveStats};
use sunna_proto::messages::{GesturePhase, InputEvent, MouseButton, StreamSettings};

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("sunna-net")
            .enable_all()
            .build()
            .expect("a tokio runtime")
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    /// Finding the host and opening the connection.
    Reaching,
    /// Connected; waiting for the first picture.
    Starting,
    Streaming,
    Ended,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Phase::Reaching => "reaching",
            Phase::Starting => "starting",
            Phase::Streaming => "streaming",
            Phase::Ended => "ended",
        }
    }
}

struct State {
    phase: Phase,
    /// Why it ended, in words for the app (empty when we left).
    reason: String,
    width: u32,
    height: u32,
}

/// Input in the order it happened. Typed text is paced (8 ms a key when
/// more is waiting), as the desktop viewer types, so apps that read keys one
/// event at a time keep up; everything else goes straight through, behind it.
enum Command {
    Now(Vec<InputEvent>),
    Keys(Vec<InputEvent>),
}

struct Session {
    host_is_mac: bool,
    shortcuts: &'static [sunna_client::keys::Shortcut],
    commands: tokio::sync::mpsc::UnboundedSender<Command>,
    requests: tokio::sync::watch::Sender<Option<StreamSettings>>,
    leave: tokio::sync::watch::Sender<bool>,
    live: Arc<Mutex<LiveStats>>,
    state: Mutex<State>,
    /// Moves on whenever something the app shows changes.
    changes: AtomicU64,
    /// (frames shown, when) at the last status, for the frame rate shown.
    last_shown: Mutex<(u64, Instant)>,
}

impl Session {
    fn bump(&self) {
        self.changes.fetch_add(1, Ordering::Release);
    }

    fn set_phase(&self, phase: Phase) {
        let mut state = self.state.lock().unwrap();
        if state.phase != phase && state.phase != Phase::Ended {
            state.phase = phase;
            drop(state);
            self.bump();
        }
    }

    fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    fn key(&self, code: u16, pressed: bool, repeat: bool) {
        self.send(Command::Keys(vec![InputEvent::Key { scancode: code, pressed, repeat }]));
    }

    fn frame(&self, width: u32, height: u32) {
        let mut state = self.state.lock().unwrap();
        if state.phase == Phase::Streaming && (state.width, state.height) == (width, height) {
            return;
        }
        if state.phase != Phase::Ended {
            state.phase = Phase::Streaming;
        }
        (state.width, state.height) = (width, height);
        drop(state);
        self.bump();
    }

    fn end(&self, reason: String) {
        let mut state = self.state.lock().unwrap();
        state.phase = Phase::Ended;
        state.reason = reason;
        drop(state);
        self.bump();
    }

    fn status(&self) -> serde_json::Value {
        let state = self.state.lock().unwrap();
        let live = self.live.lock().unwrap();
        let decode = sunna_codec_stats();
        let shown_fps = {
            let mut last = self.last_shown.lock().unwrap();
            let elapsed = last.1.elapsed().as_secs_f64();
            let fps = if elapsed > 0.2 { (decode.shown.saturating_sub(last.0)) as f64 / elapsed } else { f64::NAN };
            if elapsed > 0.2 {
                *last = (decode.shown, Instant::now());
            }
            fps
        };
        let software = decode.name.starts_with("c2.android.") || decode.name.starts_with("OMX.google.");
        // The client measures to the decoder's door; MediaCodec's own time
        // (in, out onto the screen) is added here.
        let mut shown = live.clone();
        if let Some(delay) = decode.delay_ms {
            shown.latency_ms = shown.latency_ms.map(|(p50, p95)| (p50 + delay, p95 + delay));
            shown.decode_ms_p50 = Some(delay);
        }
        let mut stats = shown.stats_line();
        if !decode.name.is_empty() {
            stats.push_str(if software { "  ·  software decoder" } else { "  ·  hardware decoder" });
        }
        json!({
            "stats": stats,
            "phase": state.phase.name(),
            "reason": state.reason,
            "width": if live.width > 0 { live.width } else { state.width },
            "height": if live.height > 0 { live.height } else { state.height },
            "codec": live.codec,
            "streamFps": live.stream_fps,
            "fps": live.fps,
            "shownFps": if shown_fps.is_finite() { Some(shown_fps.round()) } else { None },
            "mbps": live.mbps,
            "latencyMs": live.latency_ms.map(|(p50, _)| p50 + decode.delay_ms.unwrap_or(0.0)),
            "decodeMs": decode.delay_ms,
            "rttMs": live.rtt_ms,
            "dropped": live.dropped,
            "audio": live.audio_on,
            "decoder": decode.name,
            "hardware": !decode.name.is_empty() && !software,
            "cursor": live.cursor_changes,
            "streamError": live.last_stream_error,
        })
    }
}

#[cfg(target_os = "android")]
fn sunna_codec_stats() -> sunna_codec::android::DecodeStats {
    sunna_codec::android::stats()
}

#[cfg(not(target_os = "android"))]
fn sunna_codec_stats() -> DecodeStats {
    DecodeStats::default()
}

#[cfg(not(target_os = "android"))]
#[derive(Default)]
struct DecodeStats {
    shown: u64,
    delay_ms: Option<f64>,
    name: String,
}

fn sessions() -> &'static Mutex<HashMap<i64, Arc<Session>>> {
    static SESSIONS: OnceLock<Mutex<HashMap<i64, Arc<Session>>>> = OnceLock::new();
    SESSIONS.get_or_init(Default::default)
}

fn session(handle: jlong) -> Option<Arc<Session>> {
    sessions().lock().unwrap().get(&handle).cloned()
}

/// The words the app shows when a session ends by itself or can't start.
fn friendly(message: &str) -> String {
    let message = message.trim();
    if message.starts_with("lost the connection") {
        "Lost the connection: nothing came back from that computer for 10 seconds. Its network dropped, or it went to sleep.".into()
    } else if message.starts_with("the host ended the session") {
        "That computer ended the session: it restarted, or stopped sharing.".into()
    } else if message.starts_with("replaced") {
        "This session moved to a newer one from this phone.".into()
    } else if message.contains("wrong session token") {
        "The key doesn't match. Edit the computer and paste its key.".into()
    } else if let Some(who) = message.split_once("busy: ").and_then(|(_, rest)| rest.strip_suffix(" is connected")) {
        format!("{who} is connected to that computer right now.")
    } else if message.contains("busy") {
        "Someone else is connected to that computer.".into()
    } else if message.contains("protocol version mismatch") {
        "That computer runs a different version of Sunna. Update both.".into()
    } else if message.contains("no ") && message.contains("decoder") {
        format!("This phone can't play that computer's video ({message}).")
    } else {
        message.to_string()
    }
}

/// Why a session that nobody here ended, ended.
fn why_it_ended(reason: Option<sunna_transport::quinn::ConnectionError>) -> String {
    use sunna_transport::quinn::ConnectionError;
    match reason {
        Some(ConnectionError::TimedOut) => friendly("lost the connection"),
        Some(ConnectionError::ApplicationClosed(close)) if close.reason.starts_with(b"replaced") => friendly("replaced"),
        _ => friendly("the host ended the session"),
    }
}

/// Forward input to the session in order, pacing typed keys.
async fn forward(
    mut commands: tokio::sync::mpsc::UnboundedReceiver<Command>,
    input: tokio::sync::mpsc::UnboundedSender<InputEvent>,
) {
    while let Some(command) = commands.recv().await {
        let (events, paced) = match command {
            Command::Now(events) => (events, false),
            Command::Keys(events) => (events, true),
        };
        for event in events {
            if input.send(event).is_err() {
                return;
            }
        }
        if paced && !commands.is_empty() {
            tokio::time::sleep(Duration::from_millis(8)).await;
        }
    }
}

struct Connect {
    address: String,
    key: String,
    name: String,
    settings: StreamSettings,
}

async fn run(session: Arc<Session>, connect: Connect, input: tokio::sync::mpsc::UnboundedReceiver<InputEvent>, requests: tokio::sync::watch::Receiver<Option<StreamSettings>>, leave: tokio::sync::watch::Receiver<bool>) {
    let asked_to_leave = leave.clone();
    let addr = match reach::resolve(&connect.address).await {
        Ok(addr) => addr,
        Err(check) => return session.end(check.detail),
    };
    tracing::warn!("dev TLS: server certificate is NOT verified");
    let client = match tokio::time::timeout(Duration::from_secs(10), sunna_transport::connect_insecure(addr, "sunna")).await {
        Ok(Ok(client)) => client,
        Ok(Err(error)) => {
            tracing::warn!(%error, "couldn't connect");
            return session.end(format!("Couldn't reach {addr}. Is Sunna sharing on that computer?"));
        }
        Err(_) => return session.end(format!("Nothing answered at {addr} for 10 seconds. Is Sunna sharing on that computer?")),
    };
    if *asked_to_leave.borrow() {
        client.connection.close(0u32.into(), b"bye");
        return session.end(String::new());
    }
    session.set_phase(Phase::Starting);
    let connection = client.connection.clone();
    let watched = client.connection.clone();
    let shown = Arc::clone(&session);
    let woken = Arc::clone(&session);
    let options = ClientOptions {
        // A new pointer shape: the app puts it on.
        wake: Some(sunna_client::Wake(Arc::new(move || woken.bump()))),
        clipboard: false,
        name: connect.name,
        token: connect.key,
        stream: connect.settings,
        stream_requests: Some(requests),
        device: sunna_client::device_id(),
        leave: Some(leave),
        duration: None,
        live: Some(Arc::clone(&session.live)),
    };
    let result = run_client(connection, options, move |frame| shown.frame(frame.width, frame.height), |_| {}, input).await;
    let reason = match result {
        Ok(_) if *asked_to_leave.borrow() => String::new(),
        Ok(_) => why_it_ended(watched.close_reason()),
        Err(error) => {
            tracing::warn!("session error: {error:#}");
            friendly(&format!("{error:#}").replace("host refused the session: ", ""))
        }
    };
    tracing::info!(reason = %reason, "session ended");
    // Let the close reach the host (so it's free at once), without hanging
    // on a dead network.
    let _ = tokio::time::timeout(Duration::from_millis(500), client.endpoint.wait_idle()).await;
    session.end(reason);
}

fn text(env: &mut JNIEnv, value: &JString) -> String {
    if value.is_null() {
        return String::new();
    }
    env.get_string(value).map(String::from).unwrap_or_default()
}

fn java_string(env: &mut JNIEnv, value: &str) -> jstring {
    env.new_string(value).map(|string| string.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Run `body`, keeping a panic from crossing into Java (it would abort the
/// app): it's logged and `fallback` returned instead.
fn guard<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).unwrap_or_else(|panic| {
        let message = panic
            .downcast_ref::<&str>()
            .map(|text| text.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        tracing::error!(%message, "panic in Sunna's native code");
        fallback
    })
}

/// Once, before anything else: where this phone's Sunna state lives (the
/// app's files directory).
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_init(mut env: JNIEnv, _: JClass, files_dir: JString) {
    let dir = text(&mut env, &files_dir);
    guard((), || {
        #[cfg(target_os = "android")]
        logcat::init();
        sunna_client::set_state_dir(std::path::PathBuf::from(dir).join("sunna"));
        tracing::info!(version = env!("CARGO_PKG_VERSION"), protocol = sunna_proto::PROTOCOL_VERSION, "Sunna native library ready");
    })
}

/// Is a host there, does the key fit, and what is it? JSON (`reach::Check`).
/// Blocks for up to a few seconds: call it off the main thread.
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_check(mut env: JNIEnv, _: JClass, address: JString, key: JString) -> jstring {
    let (address, key) = (text(&mut env, &address), text(&mut env, &key));
    let json = guard(String::new(), || {
        let check = runtime().block_on(reach::check(&address, &key));
        serde_json::to_string(&check).unwrap_or_default()
    });
    java_string(&mut env, &json)
}

/// Several at once (the Computers screen): a JSON array, in order.
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_checkAll(mut env: JNIEnv, _: JClass, addresses: JObjectArray, keys: JObjectArray) -> jstring {
    let mut pairs = Vec::new();
    let count = env.get_array_length(&addresses).unwrap_or(0);
    for index in 0..count {
        let address = env.get_object_array_element(&addresses, index).map(JString::from).ok();
        let key = env.get_object_array_element(&keys, index).map(JString::from).ok();
        let address = address.map(|value| text(&mut env, &value)).unwrap_or_default();
        let key = key.map(|value| text(&mut env, &value)).unwrap_or_default();
        pairs.push((address, key));
    }
    let json = guard(String::from("[]"), || {
        let checks = runtime().block_on(async {
            let tasks: Vec<_> = pairs
                .into_iter()
                .map(|(address, key)| tokio::spawn(async move { reach::check(&address, &key).await }))
                .collect();
            let mut checks = Vec::new();
            for task in tasks {
                checks.push(task.await.unwrap_or_else(|_| reach::Check::failed("unreachable", "")));
            }
            checks
        });
        serde_json::to_string(&checks).unwrap_or_else(|_| "[]".into())
    });
    java_string(&mut env, &json)
}

/// "host", "host:port", "[v6]:port" or a `sunna://` link → JSON
/// `{"host", "port"}`, or `{"error"}` in words.
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_parseAddress(mut env: JNIEnv, _: JClass, address: JString) -> jstring {
    let address = text(&mut env, &address);
    let json = match reach::parse(&address) {
        Ok((host, port)) => json!({ "host": host, "port": port }),
        Err(error) => json!({ "error": error }),
    };
    java_string(&mut env, &json.to_string())
}

/// Start a session; returns its handle at once. It connects in the
/// background: watch `changes` and `status`.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_dev_sunna_app_Native_connect(
    mut env: JNIEnv,
    _: JClass,
    address: JString,
    key: JString,
    name: JString,
    host_os: JString,
    codec: JString,
    max_width: jint,
    max_height: jint,
    audio: jboolean,
) -> jlong {
    let address = text(&mut env, &address);
    let key = text(&mut env, &key);
    let name = text(&mut env, &name);
    let host_os = text(&mut env, &host_os);
    let codec = text(&mut env, &codec);
    guard(0, || {
        static NEXT: AtomicI64 = AtomicI64::new(1);
        let handle = NEXT.fetch_add(1, Ordering::Relaxed);
        let settings = StreamSettings {
            codec: (!codec.is_empty()).then_some(codec),
            max_size: (max_width > 0 && max_height > 0).then_some((max_width as u32, max_height as u32)),
            audio: Some(audio == JNI_TRUE),
            // Tiles ahead of the video are for the desktop viewers' own
            // renderers; here the decoder draws the whole picture.
            fast_lane: Some(false),
            ..Default::default()
        };
        let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (input, input_rx) = tokio::sync::mpsc::unbounded_channel();
        let (requests, request_rx) = tokio::sync::watch::channel(None);
        let (leave, leave_rx) = tokio::sync::watch::channel(false);
        let session = Arc::new(Session {
            host_is_mac: sunna_client::keys::host_is_mac(&host_os),
            shortcuts: sunna_client::keys::shortcuts_for(&host_os),
            commands,
            requests,
            leave,
            live: Arc::default(),
            state: Mutex::new(State { phase: Phase::Reaching, reason: String::new(), width: 0, height: 0 }),
            changes: AtomicU64::new(1),
            last_shown: Mutex::new((sunna_codec_stats().shown, Instant::now())),
        });
        sessions().lock().unwrap().insert(handle, Arc::clone(&session));
        let connect = Connect { address, key, name, settings };
        runtime().spawn(forward(command_rx, input));
        runtime().spawn(run(session, connect, input_rx, request_rx, leave_rx));
        handle
    })
}

/// The surface to show sessions on (the SurfaceView's), or null when it goes.
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_setSurface(env: JNIEnv, _: JClass, surface: JObject) {
    #[cfg(target_os = "android")]
    {
        extern "C" {
            fn ANativeWindow_fromSurface(env: *mut jni::sys::JNIEnv, surface: jni::sys::jobject) -> *mut sunna_codec::android::ANativeWindow;
        }
        // SAFETY: a live JNIEnv and Surface (or null); the window reference
        // returned is handed over to the decoder.
        unsafe {
            let window = if surface.is_null() { std::ptr::null_mut() } else { ANativeWindow_fromSurface(env.get_raw(), surface.as_raw()) };
            sunna_codec::android::set_surface(window);
        }
    }
    #[cfg(not(target_os = "android"))]
    let _ = (env, surface);
}

/// Moves on whenever the session's status changes (phase, picture size,
/// pointer shape): cheap enough to read every frame.
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_changes(_: JNIEnv, _: JClass, handle: jlong) -> jlong {
    session(handle).map_or(-1, |session| session.changes.load(Ordering::Acquire) as jlong)
}

/// JSON: phase, why it ended, the stream and its numbers.
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_status(mut env: JNIEnv, _: JClass, handle: jlong) -> jstring {
    let json = guard(String::new(), || session(handle).map(|session| session.status().to_string()).unwrap_or_default());
    java_string(&mut env, &json)
}

/// The host's pointer: a 20-byte little-endian header (id u64, width u16,
/// height u16, hot x u16, hot y u16, the host's screen width u32), then
/// premultiplied RGBA. Null before the host has sent one.
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_cursor(env: JNIEnv, _: JClass, handle: jlong) -> jbyteArray {
    let Some(shape) = session(handle).and_then(|session| session.live.lock().unwrap().cursor.clone()) else {
        return std::ptr::null_mut();
    };
    let mut bytes = Vec::with_capacity(20 + shape.rgba.len());
    bytes.extend_from_slice(&shape.id.to_le_bytes());
    for value in [shape.width, shape.height, shape.hot_x, shape.hot_y] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&shape.screen_width.to_le_bytes());
    bytes.extend_from_slice(&shape.rgba);
    env.byte_array_from_slice(&bytes).map(|array| array.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// The pointer, at (x, y) from 0 to 1 across the host's screen.
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_pointer(_: JNIEnv, _: JClass, handle: jlong, x: jfloat, y: jfloat) {
    if let Some(session) = session(handle) {
        session.send(Command::Now(vec![InputEvent::MouseMoveAbs { x: x.clamp(0.0, 1.0), y: y.clamp(0.0, 1.0) }]));
    }
}

/// 0 left, 1 right, 2 middle, 3 back, 4 forward.
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_button(_: JNIEnv, _: JClass, handle: jlong, button: jint, pressed: jboolean) {
    let button = match button {
        0 => MouseButton::Left,
        1 => MouseButton::Right,
        2 => MouseButton::Middle,
        3 => MouseButton::X1,
        4 => MouseButton::X2,
        _ => return,
    };
    if let Some(session) = session(handle) {
        session.send(Command::Now(vec![InputEvent::MouseButton { button, pressed: pressed == JNI_TRUE }]));
    }
}

/// Scroll by (dx, dy) pixels, positive to move the content right and down
/// (as a finger drags it). `phase`: -1 a wheel, 0 begins, 1 moves, 2 ends,
/// 3 cancelled.
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_scroll(_: JNIEnv, _: JClass, handle: jlong, dx: jfloat, dy: jfloat, phase: jint) {
    let phase = match phase {
        0 => Some(GesturePhase::Begin),
        1 => Some(GesturePhase::Update),
        2 => Some(GesturePhase::End),
        3 => Some(GesturePhase::Cancel),
        _ => None,
    };
    if let Some(session) = session(handle) {
        session.send(Command::Now(vec![InputEvent::Scroll { dx, dy, phase, momentum: false }]));
    }
}

/// A key on a hardware keyboard, by Android keycode. False if the host has
/// no such key (the app then lets Android have it).
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_key(_: JNIEnv, _: JClass, handle: jlong, keycode: jint, pressed: jboolean, repeat: jboolean) -> jboolean {
    let Some(code) = keymap::mac_keycode(keycode) else { return JNI_FALSE };
    if let Some(session) = session(handle) {
        session.key(keymap::for_host(code, session.host_is_mac), pressed == JNI_TRUE, repeat == JNI_TRUE);
    }
    JNI_TRUE
}

/// A key by its Mac keycode, as is (the on-screen modifier and arrow keys).
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_macKey(_: JNIEnv, _: JClass, handle: jlong, code: jint, pressed: jboolean) {
    if let Some(session) = session(handle) {
        session.key(code as u16, pressed == JNI_TRUE, false);
    }
}

/// Type `text` key by key on a US layout. Returns how many characters had
/// no key (é, emoji...).
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_type(mut env: JNIEnv, _: JClass, handle: jlong, value: JString) -> jint {
    let value = text(&mut env, &value);
    let Some(session) = session(handle) else { return 0 };
    let (groups, skipped) = sunna_client::keys::type_text(&value);
    for group in groups {
        session.send(Command::Keys(group));
    }
    skipped as jint
}

/// The host's shortcuts (Switch Apps, Spotlight...), for the menu.
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_shortcuts(mut env: JNIEnv, _: JClass, host_os: JString) -> jobjectArray {
    let host_os = text(&mut env, &host_os);
    let titles: Vec<&str> = sunna_client::keys::shortcuts_for(&host_os).iter().map(|shortcut| shortcut.title).collect();
    let Ok(array) = env.new_object_array(titles.len() as jint, "java/lang/String", JObject::null()) else {
        return std::ptr::null_mut();
    };
    for (index, title) in titles.iter().enumerate() {
        if let Ok(title) = env.new_string(title) {
            let _ = env.set_object_array_element(&array, index as jint, title);
        }
    }
    array.into_raw()
}

/// Press one of the host's shortcuts, by its place in `shortcuts`.
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_shortcut(_: JNIEnv, _: JClass, handle: jlong, index: jint) {
    if let Some(session) = session(handle) {
        if let Some(shortcut) = session.shortcuts.get(index as usize) {
            session.send(Command::Keys(sunna_client::keys::press(shortcut.keys)));
        }
    }
}

/// Ask the host for a different stream: codec ("" for the host's choice),
/// largest size (0 for any), bitrate and frame rate (0 for the host's
/// default), and whether to send sound.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_dev_sunna_app_Native_setStream(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    codec: JString,
    max_width: jint,
    max_height: jint,
    bitrate_mbps: jint,
    fps: jint,
    audio: jboolean,
) {
    let codec = text(&mut env, &codec);
    let Some(session) = session(handle) else { return };
    let settings = StreamSettings {
        codec: (!codec.is_empty()).then_some(codec),
        max_size: (max_width > 0 && max_height > 0).then_some((max_width as u32, max_height as u32)),
        max_bitrate_kbps: (bitrate_mbps > 0).then_some(bitrate_mbps as u32 * 1000),
        fps: (fps > 0).then_some(fps as u32),
        fast_lane: Some(false),
        audio: Some(audio == JNI_TRUE),
    };
    session.requests.send_replace(Some(settings));
}

/// Leave the session: the host is told, so it's free at once.
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_leave(_: JNIEnv, _: JClass, handle: jlong) {
    if let Some(session) = session(handle) {
        let _ = session.leave.send(true);
    }
}

/// Forget the session (leaving it first if it's still on).
#[no_mangle]
pub extern "system" fn Java_dev_sunna_app_Native_close(_: JNIEnv, _: JClass, handle: jlong) {
    if let Some(session) = sessions().lock().unwrap().remove(&handle) {
        let _ = session.leave.send(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_read_like_the_app() {
        assert_eq!(friendly("wrong session token"), "The key doesn't match. Edit the computer and paste its key.");
        assert_eq!(friendly("busy: Priya's Mac is connected"), "Priya's Mac is connected to that computer right now.");
        assert!(friendly("lost the connection: nothing came back").starts_with("Lost the connection"));
    }

    #[tokio::test]
    async fn typed_keys_keep_their_order_behind_each_other() {
        let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (input, mut input_rx) = tokio::sync::mpsc::unbounded_channel();
        let (groups, _) = sunna_client::keys::type_text("ab");
        for group in groups {
            commands.send(Command::Keys(group)).unwrap();
        }
        commands.send(Command::Now(vec![InputEvent::MouseMoveAbs { x: 0.5, y: 0.5 }])).unwrap();
        drop(commands);
        forward(command_rx, input).await;
        let mut events = Vec::new();
        while let Ok(event) = input_rx.try_recv() {
            events.push(event);
        }
        assert_eq!(events.len(), 5);
        assert!(matches!(events[0], InputEvent::Key { scancode: 0x00, pressed: true, .. }));
        assert!(matches!(events[2], InputEvent::Key { scancode: 0x0B, pressed: true, .. }));
        assert!(matches!(events[4], InputEvent::MouseMoveAbs { .. }));
    }
}
