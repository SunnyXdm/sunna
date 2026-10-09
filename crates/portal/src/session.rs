//! Portal requests share one connection, including their signal subscriptions.
use anyhow::{bail, ensure, Context};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::fd::OwnedFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex, Weak,
};
use std::time::{Duration, Instant};
use zbus::blocking::{connection::Builder, Connection, MessageIterator, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const DEST: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const RD: &str = "org.freedesktop.portal.RemoteDesktop";
const SC: &str = "org.freedesktop.portal.ScreenCast";
pub type Results = HashMap<String, OwnedValue>;
type Options<'a> = HashMap<&'a str, Value<'a>>;
const APPROVE: &str = "Approve sharing on the computer: run sunna-host setup --desktop there";
pub const STOPPED: &str = "Sharing was stopped on the computer";

#[derive(Debug, PartialEq)]
pub struct Stream {
    pub node: u32,
    pub size: (u32, u32),
    pub position: (i32, i32),
}
pub fn parse_start(mut results: Results) -> anyhow::Result<(Stream, u32, Option<String>)> {
    let streams = results
        .remove("streams")
        .context("portal returned no streams")?;
    let streams: Vec<(u32, Results)> = streams.try_into().context("invalid portal streams")?;
    let (node, mut props) = streams.into_iter().next().context("no screen selected")?;
    let size: (i32, i32) = props
        .remove("size")
        .context("portal stream has no size")?
        .try_into()?;
    ensure!(size.0 > 0 && size.1 > 0, "invalid portal stream size");
    let position = props
        .remove("position")
        .map(TryInto::try_into)
        .transpose()?
        .unwrap_or((0, 0));
    let devices = results
        .remove("devices")
        .map(u32::try_from)
        .transpose()?
        .unwrap_or(0);
    let token = results
        .remove("restore_token")
        .map(String::try_from)
        .transpose()?;
    Ok((
        Stream {
            node,
            size: (size.0 as u32, size.1 as u32),
            position,
        },
        devices,
        token,
    ))
}
fn session_handle(value: &OwnedValue) -> anyhow::Result<&str> {
    // Portals historically returned a string here, rather than an object path.
    if let Ok(path) = <&str>::try_from(value) {
        return Ok(path);
    }
    let path = <&zbus::zvariant::ObjectPath<'_>>::try_from(value)?;
    Ok(path.as_str())
}

fn token_path() -> anyhow::Result<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|p| !p.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?).join(".config"),
    };
    Ok(base.join("sunna/portal-token"))
}
fn save_token_at(path: &std::path::Path, token: &str) -> anyhow::Result<()> {
    let dir = path.parent().unwrap();
    fs::create_dir_all(dir)?;
    let temp = path.with_extension(format!("new-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)?;
    let result = (|| {
        file.write_all(token.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        fs::File::open(dir)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub struct Session {
    conn: Connection,
    path: OwnedObjectPath,
    pub stream: Stream,
    pub devices: u32,
    pub metadata_cursor: bool,
    pub remembered: bool,
    pub closed: Arc<AtomicBool>,
    watcher: Option<std::thread::JoinHandle<()>>,
}
impl Session {
    pub fn start(timeout: Duration, restore: bool) -> anyhow::Result<Self> {
        let deadline = Instant::now() + timeout;
        let conn = Builder::session()?
            .method_timeout(Duration::from_secs(10))
            .build()
            .context("connecting to the desktop portal")?;
        let sender = conn
            .unique_name()
            .context("D-Bus connection has no unique name")?
            .as_str()[1..]
            .replace('.', "_");
        let path: OwnedObjectPath = format!("{PATH}/session/{sender}/sunnasession").try_into()?;
        let mut session = Self {
            conn,
            path,
            stream: Stream {
                node: 0,
                size: (0, 0),
                position: (0, 0),
            },
            devices: 0,
            metadata_cursor: false,
            remembered: false,
            closed: Arc::new(AtomicBool::new(false)),
            watcher: None,
        };
        // Install the match before CreateSession: even immediate replies must be queued.
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(DEST)?
            .path_namespace(PATH)?
            .build();
        let mut signals = MessageIterator::for_match_rule(rule, &session.conn, Some(64))?;
        let closed = session.closed.clone();
        let watched_path = session.path.clone();
        let (tx, rx) = mpsc::channel();
        session.watcher = Some(
            std::thread::Builder::new()
                .name("sunna-portal".into())
                .spawn(move || {
                    for message in &mut signals {
                        let Ok(message) = message else { break };
                        let header = message.header();
                        if header
                            .path()
                            .is_some_and(|p| p.as_str() == watched_path.as_str())
                            && header
                                .interface()
                                .is_some_and(|v| v.as_str() == "org.freedesktop.portal.Session")
                            && header.member().is_some_and(|v| v.as_str() == "Closed")
                        {
                            closed.store(true, Ordering::Release);
                        } else if header
                            .interface()
                            .is_some_and(|v| v.as_str() == "org.freedesktop.portal.Request")
                            && header.member().is_some_and(|v| v.as_str() == "Response")
                        {
                            let _ = tx.send(message);
                        }
                    }
                    closed.store(true, Ordering::Release);
                })?,
        );
        let registry = Proxy::new(
            &session.conn,
            DEST,
            PATH,
            "org.freedesktop.host.portal.Registry",
        )?;
        let _: zbus::Result<()> = registry.call("Register", &("dev.sunna.Host", Options::new()));
        let rd = Proxy::new(&session.conn, DEST, PATH, RD)?;
        let sc = Proxy::new(&session.conn, DEST, PATH, SC)?;
        let mut count = 0;
        let mut request = |options: Options<'_>,
                           call: &dyn Fn(Options<'_>) -> zbus::Result<OwnedObjectPath>|
         -> anyhow::Result<Results> {
            count += 1;
            let token = format!("sunna{count}");
            let expected = format!("{PATH}/request/{sender}/{token}");
            let mut options = options;
            options.insert("handle_token", Value::from(token.as_str()));
            let actual = call(options)?;
            ensure!(
                actual.as_str() == expected,
                "portal returned an unexpected request path"
            );
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                ensure!(!session.closed.load(Ordering::Acquire), STOPPED);
                let message = match rx.recv_timeout(left.min(Duration::from_millis(100))) {
                    Ok(m) => m,
                    Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() < deadline => continue,
                    Err(_) => {
                        let p = Proxy::new(
                            &session.conn,
                            DEST,
                            actual.as_str(),
                            "org.freedesktop.portal.Request",
                        )?;
                        let _ = p.call_noreply("Close", &());
                        bail!("{APPROVE} (no portal approval before the deadline)");
                    }
                };
                if message
                    .header()
                    .path()
                    .is_none_or(|p| p.as_str() != expected)
                {
                    continue;
                }
                let (code, results): (u32, Results) = message.body().deserialize()?;
                ensure!(
                    code == 0,
                    "{APPROVE} (portal request was cancelled or denied)"
                );
                return Ok(results);
            }
        };
        let created = request(
            HashMap::from([("session_handle_token", Value::from("sunnasession"))]),
            &|o| rd.call("CreateSession", &(o,)),
        )?;
        let handle = session_handle(
            created
                .get("session_handle")
                .context("portal returned no session handle")?,
        )?;
        ensure!(
            handle == session.path.as_str(),
            "portal returned an unexpected session path"
        );
        let saved = if restore {
            fs::read_to_string(token_path()?).ok()
        } else {
            None
        };
        let mut devices = HashMap::from([
            ("types", Value::from(3u32)),
            ("persist_mode", Value::from(2u32)),
        ]);
        if let Some(token) = saved.as_deref() {
            devices.insert("restore_token", Value::from(token.trim()));
        }
        if let Err(error) = request(devices, &|o| rd.call("SelectDevices", &(&session.path, o))) {
            if saved.is_none()
                || session.closed.load(Ordering::Acquire)
                || Instant::now() >= deadline
            {
                return Err(error);
            }
            // Some older portals reject an obsolete token instead of ignoring it.
            drop(sc);
            drop(rd);
            drop(registry);
            drop(session);
            return Self::start(deadline.saturating_duration_since(Instant::now()), false);
        }
        let modes: u32 = sc.get_property("AvailableCursorModes").unwrap_or(2);
        let metadata = modes & 4 != 0;
        request(
            HashMap::from([
                ("types", Value::from(1u32)),
                ("multiple", Value::from(false)),
                (
                    "cursor_mode",
                    Value::from(if metadata { 4u32 } else { 2u32 }),
                ),
            ]),
            &|o| sc.call("SelectSources", &(&session.path, o)),
        )?;
        let started = request(Options::new(), &|o| {
            rd.call("Start", &(&session.path, "", o))
        })?;
        let (stream, devices, token) = parse_start(started)?;
        session.remembered = token.is_some();
        if let Some(token) = token {
            save_token_at(&token_path()?, &token).context("saving portal permission")?;
        } else {
            // A consumed token without a replacement cannot authorize the next viewer.
            match fs::remove_file(token_path()?) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e).context("removing expired portal permission"),
            }
        }
        session.stream = stream;
        session.devices = devices;
        session.metadata_cursor = metadata;
        Ok(session)
    }
    pub fn open_pipewire(&self) -> anyhow::Result<OwnedFd> {
        let fd: zbus::zvariant::OwnedFd = Proxy::new(&self.conn, DEST, PATH, SC)?
            .call("OpenPipeWireRemote", &(&self.path, Options::new()))?;
        Ok(fd.into())
    }
    fn remote(&self) -> anyhow::Result<Proxy<'_>> {
        ensure!(!self.closed.load(Ordering::Acquire), STOPPED);
        Ok(Proxy::new(&self.conn, DEST, PATH, RD)?)
    }
    pub fn motion(&self, x: f64, y: f64) -> anyhow::Result<()> {
        if self.devices & 2 != 0 {
            self.remote()?.call::<_, _, ()>(
                "NotifyPointerMotionAbsolute",
                &(&self.path, Options::new(), self.stream.node, x, y),
            )?;
        }
        Ok(())
    }
    pub fn button(&self, code: i32, pressed: bool) -> anyhow::Result<()> {
        if self.devices & 2 != 0 {
            self.remote()?.call::<_, _, ()>(
                "NotifyPointerButton",
                &(&self.path, Options::new(), code, u32::from(pressed)),
            )?;
        }
        Ok(())
    }
    pub fn key(&self, code: i32, pressed: bool) -> anyhow::Result<()> {
        if self.devices & 1 != 0 {
            self.remote()?.call::<_, _, ()>(
                "NotifyKeyboardKeycode",
                &(&self.path, Options::new(), code, u32::from(pressed)),
            )?;
        }
        Ok(())
    }
    pub fn axis(&self, dx: f64, dy: f64, finish: bool) -> anyhow::Result<()> {
        if self.devices & 2 != 0 {
            self.remote()?.call::<_, _, ()>(
                "NotifyPointerAxis",
                &(
                    &self.path,
                    HashMap::from([("finish", Value::from(finish))]),
                    dx,
                    dy,
                ),
            )?;
        }
        Ok(())
    }
    pub fn discrete(&self, axis: u32, steps: i32) -> anyhow::Result<()> {
        if self.devices & 2 != 0 {
            self.remote()?.call::<_, _, ()>(
                "NotifyPointerAxisDiscrete",
                &(&self.path, Options::new(), axis, steps),
            )?;
        }
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if let Ok(p) = Proxy::new(
            &self.conn,
            DEST,
            self.path.as_str(),
            "org.freedesktop.portal.Session",
        ) {
            let _ = p.call_noreply("Close", &());
        }
        let _ = self.conn.clone().close();
        if let Some(thread) = self.watcher.take() {
            let _ = thread.join();
        }
    }
}

// The host admits one viewer. Neither the registry nor the signal thread owns it.
static SESSION: Mutex<Weak<Session>> = Mutex::new(Weak::new());
fn shared<T>(
    registry: &Mutex<Weak<T>>,
    start: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<Arc<T>> {
    let mut slot = registry.lock().unwrap();
    if let Some(session) = slot.upgrade() {
        return Ok(session);
    }
    let session = Arc::new(start()?);
    *slot = Arc::downgrade(&session);
    Ok(session)
}
pub fn acquire() -> anyhow::Result<Arc<Session>> {
    shared(&SESSION, || Session::start(Duration::from_secs(60), true))
}

/// Mutter's current physical monitor mode, without opening a portal dialog.
pub fn display_size() -> anyhow::Result<(u32, u32)> {
    type Spec = (String, String, String, String);
    type Mode = (String, i32, i32, f64, f64, Vec<f64>, Results);
    type Monitor = (Spec, Vec<Mode>, Results);
    type Logical = (i32, i32, f64, u32, bool, Vec<Spec>, Results);
    let conn = Builder::session()?
        .method_timeout(Duration::from_secs(2))
        .build()?;
    let proxy = Proxy::new(
        &conn,
        "org.gnome.Mutter.DisplayConfig",
        "/org/gnome/Mutter/DisplayConfig",
        "org.gnome.Mutter.DisplayConfig",
    )?;
    let (_, monitors, logical, _): (u32, Vec<Monitor>, Vec<Logical>, Results) =
        proxy.call("GetCurrentState", &())?;
    let primary = logical.iter().find(|m| m.4).or(logical.first());
    for (spec, modes, _) in monitors {
        if primary.is_some_and(|m| !m.5.contains(&spec)) {
            continue;
        }
        for (_, w, h, _, _, _, props) in modes {
            if props.get("is-current").and_then(|v| bool::try_from(v).ok()) == Some(true)
                && w > 0
                && h > 0
            {
                let rotated = primary.is_some_and(|m| matches!(m.3, 1 | 3 | 5 | 7));
                return Ok(if rotated {
                    (h as u32, w as u32)
                } else {
                    (w as u32, h as u32)
                });
            }
        }
    }
    bail!("Mutter returned no current monitor")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owned(v: Value<'_>) -> OwnedValue {
        v.try_into().unwrap()
    }
    #[test]
    fn session_handles_accept_both_portal_representations() {
        let string = owned(Value::from("/org/freedesktop/portal/desktop/session/test"));
        let path = owned(Value::from(
            zbus::zvariant::ObjectPath::try_from("/org/freedesktop/portal/desktop/session/test")
                .unwrap(),
        ));
        assert_eq!(
            session_handle(&string).unwrap(),
            session_handle(&path).unwrap()
        );
        assert!(session_handle(&OwnedValue::from(1u32)).is_err());
    }
    #[test]
    fn start_results_keep_logical_size_and_position() {
        let props = HashMap::from([
            ("size".to_owned(), owned(Value::from((1920i32, 1080i32)))),
            ("position".into(), owned(Value::from((-1920i32, 0i32)))),
        ]);
        let results: Results = HashMap::from([
            ("streams".into(), owned(Value::from(vec![(42u32, props)]))),
            ("devices".into(), OwnedValue::from(3u32)),
            ("restore_token".into(), owned(Value::from("secret"))),
        ]);
        let message =
            zbus::Message::signal("/request", "org.freedesktop.portal.Request", "Response")
                .unwrap()
                .build(&(0u32, results))
                .unwrap();
        let (code, results): (u32, Results) = message.body().deserialize().unwrap();
        assert_eq!(code, 0);
        let (stream, devices, token) = parse_start(results).unwrap();
        assert_eq!(
            stream,
            Stream {
                node: 42,
                size: (1920, 1080),
                position: (-1920, 0)
            }
        );
        assert_eq!(devices, 3);
        assert_eq!(token.as_deref(), Some("secret"));
        assert!(parse_start(HashMap::new()).is_err());
    }
    #[test]
    fn registry_does_not_extend_lifetime() {
        let registry = Mutex::new(Weak::new());
        let a = shared(&registry, || Ok(7)).unwrap();
        let b = shared(&registry, || panic!("must reuse")).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        drop(a);
        assert!(registry.lock().unwrap().upgrade().is_some());
        drop(b);
        assert!(registry.lock().unwrap().upgrade().is_none());
        assert_eq!(*shared(&registry, || Ok(8)).unwrap(), 8);
    }
    #[test]
    fn token_replacement_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("sunna-token-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("portal-token");
        save_token_at(&path, "first").unwrap();
        save_token_at(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
