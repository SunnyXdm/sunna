//! Audio and clipboard use a separate inherited socket. The listener accepts
//! fixed-size PCM and bounded clipboard payloads only from the displayed UID.
use super::{Machine, Worker};
use anyhow::{ensure, Context, Result};
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use sunna_audio::{Capture, FRAME_DURATION, FRAME_LEN};
use sunna_clipboard::{Clipboard, ClipboardContent, ClipboardSync, MAX_CONTENT_BYTES};

const AUDIO: u8 = 1;
const POLL: u8 = 2;
const PUT: u8 = 3;
const STOP_AUDIO: u8 = 4;
const END: u8 = 5;
type Owner = (Arc<Worker>, u64);

impl Machine {
    fn displayed(&self, bound: &Weak<Worker>) -> Option<Owner> {
        let (weak, stream) = self.prepared.lock().unwrap().clone()?;
        if !weak.ptr_eq(bound) {
            return None;
        }
        let worker = weak.upgrade()?;
        (!worker.greeter && self.is_current(&worker)).then_some((worker, stream))
    }
}

impl Worker {
    fn media<T>(
        &self,
        stream: u64,
        op: u8,
        allowed: impl FnOnce() -> bool,
        body: impl FnOnce(&mut UnixStream) -> Result<T>,
    ) -> Option<T> {
        let mut channel = self.media.lock().unwrap();
        let io = channel.as_mut()?;
        if !allowed() {
            return None;
        }
        let result = (|| {
            let mut header = [0; 9];
            header[0] = op;
            header[1..].copy_from_slice(&stream.to_be_bytes());
            io.write_all(&header)?;
            body(io)
        })();
        if result.is_err() {
            // A stalled audio server or malformed clipboard must never stall input.
            channel.take();
            self.failed
                .store(true, std::sync::atomic::Ordering::Release);
        }
        result.ok()
    }
}

fn read_byte(io: &mut UnixStream) -> Result<u8> {
    let mut byte = [0];
    io.read_exact(&mut byte)?;
    Ok(byte[0])
}

fn send_copy(io: &mut UnixStream, content: &ClipboardContent) -> Result<()> {
    let (kind, mut bytes): (u8, &[u8]) = match content {
        ClipboardContent::Text(s) => (1, s.as_bytes()),
        ClipboardContent::Png(bytes) => (2, bytes),
    };
    ensure!(bytes.len() <= MAX_CONTENT_BYTES);
    io.write_all(&[kind])?;
    io.write_all(&(bytes.len() as u32).to_be_bytes())?;
    let start = Instant::now();
    while !bytes.is_empty() {
        ensure!(start.elapsed() < Duration::from_secs(3));
        let n = io.write(bytes)?;
        ensure!(n != 0);
        bytes = &bytes[n..];
    }
    Ok(())
}

fn read_copy(io: &mut UnixStream) -> Result<Option<ClipboardContent>> {
    let kind = read_byte(io)?;
    if kind == 0 {
        return Ok(None);
    }
    ensure!(matches!(kind, 1 | 2));
    let n = crate::linux_worker::number(io)? as usize;
    ensure!(n <= MAX_CONTENT_BYTES);
    let mut bytes = vec![0; n];
    bounded_read(io, &mut bytes)?;
    Ok(Some(if kind == 1 {
        ClipboardContent::Text(String::from_utf8(bytes)?)
    } else {
        ClipboardContent::Png(bytes)
    }))
}

pub(super) fn bounded_read(io: &mut UnixStream, mut tail: &mut [u8]) -> Result<()> {
    let start = Instant::now();
    while !tail.is_empty() {
        ensure!(start.elapsed() < Duration::from_secs(3));
        let n = io.read(tail)?;
        ensure!(n != 0);
        tail = &mut tail[n..];
    }
    Ok(())
}

// All local backend handles in this function belong to the dropped UID.
pub(super) fn serve(selected: sunna_host::SessionCheck) -> Result<()> {
    let io = UnixStream::from(std::io::stdout().as_fd().try_clone_to_owned()?);
    std::thread::Builder::new()
        .name("session-media".into())
        .spawn(move || {
            let _ = session_loop(io, selected);
        })?;
    Ok(())
}

