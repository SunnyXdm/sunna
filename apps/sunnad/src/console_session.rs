//! Windows: run the host as SYSTEM inside the session at the PC's console.
//!
//! A program the user starts can't see the lock screen, the sign-in screen or
//! UAC prompts: those are on the secure desktop, which only SYSTEM may open.
//! Session 0 (where services and SYSTEM tasks run) has no screen at all. So
//! this launcher, running as SYSTEM in session 0, starts `sunnad` again with
//! its own SYSTEM token moved into the console session, and keeps it there:
//! restarted if it exits, moved when the console changes hands (sign-out,
//! user switching, a Remote Desktop session taking over). Parsec and Sunshine
//! do the same. The Windows service wraps exactly this loop.

use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Context;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Security::{
    DuplicateTokenEx, SecurityIdentification, SetTokenInformation, TokenPrimary, TokenSessionId,
    SECURITY_ATTRIBUTES, TOKEN_ALL_ACCESS,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_ALWAYS,
};
use windows::Win32::System::RemoteDesktop::WTSGetActiveConsoleSessionId;
use windows::Win32::System::Threading::{
    CreateProcessAsUserW, GetCurrentProcess, GetExitCodeProcess, OpenProcessToken,
    TerminateProcess, WaitForSingleObject, CREATE_NO_WINDOW, PROCESS_INFORMATION,
    STARTF_USESTDHANDLES, STARTUPINFOW,
};

/// No session is attached to the console (between sign-out and sign-in).
const NO_SESSION: u32 = 0xFFFF_FFFF;

/// Run `sunnad` with `args` as SYSTEM in the console session until `stop`
/// is set (then it's stopped too). Its output is appended to
/// `sunnad-agent.log` next to the executable.
pub fn run(args: Vec<OsString>, stop: &AtomicBool) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let log = exe.with_file_name("sunnad-agent.log");
    let mut command = quote(exe.as_os_str());
    for arg in &args {
        command.push(' ');
        command.push_str(&quote(arg));
    }
    tracing::info!(log = %log.display(), "running the host in the console session");
    while !stop.load(Ordering::Relaxed) {
        // SAFETY: no arguments; returns a session id or NO_SESSION.
        let session = unsafe { WTSGetActiveConsoleSessionId() };
        if session == NO_SESSION {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        match start(&command, session, &log) {
            Ok(process) => {
                tracing::info!(session, "host started");
                let code = watch(process, session, stop);
                tracing::info!(session, ?code, "host stopped");
            }
            Err(error) => tracing::warn!(session, "{error:#}"),
        }
        // Don't spin if it keeps failing at once.
        std::thread::sleep(Duration::from_secs(2));
    }
    Ok(())
}

/// Wait while the process runs, the console stays with `session` and
/// nobody asked to stop. Returns its exit code, or None if it was stopped.
fn watch(process: HANDLE, session: u32, stop: &AtomicBool) -> Option<u32> {
    let code = loop {
        // SAFETY: our process handle; polls once a second.
        if unsafe { WaitForSingleObject(process, 1000) } == WAIT_OBJECT_0 {
            let mut code = 0;
            // SAFETY: the process has exited; reads its exit code.
            let _ = unsafe { GetExitCodeProcess(process, &mut code) };
            break Some(code);
        }
        // SAFETY: no arguments.
        if stop.load(Ordering::Relaxed) || unsafe { WTSGetActiveConsoleSessionId() } != session {
            // SAFETY: our child, which belongs to a session no longer on the console.
            let _ = unsafe { TerminateProcess(process, 1) };
            break None;
        }
    };
    // SAFETY: closing our own handle once.
    let _ = unsafe { CloseHandle(process) };
    code
}

fn start(command: &str, session: u32, log: &std::path::Path) -> anyhow::Result<HANDLE> {
    // SAFETY: token and process creation with locals; every handle opened
    // here is closed here except the returned process handle.
    unsafe {
        let mut own = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_ALL_ACCESS, &mut own)
            .context("opening this process's token")?;
        let mut token = HANDLE::default();
        let duplicated = DuplicateTokenEx(own, TOKEN_ALL_ACCESS, None, SecurityIdentification, TokenPrimary, &mut token);
        let _ = CloseHandle(own);
        duplicated.context("duplicating the token")?;
        let moved = SetTokenInformation(
            token,
            TokenSessionId,
            (&session as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        if let Err(error) = moved {
            let _ = CloseHandle(token);
            return Err(error).context("moving the token to the console session (needs SYSTEM)");
        }
        let inherit = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: true.into(),
        };
        let log_path = wide(log.as_os_str());
        let output = CreateFileW(
            PCWSTR(log_path.as_ptr()),
            FILE_APPEND_DATA.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            Some(&inherit),
            OPEN_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
        .context("opening the agent log")?;
        let mut desktop = wide("winsta0\\default".as_ref());
        let startup = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            lpDesktop: PWSTR(desktop.as_mut_ptr()),
            dwFlags: STARTF_USESTDHANDLES,
            hStdOutput: output,
            hStdError: output,
            ..Default::default()
        };
        let mut command_line = wide(command.as_ref());
        let mut process = PROCESS_INFORMATION::default();
        let created = CreateProcessAsUserW(
            Some(token),
            PCWSTR::null(),
            Some(PWSTR(command_line.as_mut_ptr())),
            None,
            None,
            true,
            CREATE_NO_WINDOW,
            None,
            PCWSTR::null(),
            &startup,
            &mut process,
        );
        let _ = CloseHandle(output);
        let _ = CloseHandle(token);
        created.context("starting the host in the console session")?;
        let _ = CloseHandle(process.hThread);
        Ok(process.hProcess)
    }
}

fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
    text.encode_wide().chain(Some(0)).collect()
}

/// Quote one argument the way the C runtime parses command lines.
pub fn quote(arg: &std::ffi::OsStr) -> String {
    let arg = arg.to_string_lossy();
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.into_owned();
    }
    let mut quoted = String::from('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                quoted.push_str(&"\\".repeat(backslashes));
                quoted.push(c);
                backslashes = 0;
            }
        }
    }
    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('"');
    quoted
}
