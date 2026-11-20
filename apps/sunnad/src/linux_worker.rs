//! Worker side of a private socketpair. This entry point is reached only
//! after exec under the chosen uid/gid. No machine credential is passed.
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;

use anyhow::{ensure, Context};
use sunna_capture::FrameSource;
use sunna_input::InputInjector;
use sunna_proto::messages::{decode, ControlMessage};

pub const BEGIN: u8 = 1;
pub const FRAME: u8 = 2;
pub const FRAME_WITH_CURSOR: u8 = 6;
pub const MAX_CURSOR: usize = 256 * 256 * 4 + 128;
pub const INPUT: u8 = 3;
pub const END: u8 = 4;
pub const RELEASE: u8 = 5;
// A zero width is followed by one of these local status codes.
pub const PERMISSION: u32 = 0;
pub const INPUT_MISSING: u32 = 1;
pub const CAPTURE_FAILED: u32 = 2;
pub const WRONG_DEPTH: u32 = 3;
pub const DISPLAY_MISSING: u32 = 4;

pub fn dimensions(w: u32, h: u32) -> anyhow::Result<usize> {
    ensure!(
        (2..=7680).contains(&w) && (2..=4320).contains(&h) && w % 2 == 0 && h % 2 == 0,
        "unsupported screen dimensions"
    );
    Ok(w as usize * h as usize * 4)
}

pub fn number(io: &mut UnixStream) -> anyhow::Result<u32> {
    let mut buf = [0; 4];
    io.read_exact(&mut buf)?;
    Ok(u32::from_be_bytes(buf))
}

// SDDM and GDM can put Xauthority outside HOME. Only inspect processes of
// this worker's uid and session. No session string is evaluated by a shell.
fn x11_environment(session: &str) -> anyhow::Result<(String, Option<std::ffi::OsString>)> {
    use std::collections::BTreeMap;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::MetadataExt;
    let scope = format!("session-{session}.scope");
    let selected = std::env::var("DISPLAY").unwrap_or_default();
    let uid = nix::unistd::getuid().as_raw();
    let mut candidates = BTreeMap::new();
    for entry in std::fs::read_dir("/proc")?.flatten() {
        if !entry
            .file_name()
            .as_encoded_bytes()
            .iter()
            .all(u8::is_ascii_digit)
        {
            continue;
        }
        let Ok(dir) = std::fs::File::open(entry.path()) else {
            continue;
        };
        use std::os::fd::AsRawFd;
        let pinned = std::path::PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd()));
        if !dir.metadata().is_ok_and(|m| m.uid() == uid) {
            continue;
        }
        if !std::fs::read_to_string(pinned.join("cgroup")).is_ok_and(|s| {
            s.lines()
                .any(|line| line.split('/').any(|part| part == scope))
        }) {
            continue;
        }
        let Ok(file) = std::fs::File::open(pinned.join("environ")) else {
            continue;
        };
        let mut fields = Vec::new();
        file.take(128 * 1024).read_to_end(&mut fields)?;
        let vars = fields.split(|b| *b == 0);
        let Some(display) = vars
            .clone()
            .find_map(|v| v.strip_prefix(b"DISPLAY="))
            .and_then(|b| std::str::from_utf8(b).ok())
        else {
            continue;
        };
        let Some(number) = display.strip_prefix(':') else {
            continue;
        };
        if number.is_empty()
            || number.len() > 5
            || !number.bytes().all(|b| b.is_ascii_digit())
            || (!selected.is_empty() && display != selected)
        {
            continue;
        }
        let entry = candidates.entry(display.to_owned()).or_insert(None);
        if let Some(path) = vars
            .filter_map(|v| v.strip_prefix(b"XAUTHORITY="))
            .find(|p| p.starts_with(b"/"))
        {
            *entry = Some(std::ffi::OsString::from_vec(path.to_vec()));
        }
    }
    if candidates.is_empty() && !selected.is_empty() {
        return Ok((selected, None));
    }
    ensure!(
        candidates.len() == 1,
        "no unique local display in the selected session"
    );
    Ok(candidates.into_iter().next().unwrap())
}

