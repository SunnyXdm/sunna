//! Only logind on the system bus can select the uid and seat. Environment
//! discovery is done by the worker after setuid.
use anyhow::{ensure, Context};
use std::collections::HashMap;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

fn property<T>(values: &mut HashMap<String, OwnedValue>, key: &str) -> anyhow::Result<T>
where
    T: TryFrom<OwnedValue>,
    T::Error: Into<zbus::Error>,
{
    Ok(T::try_from(values.remove(key).context("missing logind property")?).map_err(Into::into)?)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub uid: u32,
    pub vt: u32,
    pub graphical: bool,
    pub ambiguous: bool,
    pub owner: String,
    pub started: u64,
    pub display: String,
    pub greeter: bool,
    pub wayland: bool,
}

fn snapshot(bus: &Connection) -> anyhow::Result<Option<Session>> {
    let name = "org.freedesktop.login1";
    let dbus = zbus::blocking::fdo::DBusProxy::new(bus)?;
    let owner = dbus.get_name_owner(name.try_into()?)?;
    let name = owner.as_str();
    let manager = Proxy::new(
        bus,
        name,
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )?;
    let seat_path: OwnedObjectPath = match manager.call("GetSeat", &("seat0",)) {
        Err(zbus::Error::MethodError(name, _, _))
            if name.as_str() == "org.freedesktop.login1.NoSuchSeat" =>
        {
            return Ok(None)
        }
        result => result?,
    };
    let seat = Proxy::new(bus, name, &seat_path, "org.freedesktop.login1.Seat")?;
    let (id, path): (String, OwnedObjectPath) = seat.get_property("ActiveSession")?;
    if id.is_empty() {
        return Ok(None);
    }
    ensure!(
        id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric()),
        "unexpected session id"
    );
    // Read one consistent property snapshot without creating a new signal
    // subscription for every field on every observation.
    let result = bus.call_method(
        Some(name),
        &path,
        Some("org.freedesktop.DBus.Properties"),
        "GetAll",
        &("org.freedesktop.login1.Session",),
    )?;
    let mut values: HashMap<String, OwnedValue> = result.body().deserialize()?;
    let (uid, _): (u32, OwnedObjectPath) = property(&mut values, "User")?;
    let (seat_id, _): (String, OwnedObjectPath) = property(&mut values, "Seat")?;
    let active: bool = property(&mut values, "Active")?;
    let remote: bool = property(&mut values, "Remote")?;
    let kind: String = property(&mut values, "Type")?;
    let class: String = property(&mut values, "Class")?;
    ensure!(
        active && seat_id == "seat0",
        "seat changed during observation"
    );
    let graphical = uid != 0
        && !remote
        && matches!(class.as_str(), "greeter" | "user")
        && matches!(kind.as_str(), "x11" | "wayland");
    let display: String = property(&mut values, "Display")?;
    if kind == "x11" && !display.is_empty() {
        let number = display.strip_prefix(':').context("no local X11 display")?;
        ensure!(
            !number.is_empty() && number.len() <= 5 && number.bytes().all(|b| b.is_ascii_digit()),
            "unexpected display"
        );
    }
    let sessions: Vec<(String, u32, String, String, OwnedObjectPath)> =
        manager.call("ListSessions", &())?;
    let mut ambiguous = false;
    for (other, user, _, _, path) in sessions {
        if other != id && user == uid {
            let other = Proxy::new(bus, name, &path, "org.freedesktop.login1.Session")?;
            let kind: String = other.get_property("Type")?;
            ambiguous |= matches!(kind.as_str(), "wayland" | "x11");
        }
    }
    Ok(Some(Session {
        vt: property(&mut values, "VTNr")?,
        started: property(&mut values, "TimestampMonotonic")?,
        owner: name.to_string(),
        graphical,
        ambiguous,
        id,
        uid,
        display,
        greeter: class == "greeter",
        wayland: kind == "wayland",
    }))
}

fn consistent(
    first: Option<Session>,
    second: Option<Session>,
    actual: Option<(String, u32)>,
) -> anyhow::Result<Option<Session>> {
    ensure!(first == second, "seat changed between observations");
    ensure!(
        second.as_ref().map(|s| (&s.id, s.uid)) == actual.as_ref().map(|(id, uid)| (id, *uid)),
        "seat and active session disagree"
    );
    Ok(second)
}

pub fn active(bus: &Connection, seat: &ActiveSeat) -> anyhow::Result<Option<Session>> {
    consistent(snapshot(bus)?, snapshot(bus)?, seat.selected()?)
}

pub fn changes() -> anyhow::Result<zbus::blocking::MessageIterator> {
    subscribe(&Connection::system()?)
}

fn subscribe(bus: &Connection) -> anyhow::Result<zbus::blocking::MessageIterator> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.login1")?
        .path("/org/freedesktop/login1/seat/seat0")?
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .add_arg("org.freedesktop.login1.Seat")?
        .build();
    Ok(zbus::blocking::MessageIterator::for_match_rule(
        rule, bus, None,
    )?)
}

pub fn selection_changed(message: &zbus::Message) -> anyhow::Result<bool> {
    let (_, changed, invalidated): (String, HashMap<String, OwnedValue>, Vec<String>) =
        message.body().deserialize()?;
    Ok(changed.contains_key("ActiveSession")
        || invalidated.iter().any(|key| key == "ActiveSession"))
}

