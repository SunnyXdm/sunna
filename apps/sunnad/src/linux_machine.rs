//! The listener owns the viewer, encoder and credential. A session worker
//! uses inherited socket pairs for display/input and media. Neither has a filesystem name.
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::{fs::MetadataExt, net::UnixStream, process::CommandExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use crate::{linux_seat as seat, linux_worker as wire};
use anyhow::{ensure, Context};
use nix::unistd::{Uid, User};
use sunna_capture::{FrameData, FrameSource, PixelFormat, VideoFrame};
use sunna_host::{DesktopState, StreamConfig};
use sunna_input::InputInjector;
use sunna_proto::messages::{ControlMessage, InputEvent};
use tokio::sync::watch;

#[path = "linux_media.rs"]
mod media;

pub(crate) fn start_media_worker(active: sunna_host::SessionCheck) -> anyhow::Result<()> {
    media::serve(active)
}

const NO_DESKTOP: &str = "Its desktop isn't running.";
const NO_GREETER: &str = "Nobody is logged in and the login screen can't be shared on this system.";
const PERMISSION: &str = "Waiting for permission on that computer.";

fn unavailable_session(session: &seat::Session) -> Option<&'static str> {
    if !session.graphical {
        Some(NO_DESKTOP)
    } else if session.greeter && session.wayland {
        Some(NO_GREETER)
    } else if session.ambiguous && !session.greeter {
        Some(
            "Multiple graphical sessions for this account. Sign out of the other session to share.",
        )
    } else {
        // SDDM can have two simultaneous X11 greeters. Each has its own
        // VT-verified X server, and greeters have neither sound nor clipboard.
        None
    }
}

fn publish_status(name: &str, value: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let dir = Path::new("/run/sunna-host");
    let stage = dir.join(format!("{name}.new"));
    std::fs::write(&stage, value)?;
    std::fs::set_permissions(&stage, std::fs::Permissions::from_mode(0o644))?;
    std::fs::rename(stage, dir.join(name))
}

struct Pipe {
    io: UnixStream,
    stream: Option<u64>,
}