fn session_loop(mut io: UnixStream, selected: sunna_host::SessionCheck) -> Result<()> {
    let mut active = None;
    let mut audio: Option<Box<dyn Capture>> = None;
    let mut clipboard: Option<(Box<dyn Clipboard>, ClipboardSync)> = None;
    let mut audio_retry = Instant::now();
    let mut clip_retry = Instant::now();
    let mut header = [0; 9];
    while io.read_exact(&mut header).is_ok() {
        let stream = u64::from_be_bytes(header[1..].try_into()?);
        let op = header[0];
        if matches!(op, END | STOP_AUDIO) {
            if active == Some(stream) {
                audio = None;
                if op == END {
                    clipboard = None;
                    active = None;
                }
            }
            continue;
        }
        ensure!(selected(), "session changed");
        if active != Some(stream) {
            active = Some(stream);
            audio = None;
            clipboard = None;
            audio_retry = Instant::now();
            clip_retry = Instant::now();
        }
        if op == AUDIO {
            if audio.is_none() && Instant::now() >= audio_retry {
                audio = sunna_audio::open_capture().ok();
                audio_retry = Instant::now() + Duration::from_secs(3);
            }
            let mut frame = [0; FRAME_LEN];
            if audio.as_mut().is_some_and(|a| a.read(&mut frame).is_ok()) {
                let mut response = [0; 1 + FRAME_LEN * 2];
                response[0] = 1;
                for (sample, dst) in frame.iter().zip(response[1..].chunks_exact_mut(2)) {
                    dst.copy_from_slice(&sample.to_le_bytes());
                }
                io.write_all(&response)?;
            } else {
                audio = None;
                io.write_all(&[0])?;
            }
            continue;
        }
        ensure!(matches!(op, POLL | PUT));
        if clipboard.is_none() && Instant::now() >= clip_retry {
            clipboard = sunna_clipboard::system_clipboard().ok().map(|mut clip| {
                let sync = ClipboardSync::new(clip.change_count());
                (clip, sync)
            });
            clip_retry = Instant::now() + Duration::from_secs(3);
        }
        if op == POLL {
            let content = clipboard
                .as_mut()
                .and_then(|(clip, sync)| sync.poll(clip.as_mut()));
            match content {
                Some(content) => send_copy(&mut io, &content)?,
                None => io.write_all(&[0])?,
            }
        } else {
            let content = read_copy(&mut io)?.context("missing clipboard content")?;
            if let Some((clip, sync)) = &mut clipboard {
                ensure!(selected(), "session changed");
                sync.receive(clip.as_mut(), content);
            }
        }
    }
    Ok(())
}

pub(super) fn end(worker: &Worker, stream: u64) {
    worker.media(stream, END, || true, |_| Ok(()));
}

struct Audio {
    bound: Weak<Worker>,
    machine: Arc<Machine>,
    last: Option<(Weak<Worker>, u64)>,
}

impl Capture for Audio {
    fn read(&mut self, frame: &mut [i16; FRAME_LEN]) -> Result<()> {
        let before = Instant::now();
        if let Some((worker, stream)) = self.machine.displayed(&self.bound) {
            self.last = Some((Arc::downgrade(&worker), stream));
            let got = worker.media(
                stream,
                AUDIO,
                || self.machine.is_current(&worker),
                |io| {
                    if read_byte(io)? == 0 {
                        return Ok(false);
                    }
                    let mut bytes = [0; FRAME_LEN * 2];
                    bounded_read(io, &mut bytes)?;
                    for (dst, src) in frame.iter_mut().zip(bytes.chunks_exact(2)) {
                        *dst = i16::from_le_bytes([src[0], src[1]]);
                    }
                    Ok(true)
                },
            );
            if got == Some(true) && self.machine.is_current(&worker) {
                return Ok(());
            }
        }
        frame.fill(0);
        std::thread::sleep(FRAME_DURATION.saturating_sub(before.elapsed()));
        Ok(())
    }
}
impl Drop for Audio {
    fn drop(&mut self) {
        if let Some((owner, stream)) = self.last.take() {
            if let Some(worker) = owner.upgrade() {
                worker.media(stream, STOP_AUDIO, || true, |_| Ok(()));
            }
        }
    }
}