/// Ask libsystemd for the active seat immediately before handing a picture
/// or input to a viewer. Its lookup reads logind's state in /run, so a slow
/// worker or D-Bus round trip cannot keep an old session selected.
pub struct ActiveSeat {
    get: unsafe extern "C" fn(*const std::ffi::c_char, *mut *mut std::ffi::c_char, *mut u32) -> i32,
    _systemd: libloading::Library,
}
impl ActiveSeat {
    #[cfg(test)]
    pub fn fixture_a() -> Self {
        unsafe extern "C" fn selected(
            _: *const std::ffi::c_char,
            id: *mut *mut std::ffi::c_char,
            uid: *mut u32,
        ) -> i32 {
            unsafe {
                *id = nix::libc::strdup(c"a".as_ptr());
                *uid = 1000;
            }
            0
        }
        let mut seat = Self::open().unwrap();
        seat.get = selected;
        seat
    }
    pub fn open() -> anyhow::Result<Self> {
        // The system unit has a root-controlled dynamic linker environment.
        unsafe {
            let lib = libloading::Library::new("libsystemd.so.0")?;
            let get = *lib.get(b"sd_seat_get_active\0")?;
            Ok(Self { get, _systemd: lib })
        }
    }
    pub fn selected(&self) -> anyhow::Result<Option<(String, u32)>> {
        let mut id = std::ptr::null_mut();
        let mut uid = 0;
        // sd_seat_get_active returns a malloc-owned session identifier.
        let rc = unsafe { (self.get)(c"seat0".as_ptr(), &mut id, &mut uid) };
        if rc == -nix::libc::ENODATA || rc == -nix::libc::ENOENT {
            return Ok(None);
        }
        ensure!(rc >= 0 && !id.is_null(), "cannot query active seat");
        let name = unsafe { std::ffi::CStr::from_ptr(id).to_string_lossy().into_owned() };
        unsafe {
            nix::libc::free(id.cast());
        }
        Ok(Some((name, uid)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session(owner: &str) -> Session {
        Session {
            id: "a".into(),
            owner: owner.into(),
            uid: 1000,
            vt: 2,
            started: 1,
            graphical: true,
            ambiguous: false,
            display: ":0".into(),
            greeter: false,
            wayland: false,
        }
    }

    #[test]
    fn reject_same_id_with_changed_logind_owner() {
        assert!(consistent(
            Some(session(":1.2")),
            Some(session(":1.5")),
            Some(("a".into(), 1000))
        )
        .is_err());
    }

    #[test]
    fn only_explicit_stable_absence_is_empty() {
        assert!(consistent(None, Some(session(":1.2")), None).is_err());
        assert!(consistent(None, None, Some(("b".into(), 1000))).is_err());
        assert_eq!(consistent(None, None, None).unwrap(), None);
    }
}

#[cfg(test)]
mod bus_tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
    };

    struct Bus(Child);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    struct Manager;
    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl Manager {
        fn get_seat(&self, _id: &str) -> OwnedObjectPath {
            "/org/freedesktop/login1/seat/seat0".try_into().unwrap()
        }
    }
    struct Seat;
    #[zbus::interface(name = "org.freedesktop.login1.Seat")]
    impl Seat {
        #[zbus(property)]
        fn active_session(&self) -> (String, OwnedObjectPath) {
            ("a".into(), "/session/a".try_into().unwrap())
        }
    }
    struct Inactive;
    #[zbus::interface(name = "org.freedesktop.login1.Session")]
    impl Inactive {
        #[zbus(property)]
        fn user(&self) -> (u32, OwnedObjectPath) {
            (1000, "/user/a".try_into().unwrap())
        }
        #[zbus(property)]
        fn seat(&self) -> (String, OwnedObjectPath) {
            ("seat0".into(), "/seat/a".try_into().unwrap())
        }
        #[zbus(property)]
        fn active(&self) -> bool {
            false
        }
        #[zbus(property)]
        fn remote(&self) -> bool {
            false
        }
        #[zbus(property, name = "Type")]
        fn kind(&self) -> &str {
            "x11"
        }
        #[zbus(property)]
        fn class(&self) -> &str {
            "user"
        }
    }

    #[test]
    fn selected_session_became_inactive_before_get_all() -> anyhow::Result<()> {
        use zbus::blocking::connection::Builder;
        let mut bus = Bus(Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()?);
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().unwrap()).read_line(&mut address)?;
        let address = address.trim();
        let service = Builder::address(address)?
            .name("org.freedesktop.login1")?
            .serve_at("/org/freedesktop/login1", Manager)?
            .serve_at("/org/freedesktop/login1/seat/seat0", Seat)?
            .serve_at("/session/a", Inactive)?
            .build()?;
        let client = Builder::address(address)?.build()?;
        assert_eq!(
            snapshot(&client).unwrap_err().to_string(),
            "seat changed during observation"
        );
        // Two notifications in the interval between snapshots must both
        // survive the subscription, including invalidated properties.
        let messages = subscribe(&client)?;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for msg in messages.take(2) {
                tx.send(selection_changed(&msg.unwrap()).unwrap()).unwrap();
            }
        });
        for _ in 0..2 {
            service.emit_signal(
                None::<&str>,
                "/org/freedesktop/login1/seat/seat0",
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &(
                    "org.freedesktop.login1.Seat",
                    HashMap::<String, OwnedValue>::new(),
                    vec!["ActiveSession"],
                ),
            )?;
        }
        for _ in 0..2 {
            assert!(rx.recv_timeout(std::time::Duration::from_secs(3))?);
        }
        Ok(())
    }
}
