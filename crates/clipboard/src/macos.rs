//! NSPasteboard is accessed inside per-call pools on the clipboard worker.

use std::ffi::c_void;

use objc2::rc::{autoreleasepool, Allocated, Retained};
use objc2::runtime::{AnyObject, Bool};
use objc2::{class, msg_send};

use crate::{within_limit, Clipboard, ClipboardContent};

#[link(name = "AppKit", kind = "framework")]
extern "C" {}

// No Objective-C objects cross threads or outlive an autorelease pool.
pub struct MacClipboard;

impl MacClipboard {
    pub fn new() -> anyhow::Result<Self> {
        let available = autoreleasepool(|_| unsafe {
            let board: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
            !board.is_null()
        });
        anyhow::ensure!(available, "general pasteboard unavailable");
        Ok(Self)
    }
}

fn ns_string(text: &str) -> Retained<AnyObject> {
    unsafe {
        let string: Allocated<AnyObject> = msg_send![class!(NSString), alloc];
        msg_send![string, initWithBytes: text.as_ptr().cast::<c_void>(),
            length: text.len(), encoding: 4usize]
    }
}

unsafe fn data_bytes(data: *mut AnyObject) -> Option<Vec<u8>> {
    if data.is_null() {
        return None;
    }
    let length: usize = msg_send![data, length];
    if !within_limit(length) {
        return None;
    }
    if length == 0 {
        return Some(Vec::new());
    }
    let bytes: *const u8 = msg_send![data, bytes];
    if bytes.is_null() {
        return None;
    }
    Some(std::slice::from_raw_parts(bytes, length).to_vec())
}

#[derive(Clone, Copy)]
enum Kind {
    Png,
    Tiff,
    Text,
}

/// The first type we can carry, in the pasteboard's own order.
unsafe fn preferred_kind(board: *mut AnyObject) -> Option<Kind> {
    let types: *mut AnyObject = msg_send![board, types];
    if types.is_null() {
        return None;
    }
    let candidates = [
        (ns_string("public.png"), Kind::Png),
        (ns_string("public.tiff"), Kind::Tiff),
        (ns_string("public.utf8-plain-text"), Kind::Text),
    ];
    let count: usize = msg_send![types, count];
    for index in 0..count {
        let name: *mut AnyObject = msg_send![types, objectAtIndex: index];
        for (candidate, kind) in &candidates {
            let same: Bool = msg_send![name, isEqualToString: &**candidate];
            if same.as_bool() {
                return Some(*kind);
            }
        }
    }
    None
}

impl Clipboard for MacClipboard {
    fn read(&mut self) -> Option<ClipboardContent> {
        autoreleasepool(|_| unsafe {
            let board: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
            if board.is_null() {
                return None;
            }
            // The copying app lists its types best first. Follow that order:
            // Finder puts a file's icon next to its name, and Office and
            // Keynote add a picture of copied text, so "image if any" would
            // paste pictures where text was meant.
            match preferred_kind(board) {
                Some(Kind::Png) => {
                    let png = ns_string("public.png");
                    let data: *mut AnyObject = msg_send![board, dataForType: &*png];
                    return data_bytes(data).map(ClipboardContent::Png);
                }
                Some(Kind::Tiff) => {
                    let tiff = ns_string("public.tiff");
                    let data: *mut AnyObject = msg_send![board, dataForType: &*tiff];
                    if data.is_null() {
                        return None;
                    }
                    let length: usize = msg_send![data, length];
                    if !within_limit(length) {
                        return None;
                    }
                    let bitmap: *mut AnyObject =
                        msg_send![class!(NSBitmapImageRep), imageRepWithData: data];
                    if bitmap.is_null() {
                        return None;
                    }
                    let properties: *mut AnyObject = msg_send![class!(NSDictionary), dictionary];
                    // NSBitmapImageFileTypePNG = 4.
                    let png: *mut AnyObject = msg_send![bitmap, representationUsingType: 4usize,
                        properties: properties];
                    return data_bytes(png).map(ClipboardContent::Png);
                }
                Some(Kind::Text) => {}
                None => return None,
            }
            let text = ns_string("public.utf8-plain-text");
            let string: *mut AnyObject = msg_send![board, stringForType: &*text];
            if string.is_null() {
                return None;
            }
            let data: *mut AnyObject = msg_send![string, dataUsingEncoding: 4usize];
            String::from_utf8(data_bytes(data)?)
                .ok()
                .map(ClipboardContent::Text)
        })
    }

    fn write(&mut self, content: &ClipboardContent) {
        if !within_limit(content.bytes().len()) {
            return;
        }
        autoreleasepool(|_| unsafe {
            let board: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
            if board.is_null() {
                return;
            }
            let _: isize = msg_send![board, clearContents];
            let written: Bool = match content {
                ClipboardContent::Text(text) => {
                    let kind = ns_string("public.utf8-plain-text");
                    let string = ns_string(text);
                    msg_send![board, setString: &*string, forType: &*kind]
                }
                ClipboardContent::Png(bytes) => {
                    let kind = ns_string("public.png");
                    let data: *mut AnyObject = msg_send![class!(NSData),
                        dataWithBytes: bytes.as_ptr().cast::<c_void>(), length: bytes.len()];
                    msg_send![board, setData: data, forType: &*kind]
                }
            };
            if !written.as_bool() {
                tracing::debug!("pasteboard write failed");
            }
        })
    }

    fn change_count(&mut self) -> u64 {
        autoreleasepool(|_| unsafe {
            let board: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
            if board.is_null() {
                return 0;
            }
            let count: isize = msg_send![board, changeCount];
            count as u64
        })
    }
}
