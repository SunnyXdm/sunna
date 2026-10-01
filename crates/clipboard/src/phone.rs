//! The phone's clipboard (Android). Android's clipboard is a Java service
//! that only the app in front may read, so the app relays it: it tells
//! Sunna what the phone copied ([`copied_on_phone`]) and puts what the
//! remote copied onto the phone's clipboard ([`take_for_phone`]).

use std::sync::Mutex;

use crate::{Clipboard, ClipboardContent};

struct Shared {
    /// What the phone's clipboard holds, as the app last said, and a count
    /// that moves on with every copy.
    phone: Option<ClipboardContent>,
    count: u64,
    /// What the remote copied, waiting for the app, and a count the app
    /// watches.
    remote: Option<ClipboardContent>,
    remote_count: u64,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared { phone: None, count: 0, remote: None, remote_count: 0 });

fn shared() -> std::sync::MutexGuard<'static, Shared> {
    SHARED.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The phone's clipboard has something new.
pub fn copied_on_phone(content: ClipboardContent) {
    let mut shared = shared();
    if shared.phone.as_ref() != Some(&content) {
        shared.phone = Some(content);
        shared.count += 1;
    }
}

/// Moves on whenever the remote has copied something for the phone.
pub fn remote_count() -> u64 {
    shared().remote_count
}

/// What the remote copied last, once.
pub fn take_for_phone() -> Option<ClipboardContent> {
    shared().remote.take()
}

/// The session's view of the phone's clipboard.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) struct PhoneClipboard;

impl Clipboard for PhoneClipboard {
    fn read(&mut self) -> Option<ClipboardContent> {
        shared().phone.clone()
    }

    fn write(&mut self, content: &ClipboardContent) {
        let mut shared = shared();
        shared.remote = Some(content.clone());
        shared.remote_count += 1;
        // The app puts it on the phone's clipboard, so a copy of what was
        // there before is something new again.
        shared.phone = Some(content.clone());
    }

    fn change_count(&mut self) -> u64 {
        shared().count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ClipboardSync;

    #[test]
    fn copies_go_out_and_echoes_dont() {
        let mut clipboard = PhoneClipboard;
        let mut sync = ClipboardSync::new(clipboard.change_count());
        copied_on_phone(ClipboardContent::Text("from the phone".into()));
        assert!(sync.poll(&mut clipboard) == Some(ClipboardContent::Text("from the phone".into())));
        // The remote copies; the app puts it on the phone's clipboard, which
        // tells us about it: not sent back.
        let before = remote_count();
        sync.receive(&mut clipboard, ClipboardContent::Text("from the host".into()));
        assert_eq!(remote_count(), before + 1);
        let landed = take_for_phone().expect("waiting for the phone");
        copied_on_phone(landed);
        assert!(sync.poll(&mut clipboard).is_none());
        assert!(take_for_phone().is_none());
        // Copying the first text again on the phone sends it again.
        copied_on_phone(ClipboardContent::Text("from the phone".into()));
        assert!(sync.poll(&mut clipboard) == Some(ClipboardContent::Text("from the phone".into())));
    }
}
