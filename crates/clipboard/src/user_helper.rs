//! Windows: the clipboard through a helper running as the signed-in user.
//!
//! The host runs as SYSTEM in the console session so it can show the lock
//! screen, and Windows shows a SYSTEM process only a placeholder of what the
//! user copied (one private format, whatever the user's app put there);
//! acting as the user on the thread doesn't change that. Writing works, but
//! copies on the PC would never reach the viewer. So the host starts its own
//! executable again as the user (`HELPER_FLAG`), which does the clipboard
//! work and answers over its standard input and output, as Parsec does with
//! a process of its own in the user's session. The helper exits when the
//! host closes the pipes (or dies).
//!
//! Requests and replies are `[tag][u32 length][payload]`: `C` → the change
//! count (u64), `R` → nothing or `[kind][bytes]`, `W [kind][bytes]` → empty.

use std::ffi::c_void;
use std::fs::File;
use std::io::{Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::FromRawHandle;
use std::time::Duration;

use anyhow::{bail, Context};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, DuplicateHandle, SetHandleInformation, DUPLICATE_SAME_ACCESS, HANDLE,
    HANDLE_FLAGS, HANDLE_FLAG_INHERIT, WAIT_OBJECT_0,
};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::RemoteDesktop::{ProcessIdToSessionId, WTSQueryUserToken};
use windows::Win32::System::Threading::{
    CreateProcessAsUserW, DeleteProcThreadAttributeList, GetCurrentProcess, GetCurrentProcessId,
    InitializeProcThreadAttributeList, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, CREATE_NO_WINDOW, EXTENDED_STARTUPINFO_PRESENT,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
    STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW,
};

use crate::windows::WindowsClipboard;
use crate::{within_limit, Clipboard, ClipboardContent, MAX_CONTENT_BYTES};

/// The argument that makes an executable serve as the clipboard helper: it
/// must then call [`serve`]. `sunnad` does.
pub const HELPER_FLAG: &str = "--clipboard-helper";

/// In the helper: answer the host's requests until it closes the pipe.
pub fn serve() -> anyhow::Result<()> {
    let mut clipboard = WindowsClipboard::new()?;
    let (mut input, mut output) = (std::io::stdin().lock(), std::io::stdout().lock());
    loop {
        let (tag, payload) = match receive(&mut input) {
            Ok(message) => message,
            // The host is gone.
            Err(_) => return Ok(()),
        };
        let reply = match tag {
            b'C' => clipboard.change_count().to_le_bytes().to_vec(),
            b'R' => clipboard.read().map(encode).unwrap_or_default(),
            b'W' => {
                if let Some(content) = decode(&payload) {
                    clipboard.write(&content);
                }
                Vec::new()
            }
            other => bail!("unknown clipboard request {other}"),
        };
        send(&mut output, tag, &reply)?;
    }
}

fn send(to: &mut impl Write, tag: u8, payload: &[u8]) -> std::io::Result<()> {
    to.write_all(&[tag])?;
    to.write_all(&(payload.len() as u32).to_le_bytes())?;
    to.write_all(payload)?;
    to.flush()
}

fn receive(from: &mut impl Read) -> anyhow::Result<(u8, Vec<u8>)> {
    let mut header = [0u8; 5];
    from.read_exact(&mut header)?;
    let size = u32::from_le_bytes(header[1..].try_into().unwrap()) as usize;
    anyhow::ensure!(size <= MAX_CONTENT_BYTES + 1, "clipboard message of {size} bytes");
    let mut payload = vec![0; size];
    from.read_exact(&mut payload)?;
    Ok((header[0], payload))
}

fn encode(content: ClipboardContent) -> Vec<u8> {
    let (kind, bytes) = match content {
        ClipboardContent::Text(text) => (0, text.into_bytes()),
        ClipboardContent::Png(png) => (1, png),
    };
    let mut message = Vec::with_capacity(bytes.len() + 1);
    message.push(kind);
    message.extend_from_slice(&bytes);
    message
}

fn decode(message: &[u8]) -> Option<ClipboardContent> {
    let (&kind, bytes) = message.split_first()?;
    if !within_limit(bytes.len()) {
        return None;
    }
    match kind {
        0 => String::from_utf8(bytes.to_vec()).ok().map(ClipboardContent::Text),
        1 => Some(ClipboardContent::Png(bytes.to_vec())),
        _ => None,
    }
}

