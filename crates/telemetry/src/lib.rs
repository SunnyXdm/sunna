//! Logging setup plus optional remote telemetry for dogfooding.
//!
//! Every binary calls [`init`] once. Logs always go to stderr; when a log
//! server is configured they are also shipped, as NDJSON batches, to a
//! collector (tools/logd) so the host and viewer of one session can be
//! analysed side by side.
//!
//! Shipping never blocks the media pipeline: events go into a bounded
//! in-memory queue (oldest dropped when full) drained by a background thread.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Events held for shipping; beyond this the oldest are dropped (and counted).
const QUEUE_CAPACITY: usize = 20_000;
/// Events per POST.
const BATCH_MAX: usize = 2_000;
const SHIP_INTERVAL: Duration = Duration::from_secs(1);
/// What goes to the collector unless `SUNNA_REMOTE_LOG` overrides it.
const DEFAULT_REMOTE_FILTER: &str = "info,sunna_host=debug,sunna_client=debug,\
     sunna_capture=debug,sunna_codec=debug,sunna_input=debug,sunna_transport=debug,\
     sunnad=debug,sunna_cli=debug";

/// Where to ship logs: the collector base URL (`http://host:port`) and its token.
#[derive(Debug, Clone)]
pub struct Remote {
    pub url: String,
    pub token: String,
}

impl Remote {
    /// `SUNNA_LOG_URL` + `SUNNA_LOG_TOKEN`, if both are set and non-empty.
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("SUNNA_LOG_URL").ok()?;
        let token = std::env::var("SUNNA_LOG_TOKEN").ok()?;
        (!url.is_empty() && !token.is_empty()).then(|| Self {
            url: url.trim_end_matches('/').to_string(),
            token,
        })
    }
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    run_id: String,
    started: Instant,
}

#[derive(Default)]
struct Queue {
    events: VecDeque<Value>,
    dropped: u64,
    /// A batch has been taken off the queue and is being posted.
    in_flight: bool,
    shutdown: bool,
}

static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();

