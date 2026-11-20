//! Clipboard data belongs to the worker's window station, including under SYSTEM.

use std::time::Duration;

use windows::core::w;
use windows::Win32::Foundation::{
    GetLastError, GlobalFree, ERROR_CLASS_ALREADY_EXISTS, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT,
    WPARAM,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::System::Ole::{CF_DIB, CF_DIBV5, CF_UNICODETEXT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, PeekMessageW, RegisterClassW,
    HWND_MESSAGE, MSG, PM_REMOVE, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW,
};

use crate::dib::{dib_to_png, png_to_dib};
use crate::windows_text::{to_crlf, to_lf};
use crate::{within_limit, Clipboard, ClipboardContent};

pub struct WindowsClipboard {
    window: HWND,
    png_format: u32,
    /// The sequence number last seen, and how many more times to read it
    /// again (see `RETRIES`).
    sequence: u32,
    retries: u32,
    /// The last read found nothing usable yet: read again on the next poll.
    retry: bool,
    /// Moves `change_count` on for each retry.
    bump: u64,
}

/// Reads of one clipboard change before giving up, one per poll (~2 s).
/// Copying apps often fill the clipboard in steps: OLE apps (Office, .NET,
/// browsers) first put a placeholder, then the real formats; and the
/// clipboard may be open in another app at the moment we look.
const RETRIES: u32 = 8;

// SAFETY: HWND is an opaque OS handle; the window stores no Rust pointers and uses
// only DefWindowProcW. All access is exclusive through &mut self. ClipboardSession
// creates, uses and drops this object on its worker. Moving it elsewhere cannot
// expose Rust memory, though DestroyWindow would fail off the creating thread.
unsafe impl Send for WindowsClipboard {}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: Forward the arguments supplied by Windows unchanged to its default
    // procedure. There is no delayed rendering or Rust state attached to the window.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

impl WindowsClipboard {
    pub fn new() -> anyhow::Result<Self> {
        // SAFETY: Class strings have static lifetime, the callback is the system
        // window procedure, and no pointers to Rust state are retained by Windows.
        unsafe {
            let instance = GetModuleHandleW(None)?.into();
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: w!("SunnaClipboardOwner"),
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err(windows::core::Error::from_thread().into());
            }
            let png_format = RegisterClipboardFormatW(w!("PNG"));
            anyhow::ensure!(png_format != 0, "couldn't register PNG clipboard format");
            let window = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class.lpszClassName,
                w!(""),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance),
                None,
            )?;
            Ok(Self {
                window,
                png_format,
                sequence: GetClipboardSequenceNumber(),
                retries: 0,
                retry: false,
                bump: 0,
            })
        }
    }

    fn pump_messages(&self) {
        // SAFETY: The window has only a system callback and no Rust state. Pumping
        // also delivers sent ownership messages from other clipboard users.
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, Some(self.window), 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&message);
            }
        }
    }
}

impl Drop for WindowsClipboard {
    fn drop(&mut self) {
        // SAFETY: This is our owned window, normally dropped on its worker thread.
        if let Err(error) = unsafe { DestroyWindow(self.window) } {
            tracing::debug!(%error, "couldn't destroy clipboard owner window");
        }

    }
}

struct OpenClipboardGuard;

impl OpenClipboardGuard {
    fn new(window: HWND) -> Option<Self> {
        for attempt in 0..10 {
            // SAFETY: The owner window is alive throughout this guard's lifetime.
            match unsafe { OpenClipboard(Some(window)) } {
                Ok(()) => return Some(Self),
                Err(error) if attempt == 9 => {
                    tracing::debug!(%error, "clipboard busy; skipping this poll");
                }
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        None
    }

    fn read(&self, format: u32) -> Option<LockedGlobal> {
        // SAFETY: The clipboard remains open; callers request only HGLOBAL formats.
        let handle = match unsafe { GetClipboardData(format) } {
            Ok(handle) => handle,
            Err(error) => {
                tracing::debug!(%error, format, "couldn't read clipboard format");
                return None;
            }
        };
        LockedGlobal::new(HGLOBAL(handle.0))
    }

    fn write(&self, format: u32, bytes: &[u8]) {
        // SAFETY: Allocate movable storage, copy within its bounds while locked,
        // then transfer ownership only if SetClipboardData succeeds.
        let result = unsafe {
            (|| -> windows::core::Result<()> {
                let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1))?;
                let pointer = GlobalLock(handle);
                if pointer.is_null() {
                    let error = windows::core::Error::from_thread();
                    let _ = GlobalFree(Some(handle));
                    return Err(error);
                }
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast(), bytes.len());
                let _ = GlobalUnlock(handle);
                if let Err(error) = SetClipboardData(format, Some(HANDLE(handle.0))) {
                    let _ = GlobalFree(Some(handle));
                    return Err(error);
                }
                Ok(())
            })()
        };
        if let Err(error) = result {
            tracing::debug!(%error, format, "clipboard write failed");
        }
    }
}