pub fn run(session: &str) -> anyhow::Result<()> {
    ensure!(
        !nix::unistd::geteuid().is_root(),
        "worker requires a session uid"
    );
    let seat = std::env::var_os("SUNNA_WORKER_SESSION")
        .map(|_| crate::linux_seat::ActiveSeat::open())
        .transpose()?;
    let owner = (session.to_string(), nix::unistd::getuid().as_raw());
    let wayland = sunna_portal::use_portal();
    // A Wayland worker may have an Xwayland clipboard in the same session.
    let x11 = match x11_environment(session) {
        Ok(x11) => Some(x11),
        Err(_) if wayland => None,
        Err(error) => return Err(error),
    };
    if let Some((display, authority)) = x11 {
        std::env::set_var("DISPLAY", display);
        if let Some(path) = authority {
            // Single threaded, before any backend or async runtime is started.
            unsafe { std::env::set_var("XAUTHORITY", path) };
        }
    }
    let active: sunna_host::SessionCheck = std::sync::Arc::new(move || {
        seat.as_ref()
            .is_none_or(|s| s.selected().ok().flatten().as_ref() == Some(&owner))
    });
    super::linux_machine::start_media_worker(active.clone())?;
    let fd: OwnedFd = std::io::stdin().as_fd().try_clone_to_owned()?;
    use std::os::fd::AsFd;
    let mut io = UnixStream::from(fd);
    let mut portal = None;
    let native = if wayland {
        if !sunna_portal::session::has_restore_token() {
            io.write_all(&0u32.to_be_bytes())?;
            io.write_all(&PERMISSION.to_be_bytes())?;
            return Ok(());
        }
        let Ok(source) = sunna_portal::PortalSource::new(None, None, 2) else {
            io.write_all(&0u32.to_be_bytes())?;
            io.write_all(&PERMISSION.to_be_bytes())?;
            return Ok(());
        };
        let size = (source.width(), source.height());
        portal = Some(sunna_portal::session::acquire()?);
        size
    } else {
        match super::linux_desktop::check(None, None) {
            Ok(size) => size,
            Err(reason) => {
                let code = match reason {
                    super::linux_desktop::INPUT => INPUT_MISSING,
                    super::linux_desktop::CAPTURE => CAPTURE_FAILED,
                    super::linux_desktop::COLOR => WRONG_DEPTH,
                    _ => DISPLAY_MISSING,
                };
                io.write_all(&0u32.to_be_bytes())?;
                io.write_all(&code.to_be_bytes())?;
                return Ok(());
            }
        }
    };
    let _grant = portal;
    if !wayland {
        let mut capture = sunna_capture::linux::ScreenSource::new(Some(320), Some(240), 2)?;
        capture.next_frame()?;
        drop(capture);
        let shutdown = io.try_clone()?;
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(3));
            // A worker's native dimensions are part of its viewer binding.
            // A resized X server needs fresh negotiation, even on the same VT.
            if super::linux_desktop::check(None, None) != Ok(native) {
                let _ = shutdown.shutdown(std::net::Shutdown::Both);
                break;
            }
        });
    }
    io.write_all(&(native.0 & !1).to_be_bytes())?;
    io.write_all(&(native.1 & !1).to_be_bytes())?;
    let mut pipeline: Option<(Box<dyn FrameSource>, Box<dyn InputInjector>, u32, u32)> = None;
    let mut cursor = None;
    let mut cursor_id = None;
    let mut op = [0];
    while io.read_exact(&mut op).is_ok() {
        match op[0] {
            BEGIN => {
                let w = number(&mut io)?;
                let h = number(&mut io)?;
                dimensions(w, h)?;
                let fps = number(&mut io)?.clamp(1, 120);
                let (source, input): (Box<dyn FrameSource>, Box<dyn InputInjector>) = if wayland {
                    let source = sunna_portal::PortalSource::new(Some(w), Some(h), fps)?;
                    (
                        Box::new(source),
                        Box::new(sunna_portal::PortalInjector::new()?),
                    )
                } else {
                    (
                        Box::new(sunna_capture::linux::ScreenSource::new(
                            Some(w),
                            Some(h),
                            fps,
                        )?),
                        Box::new(sunna_input::linux::X11Injector::new()?),
                    )
                };
                if let Some((_, input, _, _)) = &mut pipeline {
                    input.release_all();
                }
                cursor = sunna_capture::cursor::reader().ok();
                cursor_id = None;
                pipeline = Some((source, input, w, h));
                io.write_all(&[1])?;
            }
            FRAME | FRAME_WITH_CURSOR => {
                if op[0] == FRAME_WITH_CURSOR {
                    let image = cursor.as_mut().and_then(|read| read().ok().flatten());
                    let mut bytes = Vec::new();
                    if let Some(im) = image.filter(|im| {
                        Some(im.id) != cursor_id && im.width <= 256 && im.height <= 256
                    }) {
                        cursor_id = Some(im.id);
                        bytes = sunna_proto::messages::encode(&ControlMessage::Cursor(
                            sunna_proto::messages::CursorShape {
                                id: im.id,
                                width: im.width,
                                height: im.height,
                                hot_x: im.hot_x,
                                hot_y: im.hot_y,
                                screen_width: im.screen_width,
                                rgba: im.rgba,
                            },
                        ))?;
                    }
                    io.write_all(&(bytes.len() as u32).to_be_bytes())?;
                    io.write_all(&bytes)?;
                }
                let (source, _, w, h) = pipeline.as_mut().context("no active stream")?;
                let frame = source.next_frame()?;
                let bytes = frame.data.to_cpu()?;
                ensure!(bytes.len() == dimensions(frame.width, frame.height)?);
                if (frame.width, frame.height) == (*w, *h) {
                    io.write_all(&bytes)?;
                } else {
                    // The backend can resize during this worker's lifetime.
                    // Send the size negotiated by this viewer.
                    let mut out = vec![0; dimensions(*w, *h)?];
                    for y in 0..*h {
                        for x in 0..*w {
                            let from = ((y * frame.height / *h) * frame.width
                                + x * frame.width / *w)
                                as usize
                                * 4;
                            let to = (y * *w + x) as usize * 4;
                            out[to..to + 4].copy_from_slice(&bytes[from..from + 4]);
                        }
                    }
                    io.write_all(&out)?;
                }
            }
            INPUT => {
                let len = number(&mut io)? as usize;
                ensure!(len <= 4096);
                let mut bytes = vec![0; len];
                io.read_exact(&mut bytes)?;
                if let (Some((_, input, _, _)), ControlMessage::Input(event)) =
                    (&mut pipeline, decode(&bytes)?)
                {
                    if !active() {
                        input.release_all();
                        break;
                    }
                    input.inject(&event)?;
                }
            }
            END | RELEASE => {
                if let Some((_, input, _, _)) = &mut pipeline {
                    input.release_all();
                }
                if op[0] == END {
                    pipeline = None;
                }
            }
            _ => anyhow::bail!("invalid worker request"),
        }
    }
    if let Some((_, input, _, _)) = &mut pipeline {
        input.release_all();
    }
    Ok(())
}
