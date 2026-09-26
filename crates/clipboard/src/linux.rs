//! X11 selections need a live owner to serve paste requests.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEventMask};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ConnectionExt as _, CreateWindowAux, EventMask,
    PropMode, Property, SelectionNotifyEvent, SelectionRequestEvent, Window, WindowClass,
    SELECTION_NOTIFY_EVENT,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{COPY_DEPTH_FROM_PARENT, CURRENT_TIME, NONE};

use crate::{within_limit, Clipboard, ClipboardContent, MAX_CONTENT_BYTES};

const TIMEOUT: Duration = Duration::from_millis(500);

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        CLIPBOARD, TARGETS, UTF8_STRING, STRING, TEXT,
        text_plain: b"text/plain;charset=utf-8",
        image_png: b"image/png",
        INCR, SUNNA_CLIPBOARD,
    }
}

enum Command {
    Read(mpsc::Sender<Option<ClipboardContent>>),
    Write(ClipboardContent, mpsc::Sender<()>),
}

pub struct X11Clipboard {
    commands: mpsc::Sender<Command>,
    count: Arc<AtomicU64>,
}

impl X11Clipboard {
    pub fn new() -> anyhow::Result<Self> {
        let (commands, rx) = mpsc::channel();
        let (ready, initialized) = mpsc::channel();
        let count = Arc::new(AtomicU64::new(0));
        let worker_count = count.clone();
        std::thread::Builder::new()
            .name("clipboard-x11".into())
            .spawn(move || {
                let mut owner = match Owner::new(worker_count) {
                    Ok(owner) => owner,
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                if ready.send(Ok(())).is_err() {
                    return;
                }
                if let Err(error) = owner.run(rx) {
                    tracing::debug!(%error, "X11 clipboard worker stopped");
                }
            })?;
        initialized.recv()??;
        Ok(Self { commands, count })
    }
}

impl Clipboard for X11Clipboard {
    fn read(&mut self) -> Option<ClipboardContent> {
        let (tx, rx) = mpsc::channel();
        self.commands.send(Command::Read(tx)).ok()?;
        rx.recv().ok().flatten()
    }

    fn write(&mut self, content: &ClipboardContent) {
        if !within_limit(content.bytes().len()) {
            return;
        }
        let (tx, rx) = mpsc::channel();
        if self
            .commands
            .send(Command::Write(content.clone(), tx))
            .is_ok()
        {
            let _ = rx.recv();
        }
    }

    fn change_count(&mut self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }
}

struct Transfer {
    data: Arc<Vec<u8>>,
    target: Atom,
    offset: usize,
    touched: Instant,
}

struct Owner {
    conn: RustConnection,
    root: Window,
    window: Window,
    atoms: Atoms,
    content: Option<ClipboardContent>,
    count: Arc<AtomicU64>,
    transfers: HashMap<(Window, Atom), Transfer>,
    chunk_size: usize,
}

impl Owner {
    fn new(count: Arc<AtomicU64>) -> anyhow::Result<Self> {
        let (conn, screen) = x11rb::connect(None)?;
        conn.xfixes_query_version(5, 0)?.reply()?;
        let atoms = Atoms::new(&conn)?.reply()?;
        let root = conn.setup().roots[screen].root;
        let window = new_window(&conn, root)?;
        conn.xfixes_select_selection_input(
            window,
            atoms.CLIPBOARD,
            SelectionEventMask::SET_SELECTION_OWNER
                | SelectionEventMask::SELECTION_WINDOW_DESTROY
                | SelectionEventMask::SELECTION_CLIENT_CLOSE,
        )?
        .check()?;
        let chunk_size = (conn.maximum_request_bytes() - 64).min(64 * 1024);
        conn.flush()?;
        Ok(Self {
            conn,
            root,
            window,
            atoms,
            content: None,
            count,
            transfers: HashMap::new(),
            chunk_size,
        })
    }