struct Copies {
    bound: Weak<Worker>,
    machine: Arc<Machine>,
    owner: Option<(Weak<Worker>, u64)>,
    count: u64,
    copy: Option<ClipboardContent>,
}
impl Copies {
    fn target(&self) -> Option<Owner> {
        let target = self.machine.displayed(&self.bound)?;
        let (old, stream) = self.owner.as_ref()?;
        (old.ptr_eq(&Arc::downgrade(&target.0)) && *stream == target.1).then_some(target)
    }
}
impl Clipboard for Copies {
    fn read(&mut self) -> Option<ClipboardContent> {
        self.target()?;
        self.copy.take()
    }
    fn write(&mut self, content: &ClipboardContent) {
        if let Some((worker, stream)) = self.target() {
            worker.media(
                stream,
                PUT,
                || self.machine.is_current(&worker),
                |io| send_copy(io, content),
            );
        }
    }
    fn change_count(&mut self) -> u64 {
        self.copy = None;
        let Some((worker, stream)) = self.machine.displayed(&self.bound) else {
            self.owner = None;
            return self.count;
        };
        self.owner = Some((Arc::downgrade(&worker), stream));
        self.copy = worker
            .media(stream, POLL, || self.machine.is_current(&worker), read_copy)
            .flatten();
        if self.copy.is_some() && self.machine.is_current(&worker) {
            self.count = self.count.wrapping_add(1);
        }
        self.count
    }
}

impl sunna_host::SessionDevices for Machine {
    fn bind_viewer(
        self: Arc<Self>,
        connection: &sunna_transport::quinn::Connection,
    ) -> Result<((u32, u32), sunna_host::SessionCheck)> {
        let generation = self.generation.lock().unwrap();
        let current = self.current.lock().unwrap();
        let worker = current.as_ref().context("no active session")?;
        ensure!(
            worker.generation == *generation && !worker.failed.load(std::sync::atomic::Ordering::Acquire)
                && self.seat.selected()?.as_ref() == Some(&worker.owner),
            "session changed"
        );
        *worker.viewer.lock().unwrap() = Some(connection.clone());
        *self.bound.lock().unwrap() = Arc::downgrade(worker);
        let bound = worker.clone();
        drop(current);
        drop(generation);
        Ok((bound.native, Arc::new(move || self.is_current(&bound))))
    }
    fn audio(self: Arc<Self>) -> Box<dyn Capture> {
        let bound = self.bound.lock().unwrap().clone();
        Box::new(Audio {
            bound,
            machine: self,
            last: None,
        })
    }
    fn clipboard(self: Arc<Self>) -> Box<dyn Clipboard> {
        let bound = self.bound.lock().unwrap().clone();
        Box::new(Copies {
            bound,
            machine: self,
            owner: None,
            count: 0,
            copy: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn losing_the_media_socket_retires_the_worker() {
        let (io, _display) = UnixStream::pair().unwrap();
        let (media, peer) = UnixStream::pair().unwrap();
        drop(peer);
        let w = Worker {
            generation: 1,
            media: std::sync::Mutex::new(Some(media)),
            greeter: false,
            native: (32, 32),
            owner: ("a".into(), 1000),
            failed: Default::default(),
            shutdown: io.try_clone().unwrap(),
            pipe: std::sync::Mutex::new(super::super::Pipe { io, stream: None }),
            viewer: Default::default(),
        };
        assert!(w.media(1, AUDIO, || true, read_byte).is_none());
        assert!(w.failed.load(std::sync::atomic::Ordering::Acquire));
        assert!(w.media.lock().unwrap().is_none());
    }

    #[test]
    fn oversized_or_truncated_copies_are_rejected_before_delivery() {
        for (length, supplied) in [(MAX_CONTENT_BYTES as u32 + 1, 0usize), (12, 2)] {
            let (mut session, mut listener) = UnixStream::pair().unwrap();
            session.write_all(&[2]).unwrap();
            session.write_all(&length.to_be_bytes()).unwrap();
            session.write_all(&vec![0; supplied]).unwrap();
            drop(session);
            assert!(read_copy(&mut listener).is_err());
        }
    }
}