struct Worker {
    generation: u64,
    media: Mutex<Option<UnixStream>>,
    greeter: bool,
    native: (u32, u32),
    owner: (String, u32),
    failed: AtomicBool,
    pipe: Mutex<Pipe>,
    shutdown: UnixStream,
    viewer: Mutex<Option<sunna_transport::quinn::Connection>>,
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Running {
    session: seat::Session,
    child: ChildGuard,
    worker: Arc<Worker>,
    size: (u32, u32),
}
impl Drop for Running {
    fn drop(&mut self) {
        if let Some(conn) = self.worker.viewer.lock().unwrap().take() {
            conn.close(0u32.into(), b"session changed");
        }
        // EOF asks the worker to release held input and exit. Child exit
        // acknowledges completion. The guard kills an unresponsive worker.
        let _ = self.worker.shutdown.shutdown(std::net::Shutdown::Both);
        let deadline = std::time::Instant::now() + Duration::from_millis(750);
        while std::time::Instant::now() < deadline && matches!(self.child.0.try_wait(), Ok(None)) {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn trusted_executable() -> anyhow::Result<PathBuf> {
    let path = std::env::current_exe()?;
    ensure!(
        path.starts_with("/opt/sunna"),
        "system sharing requires the root-owned installation under /opt/sunna"
    );
    for ancestor in path.ancestors() {
        let meta = std::fs::symlink_metadata(ancestor)?;
        ensure!(
            meta.uid() == 0 && meta.mode() & 0o022 == 0 && !meta.is_symlink(),
            "unsafe system host installation"
        );
    }
    Ok(path)
}

fn launch(exe: &Path, session: &seat::Session, generation: u64) -> anyhow::Result<Result<Running, &'static str>> {
    let user = User::from_uid(Uid::from_raw(session.uid))?.context("unknown session user")?;
    let (mut socket, child_socket) = UnixStream::pair()?;
    let (media, child_media) = UnixStream::pair()?;
    media.set_read_timeout(Some(Duration::from_millis(600)))?;
    media.set_write_timeout(Some(Duration::from_millis(600)))?;
    let runtime = format!("/run/user/{}", session.uid);
    // A greeter can still be cold when logind first selects it.
    socket.set_read_timeout(Some(Duration::from_secs(10)))?;
    socket.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut command = Command::new(exe);
    command
        .arg("--session-worker")
        .arg(&session.id)
        .env_clear()
        .current_dir("/")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", user.dir)
        .env("SUNNA_WORKER_SESSION", &session.id)
        .env("XDG_VTNR", session.vt.to_string())
        .env("USER", user.name)
        .env("DISPLAY", &session.display)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={runtime}/bus"),
        )
        .env(
            "XDG_SESSION_TYPE",
            if session.wayland { "wayland" } else { "x11" },
        )
        .uid(session.uid)
        .gid(user.gid.as_raw())
        .stdin(Stdio::from(OwnedFd::from(child_socket)))
        .stdout(Stdio::from(OwnedFd::from(child_media)))
        .stderr(Stdio::null());
    // Enforce this even when an administrator starts the binary directly.
    unsafe {
        command.pre_exec(|| {
            if nix::libc::prctl(nix::libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    // CommandExt::uid clears inherited supplementary groups before setuid.
    let child = ChildGuard(command.spawn()?);
    let size = (wire::number(&mut socket)?, wire::number(&mut socket)?);
    if size.0 == 0 {
        let reason = match size.1 {
            wire::PERMISSION if session.wayland => PERMISSION,
            wire::INPUT_MISSING => crate::linux_desktop::INPUT,
            wire::CAPTURE_FAILED => crate::linux_desktop::CAPTURE,
            wire::WRONG_DEPTH => crate::linux_desktop::COLOR,
            _ => NO_DESKTOP,
        };
        return Ok(Err(reason));
    }
    wire::dimensions(size.0, size.1)?;
    socket.set_read_timeout(Some(Duration::from_secs(8)))?;
    let shutdown = socket.try_clone()?;
    let worker = Arc::new(Worker {
        generation,
        media: Mutex::new(Some(media)),
        greeter: session.greeter,
        native: size,
        owner: (session.id.clone(), session.uid),
        failed: AtomicBool::new(false),
        pipe: Mutex::new(Pipe {
            io: socket,
            stream: None,
        }),
        shutdown,
        viewer: Mutex::default(),
    });
    Ok(Ok(Running {
        child,
        worker,
        session: session.clone(),
        size,
    }))
}

pub struct Machine {
    // Odd while subscribed. Each seat change advances by two.
    generation: Mutex<u64>,
    seat: seat::ActiveSeat,
    current: Mutex<Option<Arc<Worker>>>,
    prepared: Mutex<Option<(Weak<Worker>, u64)>>,
    bound: Mutex<Weak<Worker>>,
    next_stream: AtomicU64,
    no_session: AtomicBool,
}

impl Machine {
    pub fn start() -> anyhow::Result<(Arc<Self>, watch::Receiver<DesktopState>)> {
        ensure!(
            Uid::effective().is_root(),
            "system listener requires its system unit"
        );
        let exe = trusted_executable()?;
        let machine = Arc::new(Self {
            generation: Mutex::new(0),
            seat: seat::ActiveSeat::open()?,
            current: Mutex::default(),
            prepared: Mutex::default(),
            bound: Mutex::default(),
            next_stream: AtomicU64::default(),
            no_session: AtomicBool::default(),
        });
        let notifications = Arc::downgrade(&machine);
        std::thread::spawn(move || {
            while notifications.strong_count() != 0 {
                if let Ok(messages) = seat::changes() {
                    let advance = |delta| {
                        let Some(machine) = notifications.upgrade() else { return false };
                        let mut generation = machine.generation.lock().unwrap();
                        *generation += delta;
                        machine.no_session.store(false, Ordering::Release);
                        if let Some(worker) = machine.current() {
                            worker.failed.store(true, Ordering::Release);
                            if let Some(viewer) = worker.viewer.lock().unwrap().as_ref() {
                                viewer.close(0u32.into(), b"session changed");
                            }
                        }
                        let _ = std::fs::remove_file("/run/sunna-host/receipt");
                        let _ = std::fs::remove_file("/run/sunna-host/shares-greeter");
                        true
                    };
                    advance(1);
                    for message in messages {
                        match message.ok().and_then(|m| seat::selection_changed(&m).ok()) {
                            Some(false) => continue,
                            Some(true) if advance(2) => (),
                            _ => break,
                        }
                    }
                    advance(1);
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        });
        let observer = Arc::clone(&machine);
        let (status, rx) = watch::channel(Err(NO_DESKTOP));
        std::thread::spawn(move || {
            let mut bus = None;
            let mut running: Option<Running> = None;
            let mut retry = Duration::from_millis(250);
            let mut last_session = None;
            let mut unavailable = NO_DESKTOP;
            let mut next_try = std::time::Instant::now();
            let mut last_greeter = None;
            let mut last_receipt = None;
            while !status.is_closed() {
                if bus.is_none() {
                    bus = zbus::blocking::Connection::system().ok();
                }
                let generation = *observer.generation.lock().unwrap();
                let observation = bus.as_ref().filter(|_| generation % 2 == 1)
                    .map(|b| seat::active(b, &observer.seat));
                let empty = matches!(&observation, Some(Ok(None)));
                let selected = match observation {
                    Some(Ok(selected)) => selected,
                    _ => {
                        bus = None;
                        None
                    }
                };
                let no_desktop = empty || selected.as_ref().is_some_and(|s| !s.graphical || s.greeter && s.wayland);
                let dead = running.as_mut().is_some_and(|r| {
                    r.worker.generation != generation || r.worker.failed.load(Ordering::Relaxed)
                        || !matches!(r.child.0.try_wait(), Ok(None))
                });
                if dead || last_session != selected || !empty && selected.is_none() {
                    let _ = std::fs::remove_file("/run/sunna-host/receipt");
                    last_receipt = None;
                }
                if dead
                    || running
                        .as_ref()
                        .is_some_and(|r| Some(&r.session) != selected.as_ref())
                {
                    *observer.current.lock().unwrap() = None;
                    *observer.prepared.lock().unwrap() = None;
                    drop(running.take());
                }
                if last_session != selected {
                    retry = Duration::from_millis(250);
                    next_try = std::time::Instant::now();
                    tracing::info!(
                        session = selected.as_ref().map(|s| s.id.as_str()),
                        "active seat changed"
                    );
                    last_session = selected.clone();
                    unavailable = NO_DESKTOP;
                }
                let state = match selected.as_ref().map(|s| (s, unavailable_session(s))) {
                    None => Err(NO_DESKTOP),
                    Some((_, Some(reason))) => Err(reason),
                    Some((s, None)) => {
                        if running.is_none() && std::time::Instant::now() >= next_try {
                            let attempt = launch(&exe, s, generation);
                            unavailable = match &attempt {
                                Ok(Err(reason)) => reason,
                                _ => NO_DESKTOP,
                            };
                            if let Ok(Ok(new)) = attempt {
                                // Seat ownership can change during startup.
                                if bus
                                    .as_ref()
                                    .and_then(|b| seat::active(b, &observer.seat).ok())
                                    .flatten()
                                    .as_ref()
                                    == Some(s) && *observer.generation.lock().unwrap() == generation
                                {
                                    *observer.current.lock().unwrap() = Some(new.worker.clone());
                                    tracing::info!(session = %s.id, "session worker produced its first frame");
                                    running = Some(new);
                                    retry = Duration::from_millis(250);
                                }
                            }
                            next_try = std::time::Instant::now() + retry;
                            retry = (retry * 2).min(Duration::from_secs(5));
                        }
                        running.as_ref().map(|r| r.size).ok_or(unavailable)
                    }
                };
                let current_generation = observer.generation.lock().unwrap();
                let expected_owner = selected.as_ref().map(|s| (s.id.clone(), s.uid));
                let current = *current_generation == generation && generation % 2 == 1
                    && observer.seat.selected().is_ok_and(|actual| actual == expected_owner);
                let state = if current { state } else { Err(NO_DESKTOP) };
                observer
                    .no_session
                    .store(current && (no_desktop || unavailable == PERMISSION), Ordering::Release);
                {
                    let shareable = selected.as_ref().is_some_and(|s| s.greeter) && state.is_ok();
                    if last_greeter != Some((generation, shareable)) {
                        if publish_status("shares-greeter", if shareable { "1" } else { "0" })
                            .is_ok()
                        {
                            last_greeter = Some((generation, shareable));
                        }
                    }
                }
                let receipt = if state.is_ok() {
                    Some("frame")
                } else if current && empty {
                    Some("empty")
                } else {
                    None
                };
                if Some((generation, receipt)) != last_receipt {
                    let path = Path::new("/run/sunna-host/receipt");
                    if let (Some(kind), Ok(invocation)) = (receipt, std::env::var("INVOCATION_ID"))
                    {
                        if publish_status(
                            "receipt",
                            &format!("{invocation} {} {kind}\n", std::process::id()),
                        )
                        .is_ok()
                        {
                            last_receipt = Some((generation, receipt));
                        }
                    } else {
                        let _ = std::fs::remove_file(path);
                        last_receipt = None;
                    }
                }
                drop(current_generation);
                status.send_if_modified(|old| {
                    let changed = *old != state;
                    *old = state;
                    changed
                });
                std::thread::sleep(Duration::from_millis(250));
            }
        });
        Ok((machine, rx))
    }

    pub fn source(self: &Arc<Self>, stream: &StreamConfig) -> anyhow::Result<Box<dyn FrameSource>> {
        let len = wire::dimensions(stream.width, stream.height)?;
        Ok(Box::new(Source {
            machine: self.clone(),
            cursor: None,
            worker: self
                .bound
                .lock()
                .unwrap()
                .upgrade()
                .context("session changed")?,
            size: (stream.width, stream.height),
            fps: stream.fps.clamp(1, 120),
            len,
            id: self.next_stream.fetch_add(1, Ordering::Relaxed),
            frame: 0,
        }))
    }

    fn current(&self) -> Option<Arc<Worker>> {
        self.current.lock().unwrap().clone()
    }
    fn require_current(&self, worker: &Arc<Worker>) -> anyhow::Result<()> {
        if !self.is_current(worker) {
            if let Some(connection) = worker.viewer.lock().unwrap().as_ref() {
                connection.close(0u32.into(), b"session changed");
            }
            anyhow::bail!("session changed");
        }
        Ok(())
    }
    fn is_current(&self, candidate: &Arc<Worker>) -> bool {
        candidate.generation == *self.generation.lock().unwrap()
            && !candidate.failed.load(Ordering::Acquire)
            && self.current().is_some_and(|w| Arc::ptr_eq(&w, candidate))
            && self.seat.selected().ok().flatten().as_ref() == Some(&candidate.owner)
    }
}

struct Source {
    cursor: Option<sunna_proto::messages::CursorShape>,
    worker: Arc<Worker>,
    machine: Arc<Machine>,
    size: (u32, u32),
    len: usize,
    fps: u32,
    id: u64,
    frame: u64,
}

impl Source {
    fn read(
        &self,
        worker: &Worker,
        bytes: &mut [u8],
    ) -> anyhow::Result<Option<sunna_proto::messages::CursorShape>> {
        let mut pipe = worker.pipe.lock().unwrap();
        if pipe.stream != Some(self.id) {
            pipe.io.write_all(&[wire::BEGIN])?;
            for n in [self.size.0, self.size.1, self.fps] {
                pipe.io.write_all(&n.to_be_bytes())?;
            }
            let mut ready = [0];
            pipe.io.read_exact(&mut ready)?;
            ensure!(ready == [1]);
            pipe.stream = Some(self.id);
        }
        pipe.io.write_all(&[wire::FRAME_WITH_CURSOR])?;
        let n = wire::number(&mut pipe.io)? as usize;
        ensure!(n <= wire::MAX_CURSOR, "oversized worker cursor");
        let cursor = if n == 0 {
            None
        } else {
            let mut bytes = vec![0; n];
            media::bounded_read(&mut pipe.io, &mut bytes)?;
            let ControlMessage::Cursor(shape) = sunna_proto::messages::decode(&bytes)? else {
                anyhow::bail!("invalid cursor response");
            };
            // The greeter can withdraw its cursor while switching. Invalid
            // optional metadata must never interrupt a valid video frame.
            (shape.width <= 256
                && shape.height <= 256
                && shape.rgba.len() == usize::from(shape.width) * usize::from(shape.height) * 4
                && shape.hot_x < shape.width
                && shape.hot_y < shape.height)
                .then_some(shape)
        };
        // A slow writer cannot hold the slot indefinitely by sending one
        // byte just before each read timeout.
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let mut rest = bytes;
        while !rest.is_empty() {
            ensure!(std::time::Instant::now() < deadline, "worker frame timeout");
            let n = pipe.io.read(rest)?;
            ensure!(n > 0, "worker closed the frame channel");
            rest = &mut rest[n..];
        }
        Ok(cursor)
    }
}
impl FrameSource for Source {
    fn take_cursor(&mut self) -> Option<sunna_proto::messages::CursorShape> {
        self.cursor.take()
    }
    fn next_frame(&mut self) -> anyhow::Result<VideoFrame> {
        let mut bytes = vec![0; self.len];
        let worker = &self.worker;
        self.machine.require_current(worker)?;
        self.cursor = match self.read(worker, &mut bytes) {
            Ok(cursor) => cursor,
            Err(error) => {
                worker.failed.store(true, Ordering::Release);
                if let Some(connection) = worker.viewer.lock().unwrap().as_ref() {
                    connection.close(0u32.into(), b"session changed");
                }
                return Err(error);
            }
        };
        self.machine.require_current(worker)?;
        *self.machine.prepared.lock().unwrap() = Some((Arc::downgrade(worker), self.id));
        self.frame += 1;
        Ok(VideoFrame {
            frame_id: self.frame,
            width: self.width(),
            height: self.height(),
            format: PixelFormat::Bgra8,
            data: FrameData::Cpu(bytes.into()),
            capture_ts_us: sunna_proto::now_us(),
        })
    }
    fn width(&self) -> u32 {
        self.size.0
    }
    fn height(&self) -> u32 {
        self.size.1
    }
    fn fps(&self) -> u32 {
        self.fps
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        {
            let worker = &self.worker;
            let mut pipe = worker.pipe.lock().unwrap();
            if pipe.stream == Some(self.id) {
                pipe.stream = None;
                let _ = pipe.io.write_all(&[wire::END]);
                if !worker.greeter {
                    media::end(&worker, self.id);
                }
                *self.machine.prepared.lock().unwrap() = None;
            }
        }
    }
}

struct Input {
    bound: Weak<Worker>,
    machine: Arc<Machine>,
    sender: Option<std::sync::mpsc::SyncSender<(Arc<Worker>, u64, InputEvent)>>,
    stopped: Arc<AtomicBool>,
}

impl Input {
    fn new(machine: Arc<Machine>) -> Self {
        let (sender, rx) = std::sync::mpsc::sync_channel::<(Arc<Worker>, u64, InputEvent)>(256);
        let stopped = Arc::new(AtomicBool::new(false));
        let done = stopped.clone();
        let observer = machine.clone();
        // A slow or hostile session must not block QUIC's control runtime.
        std::thread::spawn(move || {
            let mut delivered = None;
            for (worker, stream, event) in rx {
                if done.load(Ordering::Acquire) {
                    break;
                }
                let mut pipe = worker.pipe.lock().unwrap();
                if pipe.stream != Some(stream)
                    || !observer.is_current(&worker)
                    || done.load(Ordering::Acquire)
                {
                    continue;
                }
                let Ok(bytes) = sunna_proto::messages::encode(&ControlMessage::Input(event)) else {
                    continue;
                };
                if bytes.len() > 4096 {
                    continue;
                }
                let mut message = vec![wire::INPUT];
                message.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
                message.extend_from_slice(&bytes);
                if pipe.io.write_all(&message).is_err() {
                    worker.failed.store(true, Ordering::Release);
                } else {
                    delivered = Some((worker.clone(), stream));
                }
            }
            if let Some((worker, stream)) = delivered {
                let mut pipe = worker.pipe.lock().unwrap();
                if pipe.stream == Some(stream) {
                    let _ = pipe.io.write_all(&[wire::RELEASE]);
                }
            }
        });
        let bound = machine.bound.lock().unwrap().clone();
        Self {
            bound,
            machine,
            sender: Some(sender),
            stopped,
        }
    }
}

impl InputInjector for Input {
    fn inject(&mut self, event: &InputEvent) -> anyhow::Result<()> {
        if let Some((target, stream)) = &*self.machine.prepared.lock().unwrap() {
            if !target.ptr_eq(&self.bound) {
                return Ok(());
            }
            if let Some(worker) = target.upgrade() {
                self.sender
                    .as_ref()
                    .context("input has stopped")?
                    .try_send((worker, *stream, event.clone()))
                    .map_err(|_| anyhow::anyhow!("the desktop is not accepting input"))?;
            }
        }
        Ok(())
    }
    fn release_all(&mut self) {
        self.stopped.store(true, Ordering::Release);
        self.sender.take();
    }
}

pub async fn serve(mut args: super::Args) -> anyhow::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let dir = std::fs::symlink_metadata("/etc/sunna")?;
    ensure!(
        dir.is_dir() && dir.uid() == 0 && dir.mode() & 0o077 == 0,
        "unsafe system settings directory"
    );
    let mut key = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open("/etc/sunna/host.key")?;
    let meta = key.metadata()?;
    ensure!(
        meta.is_file() && meta.uid() == 0 && meta.mode() & 0o077 == 0 && meta.len() <= 4096,
        "unsafe system key file"
    );
    args.token.clear();
    key.read_to_string(&mut args.token)?;
    ensure!(
        matches!(args.token.len(), 32 | 64) && args.token.bytes().all(|b| b.is_ascii_hexdigit()),
        "the system host requires a configured machine key"
    );
    let runtime = Path::new("/run/sunna-host");
    let owner = runtime.symlink_metadata()?;
    ensure!(
        owner.is_dir() && owner.uid() == 0 && owner.mode() & 0o022 == 0,
        "unsafe runtime directory"
    );
    for name in ["listening", "receipt", "shares-greeter"] {
        match std::fs::remove_file(runtime.join(name)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    let server = sunna_transport::Server::with_identity(args.listen, sunna_transport::HostIdentity::load_or_create(&super::identity_path(true)?)?)?;
    let bitrate = args.bitrate_kbps.unwrap_or(40_000).saturating_mul(1000);
    drop(sunna_codec::make_encoder(
        &args.codec,
        640,
        360,
        args.fps,
        bitrate,
    )?);
    publish_status("listening", &server.local_addr()?.to_string())?;
    let (machine, desktop) = Machine::start()?;
    let verifier = machine.clone();
    let availability = desktop.clone();
    sunna_update::host::start_after(move |updater| {
        if std::env::var_os("SUNNA_READY_FILE").is_none() {
            return Ok(());
        }
        for sequence in 0.. {
            if let Some(worker) = verifier.current() {
                if availability.borrow().is_ok() && verifier.is_current(&worker) {
                    // launch waits for a real backend frame before publishing this worker.
                    return Ok(());
                }
            }
            if verifier.no_session.load(Ordering::Acquire) {
                updater.waiting_for_desktop(sequence)?;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        unreachable!()
    });
    let config = sunna_host::HostConfig {
            require_proof: std::env::var("SUNNA_REQUIRE_PROOF").as_deref() != Ok("0"),
        clipboard: !args.no_clipboard,
        audio: !args.no_audio,
        name: args.name,
        width: 640,
        height: 360,
        token: args.token,
        fps: args.fps,
        codec: args.codec,
        max_bitrate_bps: bitrate,
        fast_lane: false,
        simulate_loss: 0.0,
        about: super::about::detect(),
        display_follows_viewer: false,
    };
    let capture = machine.clone();
    let devices = machine.clone();
    sunna_host::run_host_with_desktop(
        server,
        config,
        Box::new(move |s| capture.source(s)),
        Box::new(|s| sunna_codec::make_encoder(&s.codec, s.width, s.height, s.fps, s.bitrate_bps)),
        Box::new(move || Box::new(Input::new(machine.clone()))),
        desktop,
        Some(devices),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sunna_clipboard::ClipboardContent;
    use sunna_host::SessionDevices;

    #[test]
    fn concurrent_sddm_greeters_still_share_the_active_x_server() {
        let mut s = seat::Session {
            id: "a".into(),
            uid: 101,
            vt: 2,
            graphical: true,
            ambiguous: true,
            owner: ":1.7".into(),
            started: 8,
            display: ":0".into(),
            greeter: true,
            wayland: false,
        };
        assert_eq!(unavailable_session(&s), None);
        s.wayland = true;
        assert_eq!(unavailable_session(&s), Some(NO_GREETER));
        s.greeter = false;
        assert!(unavailable_session(&s).is_some());
    }

    #[test]
    fn return_to_a_does_not_revive_its_worker() {
        let (old, _io, _media) = worker("a");
        let machine = Machine {
            seat: seat::ActiveSeat::fixture_a(),
            current: Mutex::new(Some(old.clone())),
            prepared: Mutex::default(),
            bound: Mutex::default(),
            next_stream: AtomicU64::default(),
            no_session: AtomicBool::default(),
            generation: Mutex::new(1),
        };
        assert!(machine.is_current(&old));
        *machine.generation.lock().unwrap() += 4; // A → B → A
        assert!(!machine.is_current(&old));
    }

    fn worker(session: &str) -> (Arc<Worker>, UnixStream, UnixStream) {
        let (io, display) = UnixStream::pair().unwrap();
        let (media, peer) = UnixStream::pair().unwrap();
        let w = Worker {
            generation: 1,
            media: Mutex::new(Some(media)),
            greeter: false,
            native: (32, 32),
            owner: (session.to_owned(), 1000),
            failed: AtomicBool::default(),
            shutdown: io.try_clone().unwrap(),
            pipe: Mutex::new(Pipe { io, stream: None }),
            viewer: Mutex::default(),
        };
        (Arc::new(w), display, peer)
    }

    #[test]
    fn absent_cursor_shape_does_not_interrupt_the_picture() {
        let (worker, mut display, _media) = worker("greeter");
        let machine = Arc::new(Machine {
            generation: Mutex::new(1),
            seat: seat::ActiveSeat::open().unwrap(),
            current: Mutex::new(Some(worker.clone())),
            prepared: Mutex::default(),
            bound: Mutex::new(Arc::downgrade(&worker)),
            next_stream: AtomicU64::default(),
            no_session: AtomicBool::default(),
        });
        let source = Source {
            cursor: None,
            worker: worker.clone(),
            machine,
            size: (2, 2),
            len: 16,
            fps: 1,
            id: 17,
            frame: 0,
        };
        worker.pipe.lock().unwrap().stream = Some(17);
        let cursor = sunna_proto::messages::CursorShape {
            id: 1,
            width: 0,
            height: 0,
            hot_x: 0,
            hot_y: 0,
            screen_width: 2,
            rgba: Vec::new(),
        };
        let shape = sunna_proto::messages::encode(&ControlMessage::Cursor(cursor)).unwrap();
        display
            .write_all(&(shape.len() as u32).to_be_bytes())
            .unwrap();
        display.write_all(&shape).unwrap();
        display.write_all(&[153; 16]).unwrap();
        let mut frame = [0; 16];
        assert!(source.read(&worker, &mut frame).unwrap().is_none());
        assert_eq!(frame, [153; 16]);
    }

    #[test]
    fn old_viewer_cannot_use_the_replacement_workers_devices() {
        let (old, mut old_display, mut old_media) = worker("a");
        let (new, mut display, mut media) = worker("b");
        let machine = Arc::new(Machine {
            generation: Mutex::new(1),
            seat: seat::ActiveSeat::fixture_a(),
            current: Mutex::new(Some(old.clone())),
            prepared: Mutex::new(Some((Arc::downgrade(&old), 1))),
            bound: Mutex::new(Arc::downgrade(&old)),
            next_stream: AtomicU64::default(),
            no_session: AtomicBool::default(),
        });
        let mut input = Input::new(machine.clone());
        let mut audio = machine.clone().audio();
        let mut copies = machine.clone().clipboard();
        let mut source = machine
            .source(&StreamConfig {
                width: 32,
                height: 32,
                fps: 30,
                bitrate_bps: 100000,
                codec: "raw".into(),
                fast_lane: false,
            })
            .unwrap();

        // Real listener device adapters and worker socket framing, with
        // deterministic pixels, PCM, clipboard and input for session A.
        old.pipe.lock().unwrap().stream = Some(0);
        old_display.write_all(&0u32.to_be_bytes()).unwrap();
        old_display.write_all(&[0x77; 4096]).unwrap();
        let frame = source.next_frame().unwrap();
        assert!(frame.data.to_cpu().unwrap().iter().all(|&x| x == 0x77));
        let mut op = [0];
        old_display.read_exact(&mut op).unwrap();
        assert_eq!(op, [wire::FRAME_WITH_CURSOR]);
        old_media.write_all(&[1]).unwrap();
        old_media
            .write_all(&[177i16.to_le_bytes(); sunna_audio::FRAME_LEN].concat())
            .unwrap();
        let mut pcm = [0; sunna_audio::FRAME_LEN];
        audio.read(&mut pcm).unwrap();
        assert!(pcm.iter().all(|&sample| sample == 177));
        old_media.write_all(&[1]).unwrap();
        old_media.write_all(&13u32.to_be_bytes()).unwrap();
        old_media.write_all(b"old clipboard").unwrap();
        assert_eq!(copies.change_count(), 1);
        let click = InputEvent::MouseMoveAbs { x: 0.7, y: 0.3 };
        input.inject(&click).unwrap();
        old_display
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        old_display.read_exact(&mut op).unwrap();
        assert_eq!(op, [wire::INPUT]);
        let mut payload = vec![0; wire::number(&mut old_display).unwrap() as usize];
        old_display.read_exact(&mut payload).unwrap();
        assert_eq!(
            sunna_proto::messages::decode(&payload).unwrap(),
            ControlMessage::Input(click.clone())
        );

        *machine.current.lock().unwrap() = Some(new.clone());
        *machine.bound.lock().unwrap() = Arc::downgrade(&new);
        *machine.prepared.lock().unwrap() = Some((Arc::downgrade(&new), 2));
        assert!(source.next_frame().is_err());
        audio.read(&mut pcm).unwrap();
        assert!(pcm.iter().all(|&sample| sample == 0));
        assert!(copies.read().is_none());
        copies.change_count();
        copies.write(&ClipboardContent::Text("old clipboard".into()));
        assert!(copies.read().is_none());
        input.inject(&click).unwrap();
        input.release_all();
        for socket in [&mut display, &mut media] {
            socket.set_nonblocking(true).unwrap();
            assert_eq!(
                socket.read(&mut [0]).unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
        }
    }
}