/// Keeps the shipper alive; dropping it flushes what's left (bounded wait).
pub struct Guard {
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        flush();
        if let Some(shared) = SHARED.get() {
            shared.queue.lock().unwrap().shutdown = true;
            shared.wake.notify_all();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Install logging. `role` names this process in the collector ("host",
/// "viewer", "bench"...). Returns a guard to hold until exit.
pub fn init(role: &str, remote: Option<Remote>) -> Guard {
    let stderr_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let stderr = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(stderr_filter);

    let Some(remote) = remote else {
        tracing_subscriber::registry().with(stderr).init();
        warn_if_awdl_active();
        return Guard { thread: None };
    };

    let shared = Arc::new(Shared {
        queue: Mutex::new(Queue::default()),
        wake: Condvar::new(),
        run_id: run_id(),
        started: Instant::now(),
    });
    let _ = SHARED.set(Arc::clone(&shared));

    let remote_filter = std::env::var("SUNNA_REMOTE_LOG")
        .ok()
        .and_then(|value| EnvFilter::try_new(value).ok())
        .unwrap_or_else(|| EnvFilter::new(DEFAULT_REMOTE_FILTER));
    tracing_subscriber::registry()
        .with(stderr)
        .with(RemoteLayer.with_filter(remote_filter))
        .init();

    push(json!({ "kind": "env", "env": environment(role) }));
    install_panic_hook();

    let thread = {
        let shared = Arc::clone(&shared);
        let role = role.to_string();
        std::thread::Builder::new()
            .name("sunna-telemetry".into())
            .spawn(move || ship_loop(shared, remote, role))
            .ok()
    };
    tracing::info!(
        run = %shared.run_id,
        "remote telemetry enabled"
    );
    warn_if_awdl_active();
    Guard { thread }
}

/// Ship everything queued so far, waiting at most a few seconds. Call before
/// `std::process::exit`, which skips destructors.
pub fn flush() {
    let Some(shared) = SHARED.get() else { return };
    shared.wake.notify_all();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let queue = shared.queue.lock().unwrap();
        if queue.events.is_empty() && !queue.in_flight {
            return;
        }
        drop(queue);
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn push(mut record: Value) {
    let Some(shared) = SHARED.get() else { return };
    if let Value::Object(map) = &mut record {
        map.insert("t".into(), json!(unix_ms()));
        map.insert(
            "mono_ms".into(),
            json!(shared.started.elapsed().as_millis() as u64),
        );
    }
    let mut queue = shared.queue.lock().unwrap();
    if queue.events.len() >= QUEUE_CAPACITY {
        queue.events.pop_front();
        queue.dropped += 1;
    }
    queue.events.push_back(record);
    if queue.events.len() >= BATCH_MAX {
        shared.wake.notify_all();
    }
}

fn ship_loop(shared: Arc<Shared>, remote: Remote, role: String) {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .build();
    let url = format!("{}/ingest", remote.url);
    let device = hostname();
    let mut backoff = SHIP_INTERVAL;
    loop {
        let (batch, dropped, shutdown) = {
            let mut queue = shared.queue.lock().unwrap();
            if queue.events.is_empty() && !queue.shutdown {
                queue = shared.wake.wait_timeout(queue, backoff).unwrap().0;
            }
            let take = queue.events.len().min(BATCH_MAX);
            let batch: Vec<Value> = queue.events.drain(..take).collect();
            queue.in_flight = !batch.is_empty();
            (batch, std::mem::take(&mut queue.dropped), queue.shutdown)
        };
        if batch.is_empty() {
            if shutdown {
                return;
            }
            continue;
        }
        let mut body = String::new();
        if dropped > 0 {
            body.push_str(&json!({ "kind": "dropped", "count": dropped }).to_string());
            body.push('\n');
        }
        for record in &batch {
            body.push_str(&record.to_string());
            body.push('\n');
        }
        let result = agent
            .post(&url)
            .set("Authorization", &format!("Bearer {}", remote.token))
            .set("X-Sunna-Device", &device)
            .set("X-Sunna-Role", &role)
            .set("X-Sunna-Run", &shared.run_id)
            .set("Content-Type", "application/x-ndjson")
            .send_string(&body);
        match result {
            Ok(_) => {
                backoff = SHIP_INTERVAL;
                shared.queue.lock().unwrap().in_flight = false;
            }
            Err(error) => {
                // Put the batch back (bounded) and retry later; never log
                // through tracing here, it would feed the queue we're draining.
                eprintln!("telemetry: ship failed: {error}");
                let mut queue = shared.queue.lock().unwrap();
                for record in batch.into_iter().rev() {
                    if queue.events.len() >= QUEUE_CAPACITY {
                        queue.dropped += 1;
                        continue;
                    }
                    queue.events.push_front(record);
                }
                queue.dropped += dropped;
                queue.in_flight = false;
                if shutdown {
                    return;
                }
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
        }
    }
}

struct RemoteLayer;

impl<S: tracing::Subscriber> Layer<S> for RemoteLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        push(json!({
            "kind": "log",
            "lvl": metadata.level().as_str(),
            "tgt": metadata.target(),
            "msg": visitor.message,
            "f": Value::Object(visitor.fields),
        }));
    }
}

#[derive(Default)]
struct FieldVisitor {
    message: String,
    fields: Map<String, Value>,
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let text = format!("{value:?}");
        if field.name() == "message" {
            self.message = text;
        } else {
            self.fields.insert(field.name().into(), json!(text));
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields.insert(field.name().into(), json!(value));
        }
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.insert(field.name().into(), json!(value));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.insert(field.name().into(), json!(value));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields.insert(field.name().into(), json!(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.insert(field.name().into(), json!(value));
    }
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        push(json!({
            "kind": "panic",
            "msg": info.to_string(),
            "backtrace": std::backtrace::Backtrace::force_capture().to_string(),
        }));
        flush();
        previous(info);
    }));
}

fn environment(role: &str) -> Value {
    json!({
        "role": role,
        "version": env!("CARGO_PKG_VERSION"),
        "git": option_env!("SUNNA_GIT_HASH").unwrap_or("unknown"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "os_version": command_output("sw_vers", &["-productVersion"])
            .or_else(|| command_output("uname", &["-r"])),
        "hw_model": command_output("sysctl", &["-n", "hw.model"]),
        "cpu": command_output("sysctl", &["-n", "machdep.cpu.brand_string"]),
        "hostname": hostname(),
        "awdl_active": awdl_active(),
        "pid": std::process::id(),
        "args": std::env::args().collect::<Vec<_>>(),
    })
}

/// Whether AWDL (Apple's peer-to-peer Wi-Fi for AirDrop/Continuity) is up.
/// While active, the Wi-Fi radio periodically leaves the network's channel,
/// which showed up in dogfooding as ~150-190 ms hitches every second.
pub fn awdl_active() -> Option<bool> {
    if std::env::consts::OS != "macos" {
        return None;
    }
    let output = std::process::Command::new("ifconfig").arg("awdl0").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    Some(text.contains("status: active") || (text.contains("<UP") && !text.contains("inactive")))
}

fn warn_if_awdl_active() {
    if awdl_active() == Some(true) {
        tracing::warn!(
            "AWDL (AirDrop/Continuity Wi-Fi) is active: expect ~150 ms hitches every second \
             over Wi-Fi. For a smooth session run `sudo ifconfig awdl0 down` \
             (AirDrop stays off until `sudo ifconfig awdl0 up` or a reboot)."
        );
    }
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program).args(args).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !text.is_empty()).then_some(text)
}

fn hostname() -> String {
    command_output("hostname", &[])
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "unknown".into())
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

fn run_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    format!("{:x}-{:x}", nanos, std::process::id())
}