impl Drop for OpenClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: Constructed only after a successful OpenClipboard on this thread.
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

struct LockedGlobal {
    handle: HGLOBAL,
    pointer: *const u8,
    size: usize,
}

impl LockedGlobal {
    fn new(handle: HGLOBAL) -> Option<Self> {
        // SAFETY: The clipboard supplied this global handle and stays open while
        // it is locked. Check its allocation size before exposing or copying bytes.
        unsafe {
            let size = GlobalSize(handle);
            if size == 0 || !within_limit(size) {
                return None;
            }
            let pointer = GlobalLock(handle).cast::<u8>();
            if pointer.is_null() {
                return None;
            }
            Some(Self {
                handle,
                pointer,
                size,
            })
        }
    }

    fn bytes(&self) -> &[u8] {
        // SAFETY: GlobalLock keeps this allocation valid for the reported size;
        // callers drop this lock before closing the clipboard.
        unsafe { std::slice::from_raw_parts(self.pointer, self.size) }
    }
}

impl Drop for LockedGlobal {
    fn drop(&mut self) {
        // SAFETY: This object holds exactly one successful lock on this handle.
        unsafe {
            let _ = GlobalUnlock(self.handle);
        }
    }
}

impl Clipboard for WindowsClipboard {
    fn read(&mut self) -> Option<ClipboardContent> {
        self.pump_messages();
        // Until something is found: busy, or not filled in yet.
        self.retry = true;
        let clipboard = OpenClipboardGuard::new(self.window)?;
        let mut format = 0;
        let mut seen = Vec::new();
        loop {
            // SAFETY: The clipboard is open; zero starts or terminates enumeration.
            format = unsafe { EnumClipboardFormats(format) };
            if format == 0 {
                // SAFETY: reads this thread's last error, set by the enumeration.
                let error = unsafe { GetLastError() };
                tracing::debug!(?seen, ?error, "nothing shareable on the clipboard yet");
                return None;
            }
            seen.push(format);
            // Follow the copying application's order, including synthesized DIBs
            // after real formats, so Office text isn't replaced by its picture.
            if format != u32::from(CF_UNICODETEXT.0)
                && format != self.png_format
                && format != u32::from(CF_DIB.0)
                && format != u32::from(CF_DIBV5.0)
            {
                continue;
            }
            let Some(data) = clipboard.read(format) else {
                continue;
            };
            let content = if format == u32::from(CF_UNICODETEXT.0) {
                let units: Vec<_> = data
                    .bytes()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                    .take_while(|&unit| unit != 0)
                    .collect();
                Some(ClipboardContent::Text(to_lf(&String::from_utf16_lossy(
                    &units,
                ))))
            } else if format == self.png_format {
                Some(ClipboardContent::Png(data.bytes().to_vec()))
            } else {
                dib_to_png(data.bytes()).map(ClipboardContent::Png)
            };
            if let Some(content) = content {
                self.retry = false;
                if within_limit(content.bytes().len()) {
                    return Some(content);
                }
            }
        }
    }

    fn write(&mut self, content: &ClipboardContent) {
        if !within_limit(content.bytes().len()) {
            return;
        }
        // Convert before opening, keeping other applications' clipboard waits short.
        let (format, bytes, dib) = match content {
            ClipboardContent::Text(text) => {
                let bytes: Vec<_> = to_crlf(text)
                    .encode_utf16()
                    .chain(Some(0))
                    .flat_map(u16::to_le_bytes)
                    .collect();
                if !within_limit(bytes.len()) {
                    return;
                }
                (u32::from(CF_UNICODETEXT.0), bytes, None)
            }
            ClipboardContent::Png(bytes) => (self.png_format, bytes.clone(), png_to_dib(bytes)),
        };
        self.pump_messages();
        let Some(clipboard) = OpenClipboardGuard::new(self.window) else {
            return;
        };
        // SAFETY: OpenClipboard used our non-NULL owner, so SetClipboardData can
        // transfer ownership after EmptyClipboard even in the SYSTEM session.
        if let Err(error) = unsafe { EmptyClipboard() } {
            tracing::debug!(%error, "couldn't empty clipboard");
            return;
        }
        clipboard.write(format, &bytes);
        if let Some(dib) = dib {
            clipboard.write(u32::from(CF_DIB.0), &dib);
        }
    }

    fn change_count(&mut self) -> u64 {
        self.pump_messages();
        // SAFETY: This reads the window station's sequence number without pointers.
        let sequence = unsafe { GetClipboardSequenceNumber() };
        if sequence != self.sequence {
            (self.sequence, self.retries, self.retry) = (sequence, RETRIES, false);
        } else if std::mem::take(&mut self.retry) && self.retries > 0 {
            self.retries -= 1;
            self.bump += 1;
        }
        u64::from(sequence) << 32 | (self.bump & 0xffff_ffff)
    }
}