/// The host's side: a helper process and the pipes to it.
pub(crate) struct HelperClipboard {
    process: HANDLE,
    /// Our ends of its standard input and output.
    pipes: Option<(File, File)>,
    /// The last change count, kept when the helper has gone.
    count: u64,
    failed: bool,
}

// SAFETY: the process handle and the files are plain OS handles, used only
// through &mut self.
unsafe impl Send for HelperClipboard {}

impl HelperClipboard {
    /// Start the helper as the signed-in user of this process's session.
    /// `Ok(None)` when it isn't needed or possible: not SYSTEM (we already
    /// are the user), or nobody is signed in (the sign-in screen).
    pub(crate) fn start() -> anyhow::Result<Option<Self>> {
        // SAFETY: Windows calls with locals; every handle made here is
        // either closed here or owned by the returned value.
        unsafe {
            let mut session = 0;
            ProcessIdToSessionId(GetCurrentProcessId(), &mut session)?;
            let mut token = HANDLE::default();
            // Needs SYSTEM's privilege to act for others, and a user.
            if WTSQueryUserToken(session, &mut token).is_err() {
                return Ok(None);
            }
            let started = spawn(token);
            let _ = CloseHandle(token);
            let (process, to, from) = started?;
            tracing::info!(session, "clipboard: helper started as the signed-in user");
            let mut helper = Self {
                process,
                pipes: Some((File::from_raw_handle(to.0), File::from_raw_handle(from.0))),
                count: 0,
                failed: false,
            };
            helper.count = helper.request(b'C', &[]).map(|reply| count_of(&reply)).context("the clipboard helper didn't answer")?;
            Ok(Some(helper))
        }
    }

    fn request(&mut self, tag: u8, payload: &[u8]) -> anyhow::Result<Vec<u8>> {
        anyhow::ensure!(!self.failed, "the clipboard helper has stopped");
        let (to, from) = self.pipes.as_mut().context("no clipboard helper")?;
        let reply = send(to, tag, payload)
            .map_err(anyhow::Error::from)
            .and_then(|()| receive(from))
            .and_then(|(echo, reply)| {
                anyhow::ensure!(echo == tag, "the clipboard helper answered out of turn");
                Ok(reply)
            });
        if let Err(error) = &reply {
            tracing::warn!(%error, "clipboard helper failed; no more clipboard sharing this session");
            self.failed = true;
        }
        reply
    }
}

fn count_of(reply: &[u8]) -> u64 {
    reply.try_into().map(u64::from_le_bytes).unwrap_or(0)
}

impl Clipboard for HelperClipboard {
    fn read(&mut self) -> Option<ClipboardContent> {
        decode(&self.request(b'R', &[]).ok()?)
    }

    fn write(&mut self, content: &ClipboardContent) {
        let _ = self.request(b'W', &encode(content.clone()));
    }

    fn change_count(&mut self) -> u64 {
        if let Ok(reply) = self.request(b'C', &[]) {
            self.count = count_of(&reply);
        }
        self.count
    }
}

impl Drop for HelperClipboard {
    fn drop(&mut self) {
        // Closing our ends of the pipes tells the helper to exit; make sure.
        drop(self.pipes.take());
        // SAFETY: our process handle, closed once.
        unsafe {
            if WaitForSingleObject(self.process, Duration::from_secs(2).as_millis() as u32) != WAIT_OBJECT_0 {
                let _ = TerminateProcess(self.process, 1);
            }
            let _ = CloseHandle(self.process);
        }
    }
}

