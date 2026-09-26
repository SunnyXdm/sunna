//! Linux capture backend.
//!
//! Plan (research/03 §1): KMS/DRM scanout capture headless-first (needs
//! cap_sys_admin; works with no display attached — the homelab story), with
//! xdg-desktop-portal ScreenCast + PipeWire (DMA-BUF, not SHM) as the
//! compositor-friendly fallback.
//!
//! What exists today is the X11 path: MIT-SHM `GetImage` of the root window,
//! which works on Xorg and Xvfb (the headless dev VM) with no permissions.
//! It's a CPU copy per frame, fine for software encoding at 1080p.

use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use bytes::Bytes;
use x11rb::connection::Connection;
use x11rb::protocol::shm::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{ImageFormat, Window};
use x11rb::rust_connection::RustConnection;

use crate::{FrameData, FrameSource, PixelFormat, VideoFrame};

/// With nothing changing on screen, still send a frame this often so a
/// viewer that lost one recovers without waiting for the next change.
const IDLE_REDELIVERY: Duration = Duration::from_millis(100);

/// Pixel size of the X screen named by `DISPLAY`.
pub fn main_display_pixel_size() -> anyhow::Result<(u32, u32)> {
    let (conn, screen_num) =
        x11rb::connect(None).context("connecting to the X server (is DISPLAY set?)")?;
    let screen = &conn.setup().roots[screen_num];
    Ok((
        screen.width_in_pixels as u32,
        screen.height_in_pixels as u32,
    ))
}

/// A System V shared-memory segment attached to both us and the X server.
struct ShmSegment {
    addr: *mut u8,
    len: usize,
    xid: shm::Seg,
}

// The mapping is only touched from the thread that owns the ScreenSource.
unsafe impl Send for ShmSegment {}

impl ShmSegment {
    fn new(conn: &RustConnection, len: usize) -> anyhow::Result<Self> {
        unsafe {
            let id = libc::shmget(libc::IPC_PRIVATE, len, libc::IPC_CREAT | 0o600);
            if id < 0 {
                bail!("shmget({len}) failed: {}", std::io::Error::last_os_error());
            }
            let addr = libc::shmat(id, std::ptr::null(), 0);
            if addr as isize == -1 {
                let error = std::io::Error::last_os_error();
                libc::shmctl(id, libc::IPC_RMID, std::ptr::null_mut());
                bail!("shmat failed: {error}");
            }
            let segment = Self {
                addr: addr as *mut u8,
                len,
                xid: conn.generate_id()?,
            };
            conn.shm_attach(segment.xid, id as u32, false)?
                .check()
                .context("XShmAttach")?;
            // Marked for removal now; it goes away once both sides detach.
            libc::shmctl(id, libc::IPC_RMID, std::ptr::null_mut());
            Ok(segment)
        }
    }

    fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.addr, self.len) }
    }
}

impl Drop for ShmSegment {
    fn drop(&mut self) {
        unsafe {
            libc::shmdt(self.addr as *const _);
        }
    }
}

pub struct ScreenSource {
    conn: RustConnection,
    root: Window,
    segment: ShmSegment,
    /// Size of the X screen.
    native: (u32, u32),
    /// Size frames are delivered at (the session's stream size).
    width: u32,
    height: u32,
    fps: u32,
    period: Duration,
    next_deadline: Instant,
    frame_id: u64,
    previous: Option<Bytes>,
    last_sent: Instant,
    last_log: Instant,
    grabs: u32,
    grab_us: u64,
    skipped: u32,
}

