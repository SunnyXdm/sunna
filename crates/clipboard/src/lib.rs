//! Platform clipboard access and the shared session sync state.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

use sunna_proto::messages::{ClipboardData, ClipboardKind};
use tokio::sync::watch;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
// Plain Rust (the app relays Android's clipboard), so it builds and tests
// anywhere; only Android uses it.
mod phone;

pub use phone::{copied_on_phone, remote_count, take_for_phone};

pub const MAX_CONTENT_BYTES: usize = 16 * 1024 * 1024;
pub const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum ClipboardContent {
    Text(String),
    Png(Vec<u8>),
}

impl ClipboardContent {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Text(text) => text.as_bytes(),
            Self::Png(data) => data,
        }
    }

    fn fingerprint(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    pub fn into_data(self) -> ClipboardData {
        match self {
            Self::Text(text) => ClipboardData {
                kind: ClipboardKind::Text,
                data: text.into_bytes(),
            },
            Self::Png(data) => ClipboardData {
                kind: ClipboardKind::Png,
                data,
            },
        }
    }

    pub fn from_data(data: ClipboardData) -> Option<Self> {
        if !within_limit(data.data.len()) {
            return None;
        }
        match data.kind {
            ClipboardKind::Text => String::from_utf8(data.data).ok().map(Self::Text),
            ClipboardKind::Png => Some(Self::Png(data.data)),
        }
    }
}

pub(crate) fn within_limit(size: usize) -> bool {
    if size > MAX_CONTENT_BYTES {
        tracing::debug!(size, "ignoring oversized clipboard content");
        false
    } else {
        true
    }
}

pub trait Clipboard: Send {
    fn read(&mut self) -> Option<ClipboardContent>;
    fn write(&mut self, content: &ClipboardContent);
    fn change_count(&mut self) -> u64;
}

pub fn system_clipboard() -> anyhow::Result<Box<dyn Clipboard>> {
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(linux::X11Clipboard::new()?))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::MacClipboard::new()?))
    }
    #[cfg(target_os = "android")]
    {
        Ok(Box::new(phone::PhoneClipboard))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "android")))]
    {
        anyhow::bail!("clipboard is unsupported on this platform")
    }
}

/// Deterministic state; scheduling and platform access belong to the caller.
pub struct ClipboardSync {
    count: u64,
    received: Option<u64>,
    observed: Option<u64>,
}

impl ClipboardSync {
    /// Start watching future copies without sending the pre-session clipboard.
    pub fn new(count: u64) -> Self {
        Self {
            count,
            received: None,
            observed: None,
        }
    }

    pub fn poll(&mut self, clipboard: &mut dyn Clipboard) -> Option<ClipboardContent> {
        let count = clipboard.change_count();
        if self.count == count {
            return None;
        }
        self.count = count;
        let Some(content) = clipboard.read() else {
            self.observed = None;
            return None;
        };
        if !within_limit(content.bytes().len()) {
            return None;
        }
        let hash = content.fingerprint();
        let duplicate = self.observed == Some(hash) || self.received == Some(hash);
        self.observed = Some(hash);
        (!duplicate).then_some(content)
    }

    pub fn receive(&mut self, clipboard: &mut dyn Clipboard, content: ClipboardContent) {
        if !within_limit(content.bytes().len()) {
            return;
        }
        let hash = content.fingerprint();
        self.received = Some(hash);
        self.observed = Some(hash);
        clipboard.write(&content);
    }
}

/// Dropping the session closes the channels and stops its worker.
/// Watch channels keep slow peers from accumulating clipboard images.
pub struct ClipboardSession {
    incoming: watch::Sender<Option<ClipboardData>>,
    outgoing: watch::Receiver<Option<ClipboardData>>,
}