/// Start this executable with `HELPER_FLAG` as `token`'s user, on the
/// interactive desktop, with only its two pipes and our standard error
/// (the log) handed to it: it mustn't inherit any other handle of ours, which
/// are SYSTEM's. Returns the process and our ends of its input and output.
///
/// # Safety
/// `token` must be a primary token of the user.
unsafe fn spawn(token: HANDLE) -> anyhow::Result<(HANDLE, HANDLE, HANDLE)> {
    unsafe {
        let inherit = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: true.into(),
        };
        let (mut child_in, mut to_child) = (HANDLE::default(), HANDLE::default());
        CreatePipe(&mut child_in, &mut to_child, Some(&inherit), 0).context("CreatePipe")?;
        let (mut from_child, mut child_out) = (HANDLE::default(), HANDLE::default());
        if let Err(error) = CreatePipe(&mut from_child, &mut child_out, Some(&inherit), 0) {
            close(&[child_in, to_child]);
            return Err(error).context("CreatePipe");
        }
        // Our ends stay ours.
        let _ = SetHandleInformation(to_child, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0));
        let _ = SetHandleInformation(from_child, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0));
        // The helper logs to our standard error, if we have one.
        let mut log = HANDLE::default();
        let have_log = GetStdHandle(STD_ERROR_HANDLE).is_ok_and(|stderr| {
            !stderr.is_invalid()
                && DuplicateHandle(GetCurrentProcess(), stderr, GetCurrentProcess(), &mut log, 0, true, DUPLICATE_SAME_ACCESS)
                    .is_ok()
        });
        let mut handles = vec![child_in, child_out];
        if have_log {
            handles.push(log);
        }
        let result = create(token, &handles, if have_log { log } else { child_out });
        close(&handles);
        match result {
            Ok(process) => Ok((process, to_child, from_child)),
            Err(error) => {
                close(&[to_child, from_child]);
                Err(error)
            }
        }
    }
}

unsafe fn close(handles: &[HANDLE]) {
    for &handle in handles {
        // SAFETY: handles this module created, each closed once.
        let _ = unsafe { CloseHandle(handle) };
    }
}

/// CreateProcessAsUserW with inheritance limited to `handles` (stdin,
/// stdout, then stderr if present).
unsafe fn create(token: HANDLE, handles: &[HANDLE], stderr: HANDLE) -> anyhow::Result<HANDLE> {
    unsafe {
        let mut size = 0;
        let _ = InitializeProcThreadAttributeList(None, 1, None, &mut size);
        let mut storage = vec![0u64; size.div_ceil(8)];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr().cast());
        InitializeProcThreadAttributeList(Some(list), 1, None, &mut size).context("InitializeProcThreadAttributeList")?;
        let created = (|| -> anyhow::Result<HANDLE> {
            UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                Some(handles.as_ptr().cast::<c_void>()),
                std::mem::size_of_val(handles),
                None,
                None,
            )
            .context("UpdateProcThreadAttribute")?;
            let mut desktop: Vec<u16> = "winsta0\\default".encode_utf16().chain(Some(0)).collect();
            let info = STARTUPINFOEXW {
                StartupInfo: STARTUPINFOW {
                    cb: std::mem::size_of::<STARTUPINFOEXW>() as u32,
                    lpDesktop: PWSTR(desktop.as_mut_ptr()),
                    dwFlags: STARTF_USESTDHANDLES,
                    hStdInput: handles[0],
                    hStdOutput: handles[1],
                    hStdError: stderr,
                    ..Default::default()
                },
                lpAttributeList: list,
            };
            let exe = std::env::current_exe()?;
            let mut command: Vec<u16> = format!("\"{}\" {HELPER_FLAG}", exe.display())
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let application: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
            let mut process = PROCESS_INFORMATION::default();
            CreateProcessAsUserW(
                Some(token),
                PCWSTR(application.as_ptr()),
                Some(PWSTR(command.as_mut_ptr())),
                None,
                None,
                true,
                CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
                None,
                PCWSTR::null(),
                &info.StartupInfo,
                &mut process,
            )
            .context("starting the clipboard helper as the signed-in user")?;
            let _ = CloseHandle(process.hThread);
            Ok(process.hProcess)
        })();
        DeleteProcThreadAttributeList(list);
        created
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        let mut wire = Vec::new();
        send(&mut wire, b'W', &encode(ClipboardContent::Text("héllo".into()))).unwrap();
        let (tag, payload) = receive(&mut wire.as_slice()).unwrap();
        assert_eq!(tag, b'W');
        assert!(decode(&payload) == Some(ClipboardContent::Text("héllo".into())));
        assert!(decode(&encode(ClipboardContent::Png(vec![1, 2]))) == Some(ClipboardContent::Png(vec![1, 2])));
        assert!(decode(&[]).is_none() && decode(&[7, 1]).is_none());
    }
}
