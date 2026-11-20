//! A filesystem Unix socket, with peer credentials checked before X11 setup.
use anyhow::{ensure, Context};
use std::fs;
use std::os::fd::AsRawFd;
use std::os::unix::{fs::MetadataExt, net::UnixStream};
use x11rb::rust_connection::{DefaultStream, RustConnection};

fn display_number(display: &str) -> anyhow::Result<(u16, usize)> {
    let local = display
        .strip_prefix(':')
        .context("expected a local display")?;
    let (number, screen) = local.split_once('.').unwrap_or((local, "0"));
    ensure!(number
        .bytes()
        .chain(screen.bytes())
        .all(|b| b.is_ascii_digit()));
    Ok((number.parse()?, screen.parse()?))
}

pub fn connect(display: Option<&str>) -> anyhow::Result<(RustConnection, usize)> {
    let default = std::env::var("DISPLAY").unwrap_or_default();
    let (number, screen) = display_number(display.unwrap_or(&default))?;
    let socket = UnixStream::connect(format!("/tmp/.X11-unix/X{number}"))?;
    verify_server(&socket)?;
    let (stream, (family, address)) = DefaultStream::from_unix_stream(socket)?;
    let (name, cookie) = match x11rb_protocol::xauth::get_auth(family, &address, number) {
        Ok(auth) => auth.unwrap_or_default(),
        // Some verified local servers authorize by Unix uid (or disable Xauth).
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Default::default(),
        Err(error) => return Err(error.into()),
    };
    let conn = RustConnection::connect_to_stream_with_auth_info(stream, screen, name, cookie)?;
    ensure!(
        screen < x11rb::connection::Connection::setup(&conn).roots.len(),
        "invalid X screen"
    );
    Ok((conn, screen))
}

fn verify_server(socket: &UnixStream) -> anyhow::Result<()> {
    let mut peer = std::mem::MaybeUninit::<libc::ucred>::uninit();
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            peer.as_mut_ptr().cast(),
            &mut len,
        )
    };
    ensure!(
        rc == 0 && len as usize == std::mem::size_of::<libc::ucred>(),
        "missing display peer credentials"
    );
    let peer = unsafe { peer.assume_init() };
    ensure!(
        peer.pid > 0 && (peer.uid == 0 || peer.uid == unsafe { libc::geteuid() }),
        "unexpected X server uid"
    );
    let dir = fs::File::open(format!("/proc/{}", peer.pid))?;
    let base = std::path::PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd()));
    if peer.uid == 0 {
        // The session uid cannot follow /proc/PID/exe for root-owned Xorg.
        // The kernel-authenticated root peer controls its own process name.
        let name = fs::read_to_string(base.join("comm"))?;
        ensure!(
            matches!(name.trim_end(), "Xorg" | "X" | "Xwayland" | "Xvfb"),
            "unrecognized root X display server"
        );
    } else {
        let exe = fs::metadata(base.join("exe"))?;
        ensure!(
            [
                "/usr/bin/Xvfb",
                "/usr/lib/xorg/Xorg",
                "/usr/bin/Xorg",
                "/usr/bin/Xwayland",
                "/usr/libexec/Xorg"
            ]
            .iter()
            .filter_map(|path| fs::metadata(path).ok())
            .any(|m| m.uid() == 0
                && m.mode() & 0o022 == 0
                && m.dev() == exe.dev()
                && m.ino() == exe.ino()),
            "unrecognized X display server"
        );
    }
    if let Ok(session) = std::env::var("SUNNA_WORKER_SESSION").or_else(|error| {
        if matches!(
            std::env::var("XDG_SESSION_TYPE").as_deref(),
            Ok("wayland" | "x11")
        ) {
            std::env::var("XDG_SESSION_ID")
        } else {
            Err(error)
        }
    }) {
        let scope = format!("session-{session}.scope");
        let in_session = fs::read_to_string(base.join("cgroup"))?
            .lines()
            .any(|line| line.split('/').any(|part| part == scope));
        if !in_session {
            // LightDM and SDDM own root Xorg outside the user's scope.
            let vt: u32 = std::env::var("XDG_VTNR")?.parse()?;
            let arg = format!("vt{vt}");
            ensure!(
                peer.uid == 0
                    && vt != 0
                    && fs::read(base.join("cmdline"))?
                        .split(|b| *b == 0)
                        .any(|a| a == arg.as_bytes()),
                "X server belongs to another session"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_an_unrelated_local_peer_before_setup() {
        use std::io::Read;
        let (socket, mut peer) = UnixStream::pair().unwrap();
        assert!(verify_server(&socket).is_err());
        drop(socket);
        assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
    }
    #[test]
    fn only_local_display_numbers() {
        assert_eq!(display_number(":12.1").unwrap(), (12, 1));
        for value in ["localhost:0", ":../x", ":2.foo", ":3.1.2"] {
            assert!(display_number(value).is_err());
        }
    }
}
