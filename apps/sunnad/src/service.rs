//! Windows: `sunnad` as a service, so the host is there from boot, at the
//! sign-in screen, and after the user signs out. The service (SYSTEM, in
//! session 0) runs the console-session launcher, which runs the host itself
//! in the session at the PC's screen. `--install-service` and
//! `--uninstall-service` register and remove it (the installer will do this
//! in a release).

use std::ffi::OsString;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::OnceLock;

use anyhow::Context;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_CALL_NOT_IMPLEMENTED, NO_ERROR};
use windows::Win32::System::Services::*;

const NAME: &str = "SunnaHost";
const DISPLAY_NAME: &str = "Sunna host";

static ARGS: OnceLock<Vec<OsString>> = OnceLock::new();
static STOP: AtomicBool = AtomicBool::new(false);
/// The SERVICE_STATUS_HANDLE (a pointer-sized value), 0 until registered.
static STATUS: AtomicUsize = AtomicUsize::new(0);

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Called by the Service Control Manager (`sunnad --service ...`): runs
/// until the service is stopped. `args` go to the host.
pub fn run(args: Vec<OsString>) -> anyhow::Result<()> {
    let _ = ARGS.set(args);
    let mut name = wide(NAME);
    let table = [
        SERVICE_TABLE_ENTRYW { lpServiceName: PWSTR(name.as_mut_ptr()), lpServiceProc: Some(service_main) },
        SERVICE_TABLE_ENTRYW::default(),
    ];
    // SAFETY: the table and its name outlive the dispatcher, which returns
    // when the service has stopped.
    unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) }
        .context("not started by the Service Control Manager (use --install-service)")
}

fn set_state(state: SERVICE_STATUS_CURRENT_STATE) {
    let handle = STATUS.load(Ordering::Relaxed);
    if handle == 0 {
        return;
    }
    let handle = SERVICE_STATUS_HANDLE(handle as *mut core::ffi::c_void);
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if state == SERVICE_RUNNING { SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN } else { 0 },
        dwWin32ExitCode: NO_ERROR.0,
        dwWaitHint: 5000,
        ..Default::default()
    };
    // SAFETY: our registered status handle.
    let _ = unsafe { SetServiceStatus(handle, &status) };
}

unsafe extern "system" fn service_main(_argc: u32, _argv: *mut PWSTR) {
    let name = wide(NAME);
    // SAFETY: registers our handler for this service name.
    match unsafe { RegisterServiceCtrlHandlerExW(PCWSTR(name.as_ptr()), Some(handler), None) } {
        Ok(handle) => STATUS.store(handle.0 as usize, Ordering::Relaxed),
        Err(error) => {
            tracing::error!(%error, "couldn't register the service handler");
            return;
        }
    }
    if std::env::var_os("SUNNA_REQUIRE_PROOF").is_none() {
        std::env::set_var("SUNNA_REQUIRE_PROOF", "1");
    }
    set_state(SERVICE_RUNNING);
    let mut args = ARGS.get().cloned().unwrap_or_default();
    args.push("--service-key-store".into());
    if let Err(error) = crate::console_session::run(args, &STOP) {
        tracing::error!("{error:#}");
    }
    set_state(SERVICE_STOPPED);
}

unsafe extern "system" fn handler(control: u32, _event: u32, _data: *mut core::ffi::c_void, _context: *mut core::ffi::c_void) -> u32 {
    match control {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            set_state(SERVICE_STOP_PENDING);
            STOP.store(true, Ordering::Relaxed);
            NO_ERROR.0
        }
        SERVICE_CONTROL_INTERROGATE => NO_ERROR.0,
        _ => ERROR_CALL_NOT_IMPLEMENTED.0,
    }
}

/// Register the service to start with Windows, running this executable with
/// `args` for the host, and start it.
pub fn install(args: &[OsString]) -> anyhow::Result<()> {
    crate::service_args::validate_service_args(args).map_err(anyhow::Error::msg)?;
    let exe = std::env::current_exe()?;
    let mut command = format!("\"{}\" --service", exe.display());
    for arg in args {
        command.push(' ');
        command.push_str(&crate::console_session::quote(arg));
    }
    let (name, display, binary) = (wide(NAME), wide(DISPLAY_NAME), wide(&command));
    // SAFETY: SCM calls with locals; handles closed before returning.
    unsafe {
        let manager = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CREATE_SERVICE)
            .context("opening the Service Control Manager (run as administrator)")?;
        let service = CreateServiceW(
            manager,
            PCWSTR(name.as_ptr()),
            PCWSTR(display.as_ptr()),
            SERVICE_ALL_ACCESS,
            SERVICE_WIN32_OWN_PROCESS,
            SERVICE_AUTO_START,
            SERVICE_ERROR_NORMAL,
            PCWSTR(binary.as_ptr()),
            PCWSTR::null(),
            None,
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
        );
        let _ = CloseServiceHandle(manager);
        let service = service.context("creating the service")?;
        let started = StartServiceW(service, None);
        let _ = CloseServiceHandle(service);
        started.context("starting the service")?;
    }
    println!("installed and started {NAME}");
    Ok(())
}

/// Stop and remove the service.
pub fn uninstall() -> anyhow::Result<()> {
    let name = wide(NAME);
    // SAFETY: SCM calls with locals; handles closed before returning.
    unsafe {
        let manager = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT)
            .context("opening the Service Control Manager (run as administrator)")?;
        let service = OpenServiceW(manager, PCWSTR(name.as_ptr()), SERVICE_STOP | SERVICE_QUERY_STATUS | 0x0001_0000);
        let _ = CloseServiceHandle(manager);
        let service = service.context("opening the service (is it installed?)")?;
        let mut status = SERVICE_STATUS::default();
        let _ = ControlService(service, SERVICE_CONTROL_STOP, &mut status);
        let deleted = DeleteService(service);
        let _ = CloseServiceHandle(service);
        deleted.context("removing the service")?;
    }
    println!("removed {NAME}");
    Ok(())
}