    fn run(&mut self, commands: mpsc::Receiver<Command>) -> anyhow::Result<()> {
        loop {
            while let Some(event) = self.conn.poll_for_event()? {
                self.event(event)?;
            }
            self.transfers
                .retain(|_, transfer| transfer.touched.elapsed() < Duration::from_secs(5));
            match commands.recv_timeout(Duration::from_millis(2)) {
                Ok(Command::Read(reply)) => {
                    let content = self.read();
                    let _ = reply.send(content);
                }
                Ok(Command::Write(content, reply)) => {
                    self.content = Some(content);
                    self.conn
                        .set_selection_owner(self.window, self.atoms.CLIPBOARD, CURRENT_TIME)?
                        .check()?;
                    self.conn.flush()?;
                    let _ = reply.send(());
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    fn event(&mut self, event: Event) -> anyhow::Result<()> {
        match event {
            Event::XfixesSelectionNotify(event) if event.selection == self.atoms.CLIPBOARD => {
                self.count.fetch_add(1, Ordering::Relaxed);
            }
            Event::SelectionClear(_) => self.content = None,
            Event::SelectionRequest(request) => {
                // A requestor may disappear between any two requests.
                if let Err(error) = self.serve(request) {
                    tracing::debug!(%error, "clipboard selection request failed");
                }
            }
            Event::PropertyNotify(event) if event.state == Property::DELETE => {
                let key = (event.window, event.atom);
                if let Some(mut transfer) = self.transfers.remove(&key) {
                    let end = (transfer.offset + self.chunk_size).min(transfer.data.len());
                    let chunk = &transfer.data[transfer.offset..end];
                    let sent = self
                        .conn
                        .change_property8(
                            PropMode::REPLACE,
                            event.window,
                            event.atom,
                            transfer.target,
                            chunk,
                        )?
                        .check()
                        .is_ok();
                    if sent && !chunk.is_empty() {
                        transfer.offset = end;
                        transfer.touched = Instant::now();
                        self.transfers.insert(key, transfer);
                    }
                    self.conn.flush()?;
                }
            }
            Event::DestroyNotify(event) => self
                .transfers
                .retain(|(window, _), _| *window != event.window),
            _ => {}
        }
        Ok(())
    }

    fn targets(&self) -> Vec<Atom> {
        let mut targets = vec![self.atoms.TARGETS];
        match self.content {
            Some(ClipboardContent::Text(_)) => targets.extend([
                self.atoms.UTF8_STRING,
                self.atoms.STRING,
                self.atoms.TEXT,
                self.atoms.text_plain,
            ]),
            Some(ClipboardContent::Png(_)) => targets.push(self.atoms.image_png),
            None => {}
        }
        targets
    }

    fn serve(&mut self, request: SelectionRequestEvent) -> anyhow::Result<()> {
        let property = if request.property == NONE {
            request.target
        } else {
            request.property
        };
        let supported = request.selection == self.atoms.CLIPBOARD
            && self.targets().contains(&request.target)
            && self.content.is_some();
        let mut reply_property = NONE;
        if supported && request.target == self.atoms.TARGETS {
            self.conn
                .change_property32(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    AtomEnum::ATOM,
                    &self.targets(),
                )?
                .check()?;
            reply_property = property;
        } else if supported {
            let content = self.content.as_ref().unwrap();
            let data = match content {
                ClipboardContent::Text(text) if request.target == self.atoms.STRING => text
                    .chars()
                    .map(|c| if u32::from(c) <= 255 { c as u8 } else { b'?' })
                    .collect(),
                _ => content.bytes().to_vec(),
            };
            let target = if request.target == self.atoms.TEXT {
                self.atoms.UTF8_STRING
            } else {
                request.target
            };
            if data.len() <= self.chunk_size {
                self.conn
                    .change_property8(
                        PropMode::REPLACE,
                        request.requestor,
                        property,
                        target,
                        &data,
                    )?
                    .check()?;
                reply_property = property;
            } else if self.transfers.len() < 32 {
                self.conn
                    .change_window_attributes(
                        request.requestor,
                        &ChangeWindowAttributesAux::new()
                            .event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY),
                    )?
                    .check()?;
                self.conn
                    .change_property32(
                        PropMode::REPLACE,
                        request.requestor,
                        property,
                        self.atoms.INCR,
                        &[data.len() as u32],
                    )?
                    .check()?;
                self.transfers.insert(
                    (request.requestor, property),
                    Transfer {
                        data: Arc::new(data),
                        target,
                        offset: 0,
                        touched: Instant::now(),
                    },
                );
                reply_property = property;
            }
        }
        self.conn
            .send_event(
                false,
                request.requestor,
                EventMask::NO_EVENT,
                SelectionNotifyEvent {
                    response_type: SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time: request.time,
                    requestor: request.requestor,
                    selection: request.selection,
                    target: request.target,
                    property: reply_property,
                },
            )?
            .check()?;
        self.conn.flush()?;
        Ok(())
    }

    fn read(&mut self) -> Option<ClipboardContent> {
        let owner = self
            .conn
            .get_selection_owner(self.atoms.CLIPBOARD)
            .ok()?
            .reply()
            .ok()?
            .owner;
        if owner == NONE {
            return None;
        }
        if owner == self.window {
            return self.content.clone();
        }
        if let Some(data) = self.convert(self.atoms.image_png) {
            return Some(ClipboardContent::Png(data));
        }
        String::from_utf8(self.convert(self.atoms.UTF8_STRING)?)
            .ok()
            .map(ClipboardContent::Text)
    }

    fn convert(&mut self, target: Atom) -> Option<Vec<u8>> {
        // Separate windows prevent late replies from a timed-out conversion
        // being mistaken for a subsequent request, and cancel abandoned INCRs.
        let window = new_window(&self.conn, self.root).ok()?;
        let result = self.receive(window, target);
        let _ = self.conn.destroy_window(window);
        let _ = self.conn.flush();
        match result {
            Ok(data) => data,
            Err(error) => {
                tracing::debug!(%error, "clipboard conversion failed");
                None
            }
        }
    }

    fn receive(&mut self, window: Window, target: Atom) -> anyhow::Result<Option<Vec<u8>>> {
        let property = self.atoms.SUNNA_CLIPBOARD;
        self.conn
            .convert_selection(window, self.atoms.CLIPBOARD, target, property, CURRENT_TIME)?
            .check()?;
        self.conn.flush()?;
        let mut deadline = Instant::now() + TIMEOUT;
        let total_deadline = Instant::now() + Duration::from_secs(5);
        let mut incremental = false;
        let mut data = Vec::new();
        loop {
            if Instant::now() >= deadline || Instant::now() >= total_deadline {
                return Ok(None);
            }
            let Some(event) = self.conn.poll_for_event()? else {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            };
            let ready = match &event {
                Event::SelectionNotify(event)
                    if event.requestor == window
                        && event.target == target
                        && event.selection == self.atoms.CLIPBOARD =>
                {
                    if event.property == NONE {
                        return Ok(None);
                    }
                    event.property == property && !incremental
                }
                Event::PropertyNotify(event) => {
                    incremental
                        && event.window == window
                        && event.atom == property
                        && event.state == Property::NEW_VALUE
                }
                _ => false,
            };
            if !ready {
                self.event(event)?;
                continue;
            }
            let reply = self
                .conn
                .get_property(
                    true,
                    window,
                    property,
                    AtomEnum::ANY,
                    0,
                    (MAX_CONTENT_BYTES / 4 + 1) as u32,
                )?
                .reply()?;
            if reply.bytes_after != 0 || !within_limit(data.len() + reply.value.len()) {
                tracing::debug!("ignoring oversized X11 clipboard property");
                return Ok(None);
            }
            if !incremental && reply.type_ == self.atoms.INCR {
                let Some(size) = reply.value32().and_then(|mut values| values.next()) else {
                    return Ok(None);
                };
                if !within_limit(size as usize) {
                    return Ok(None);
                }
                incremental = true;
                // GetProperty(delete=true) acknowledges the INCR header.
                self.conn.flush()?;
                deadline = Instant::now() + TIMEOUT;
                continue;
            }
            if reply.type_ != target || reply.format != 8 {
                return Ok(None);
            }
            let done = !incremental || reply.value.is_empty();
            data.extend_from_slice(&reply.value);
            if done {
                return Ok(Some(data));
            }
            self.conn.flush()?;
            deadline = Instant::now() + TIMEOUT;
        }
    }
}

fn new_window(conn: &RustConnection, root: Window) -> anyhow::Result<Window> {
    let window = conn.generate_id()?;
    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        window,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )?
    .check()?;
    Ok(window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an X11 server (DISPLAY=:40)"]
    fn x11_roundtrip() {
        let mut owner = X11Clipboard::new().unwrap();
        let mut reader = X11Clipboard::new().unwrap();
        for content in [
            ClipboardContent::Text("hello 🌍\0clipboard".into()),
            ClipboardContent::Png(vec![137, 80, 78, 71, 13, 10, 26, 10]),
            ClipboardContent::Text("large λ".repeat(150_000)),
            ClipboardContent::Png(vec![0x5a; MAX_CONTENT_BYTES]),
            ClipboardContent::Text(String::new()),
        ] {
            let previous = reader.change_count();
            owner.write(&content);
            let deadline = Instant::now() + TIMEOUT;
            while reader.change_count() == previous && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            assert_ne!(reader.change_count(), previous);
            assert!(reader.read() == Some(content));
        }
        // Swap owners to exercise SelectionClear and both change counters.
        let content = ClipboardContent::Text("back again".into());
        reader.write(&content);
        assert!(owner.read() == Some(content));
    }
}