impl ScreenSource {
    pub fn new(width: Option<u32>, height: Option<u32>, fps: u32) -> anyhow::Result<Self> {
        let (conn, screen_num) =
            x11rb::connect(None).context("connecting to the X server (is DISPLAY set?)")?;
        conn.shm_query_version()?
            .reply()
            .context("the X server has no MIT-SHM extension")?;
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let native = (
            screen.width_in_pixels as u32,
            screen.height_in_pixels as u32,
        );
        anyhow::ensure!(
            screen.root_depth >= 24,
            "X screen depth {} unsupported (need 24/32)",
            screen.root_depth
        );
        let segment = ShmSegment::new(&conn, native.0 as usize * native.1 as usize * 4)?;
        let fps = fps.max(1);
        let width = width.unwrap_or(native.0).min(native.0).max(2) & !1;
        let height = height.unwrap_or(native.1).min(native.1).max(2) & !1;
        tracing::info!(
            screen = format!("{}x{}", native.0, native.1),
            stream = format!("{width}x{height}"),
            depth = screen.root_depth,
            "X11 capture (MIT-SHM)"
        );
        Ok(Self {
            conn,
            root,
            segment,
            native,
            width,
            height,
            fps,
            period: Duration::from_secs_f64(1.0 / fps as f64),
            next_deadline: Instant::now(),
            frame_id: 0,
            previous: None,
            last_sent: Instant::now(),
            last_log: Instant::now(),
            grabs: 0,
            grab_us: 0,
            skipped: 0,
        })
    }

    /// Grab the root window into the shared segment; returns BGRX bytes at
    /// the stream size.
    fn grab(&mut self) -> anyhow::Result<Bytes> {
        let (w, h) = self.native;
        self.conn
            .shm_get_image(
                self.root,
                0,
                0,
                w as u16,
                h as u16,
                !0,
                ImageFormat::Z_PIXMAP.into(),
                self.segment.xid,
                0,
            )?
            .reply()
            .context("XShmGetImage")?;
        let src = self.segment.bytes();
        if (self.width, self.height) == self.native {
            return Ok(Bytes::copy_from_slice(src));
        }
        // Nearest-neighbour downscale; only used when the viewer's screen is
        // smaller than ours.
        let (dw, dh) = (self.width as usize, self.height as usize);
        let mut out = vec![0u8; dw * dh * 4];
        for y in 0..dh {
            let sy = y * h as usize / dh;
            let row = &src[sy * w as usize * 4..][..w as usize * 4];
            let dst = &mut out[y * dw * 4..][..dw * 4];
            for x in 0..dw {
                let sx = x * w as usize / dw;
                dst[x * 4..x * 4 + 4].copy_from_slice(&row[sx * 4..sx * 4 + 4]);
            }
        }
        Ok(Bytes::from(out))
    }
}

impl FrameSource for ScreenSource {
    fn next_frame(&mut self) -> anyhow::Result<VideoFrame> {
        loop {
            let now = Instant::now();
            if self.next_deadline > now {
                std::thread::sleep(self.next_deadline - now);
            }
            // Pace from the deadline, but don't try to catch up after a stall.
            self.next_deadline = (self.next_deadline + self.period).max(Instant::now());

            let started = Instant::now();
            let data = self.grab()?;
            self.grabs += 1;
            self.grab_us += started.elapsed().as_micros() as u64;
            if self.last_log.elapsed() >= Duration::from_secs(2) {
                tracing::info!(
                    grabs = self.grabs,
                    grab_ms = format!(
                        "{:.1}",
                        self.grab_us as f64 / self.grabs.max(1) as f64 / 1000.0
                    ),
                    unchanged_skipped = self.skipped,
                    "x11 capture window"
                );
                (self.grabs, self.grab_us, self.skipped) = (0, 0, 0);
                self.last_log = Instant::now();
            }

            // Encoding is the expensive part in software, so an unchanged
            // screen only goes out as an occasional refresh.
            let unchanged = self
                .previous
                .as_ref()
                .is_some_and(|previous| previous[..] == data[..]);
            if unchanged && self.last_sent.elapsed() < IDLE_REDELIVERY {
                self.skipped += 1;
                continue;
            }
            self.previous = Some(data.clone());
            self.last_sent = Instant::now();
            self.frame_id += 1;
            return Ok(VideoFrame {
                frame_id: self.frame_id,
                width: self.width,
                height: self.height,
                format: PixelFormat::Bgra8,
                data: FrameData::Cpu(data),
                capture_ts_us: sunna_proto::now_us(),
            });
        }
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn fps(&self) -> u32 {
        self.fps
    }
}