impl ClipboardSession {
    pub fn start(enabled: bool) -> Self {
        let (incoming, mut commands) = watch::channel(None::<ClipboardData>);
        let (updates, outgoing) = watch::channel(None);
        if enabled {
            let spawn = std::thread::Builder::new()
                .name("clipboard-sync".into())
                .spawn(move || {
                    let mut clipboard = match system_clipboard() {
                        Ok(clipboard) => clipboard,
                        Err(error) => {
                            tracing::warn!(%error, "clipboard sharing unavailable");
                            return;
                        }
                    };
                    let mut sync = ClipboardSync::new(clipboard.change_count());
                    let mut next_poll = Instant::now() + POLL_INTERVAL;
                    loop {
                        if updates.is_closed() {
                            break;
                        }
                        match commands.has_changed() {
                            Err(_) => break,
                            Ok(true) => {
                                let data = commands.borrow_and_update().clone();
                                if let Some(data) = data {
                                    let (kind, size) = (data.kind, data.data.len());
                                    if let Some(content) = ClipboardContent::from_data(data) {
                                        sync.receive(clipboard.as_mut(), content);
                                        tracing::info!(?kind, size, "clipboard received");
                                    }
                                }
                            }
                            Ok(false) => {}
                        }
                        if Instant::now() >= next_poll {
                            if let Some(content) = sync.poll(clipboard.as_mut()) {
                                if updates.send(Some(content.into_data())).is_err() {
                                    break;
                                }
                            }
                            next_poll = Instant::now() + POLL_INTERVAL;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                });
            if let Err(error) = spawn {
                tracing::warn!(%error, "couldn't start clipboard sharing");
            }
        }
        Self { incoming, outgoing }
    }

    pub fn receive(&self, data: ClipboardData) {
        if within_limit(data.data.len()) {
            let _ = self.incoming.send(Some(data));
        }
    }

    pub async fn next(&mut self) -> ClipboardData {
        loop {
            if self.outgoing.changed().await.is_err() {
                return std::future::pending().await;
            }
            if let Some(data) = self.outgoing.borrow_and_update().clone() {
                return data;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeClipboard {
        content: Option<ClipboardContent>,
        count: u64,
        reads: usize,
    }
    impl Clipboard for FakeClipboard {
        fn read(&mut self) -> Option<ClipboardContent> {
            self.reads += 1;
            self.content.clone()
        }
        fn write(&mut self, content: &ClipboardContent) {
            self.content = Some(content.clone());
            self.count += 1;
        }
        fn change_count(&mut self) -> u64 {
            self.count
        }
    }
    fn text(s: &str) -> ClipboardContent {
        ClipboardContent::Text(s.into())
    }

    #[test]
    fn session_starts_with_future_copies() {
        let mut clipboard = FakeClipboard::default();
        clipboard.write(&text("before session"));
        let mut sync = ClipboardSync::new(clipboard.change_count());
        assert!(sync.poll(&mut clipboard).is_none());
        assert_eq!(clipboard.reads, 0);
        clipboard.write(&text("after session"));
        assert!(sync.poll(&mut clipboard) == Some(text("after session")));
    }

    #[test]
    fn suppresses_echo_and_duplicate_copies() {
        let mut clipboard = FakeClipboard::default();
        let mut sync = ClipboardSync::new(clipboard.change_count());
        assert!(sync.poll(&mut clipboard).is_none());
        assert_eq!(clipboard.reads, 0);
        clipboard.write(&text("local"));
        assert!(sync.poll(&mut clipboard) == Some(text("local")));
        clipboard.write(&text("local"));
        assert!(sync.poll(&mut clipboard).is_none());
        sync.receive(&mut clipboard, text("remote"));
        assert!(sync.poll(&mut clipboard).is_none());
        clipboard.write(&text("remote"));
        assert!(sync.poll(&mut clipboard).is_none());
        // A prior local copy is new again after the peer replaces it.
        clipboard.write(&text("local"));
        assert!(sync.poll(&mut clipboard) == Some(text("local")));
    }

    #[test]
    fn two_peers_settle_after_each_copy() {
        let (mut a, mut b) = (FakeClipboard::default(), FakeClipboard::default());
        let (mut sa, mut sb) = (ClipboardSync::new(0), ClipboardSync::new(0));
        for content in [
            text("one"),
            ClipboardContent::Png(vec![1, 2, 3]),
            text("three"),
        ] {
            a.write(&content);
            sb.receive(&mut b, sa.poll(&mut a).unwrap());
            assert!(sb.poll(&mut b).is_none());
            assert!(b.content == Some(content));
            std::mem::swap(&mut a, &mut b);
            std::mem::swap(&mut sa, &mut sb);
        }
    }

    #[test]
    fn validates_size_utf8_and_content_kind() {
        assert!(ClipboardContent::from_data(ClipboardData {
            kind: ClipboardKind::Text,
            data: vec![0xff],
        })
        .is_none());
        let mut clipboard = FakeClipboard::default();
        let mut sync = ClipboardSync::new(0);
        let large = ClipboardContent::Png(vec![0; MAX_CONTENT_BYTES + 1]);
        sync.receive(&mut clipboard, large.clone());
        assert_eq!(clipboard.count, 0);
        clipboard.write(&large);
        assert!(sync.poll(&mut clipboard).is_none());
        clipboard.write(&text("same"));
        assert!(sync.poll(&mut clipboard).is_some());
        clipboard.write(&ClipboardContent::Png(b"same".to_vec()));
        assert!(sync.poll(&mut clipboard).is_some());
        let boundary = ClipboardContent::Png(vec![0; MAX_CONTENT_BYTES]);
        assert!(ClipboardContent::from_data(boundary.into_data()).is_some());
    }
}
