//! Sunna's own virtual display on Windows (`drivers/windows/sunna-display`).
//!
//! A PC with no monitor has nothing Windows draws, so nothing to capture.
//! This adds a display made for the viewer: its exact size and refresh rate,
//! drawn at full rate. The display adapter is a software device that exists
//! while this process does (Windows removes it when we exit or crash), so
//! nothing is left behind; monitors on it come and go by request.

use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, ensure, Context};
use windows::core::{GUID, HRESULT, PCWSTR};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_Device_Interface_ListW, CM_Get_Device_Interface_List_SizeW,
    CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CR_SUCCESS,
};
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    DISPLAYCONFIG_TARGET_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Devices::Enumeration::Pnp::{
    SwDeviceClose, SwDeviceCreate, HSWDEVICE, SWDeviceCapabilitiesDriverRequired,
    SWDeviceCapabilitiesRemovable, SWDeviceCapabilitiesSilentInstall, SW_DEVICE_CREATE_INFO,
};
use windows::Win32::Foundation::{CloseHandle, ERROR_NOT_READY, ERROR_SUCCESS, GENERIC_READ, GENERIC_WRITE, HANDLE};
use windows::Win32::Graphics::Gdi::{
    ChangeDisplaySettingsExW, CDS_UPDATEREGISTRY, DEVMODEW, DISP_CHANGE_SUCCESSFUL,
    DM_DISPLAYFREQUENCY, DM_PELSHEIGHT, DM_PELSWIDTH,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};
use windows::Win32::System::IO::DeviceIoControl;

/// The driver's control interface and requests (`include/sunna_display.h`).
const INTERFACE: GUID = GUID::from_u128(0x27e046a1_9ba8_4479_963f_cd0f642788c1);
const fn ioctl(function: u32) -> u32 {
    // CTL_CODE(FILE_DEVICE_UNKNOWN, function, METHOD_BUFFERED, FILE_WRITE_ACCESS)
    (0x22 << 16) | (0x2 << 14) | (function << 2)
}
const IOCTL_INFO: u32 = ioctl(0x800);
const IOCTL_ADD: u32 = ioctl(0x801);
const IOCTL_REMOVE: u32 = ioctl(0x802);
const PROTOCOL: u32 = 1;
/// What the driver calls its monitors (EDID name), to find ours.
const MONITOR_NAME: &str = "Sunna Display";

/// The software device (the virtual display adapter) and an open handle to
/// its driver. Shared by every display this process adds.
struct Adapter {
    device: HSWDEVICE,
    control: HANDLE,
}

// SAFETY: plain OS handles; requests on `control` are independent calls.
unsafe impl Send for Adapter {}
unsafe impl Sync for Adapter {}

impl Drop for Adapter {
    fn drop(&mut self) {
        // SAFETY: our handles, closed once; closing the device removes it.
        unsafe {
            let _ = CloseHandle(self.control);
            SwDeviceClose(self.device);
        }
    }
}

/// One of Sunna's virtual displays; removed when dropped.
pub struct VirtualDisplay {
    adapter: Arc<Adapter>,
    id: u32,
    /// Its GDI name (`\\.\DISPLAY7`), what capture looks for.
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
}

impl std::fmt::Debug for VirtualDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}x{}@{}", self.name, self.width, self.height, self.refresh_hz)
    }
}

impl VirtualDisplay {
    /// Add a display of exactly this mode and wait until Windows shows it.
    pub fn create(width: u32, height: u32, refresh_hz: u32) -> anyhow::Result<Self> {
        let adapter = adapter()?;
        let before = sunna_displays();
        let request = [PROTOCOL, width & !1, height & !1, refresh_hz];
        let mut id = [0u32; 1];
        let started = Instant::now();
        // The adapter answers once Windows has started it.
        loop {
            match control(&adapter, IOCTL_ADD, &request, &mut id) {
                Ok(()) => break,
                Err(error) if is_not_ready(&error) && started.elapsed() < Duration::from_secs(10) => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(error) => return Err(error).context("adding a Sunna display"),
            }
        }
        let mut added = Self {
            adapter,
            id: id[0],
            name: String::new(),
            width: width & !1,
            height: height & !1,
            refresh_hz,
        };
        // Its GDI name appears once Windows has set the display up.
        let deadline = Instant::now() + Duration::from_secs(10);
        added.name = loop {
            if let Some(name) = sunna_displays().into_iter().find(|name| !before.contains(name)) {
                break name;
            }
            ensure!(Instant::now() < deadline, "the Sunna display didn't appear in Windows");
            std::thread::sleep(Duration::from_millis(50));
        };
        // Windows may restore a mode it remembers for this monitor: ask for ours.
        if let Err(error) = set_mode(&added.name, added.width, added.height, refresh_hz) {
            tracing::warn!(display = %added.name, "{error:#}");
        }
        tracing::info!(display = ?added, after_ms = started.elapsed().as_millis() as u64, "Sunna display added");
        Ok(added)
    }
}

impl Drop for VirtualDisplay {
    fn drop(&mut self) {
        if let Err(error) = control(&self.adapter, IOCTL_REMOVE, &[self.id], &mut []) {
            tracing::warn!(id = self.id, "removing the Sunna display: {error:#}");
        }
    }
}

/// Whether Sunna's display driver is installed and answers. Starts the
/// adapter, which stays while this process runs.
pub fn available() -> bool {
    adapter().is_ok()
}

fn is_not_ready(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<windows::core::Error>()
        .is_some_and(|error| error.code() == ERROR_NOT_READY.to_hresult())
}

/// The process's adapter, started on first use and kept for the life of
/// the process: starting it again for each session would make Windows set
/// the device up again each time.
fn adapter() -> anyhow::Result<Arc<Adapter>> {
    static ADAPTER: Mutex<Option<Arc<Adapter>>> = Mutex::new(None);
    let mut slot = ADAPTER.lock().unwrap();
    if let Some(adapter) = slot.as_ref() {
        return Ok(Arc::clone(adapter));
    }
    let adapter = Arc::new(start_adapter()?);
    *slot = Some(Arc::clone(&adapter));
    Ok(adapter)
}

fn start_adapter() -> anyhow::Result<Adapter> {
    struct Created {
        event: HANDLE,
        result: Mutex<Option<(HRESULT, String)>>,
    }
    unsafe extern "system" fn on_created(
        _device: HSWDEVICE,
        result: HRESULT,
        context: *const c_void,
        instance: PCWSTR,
    ) {
        // SAFETY: `context` is the `Created` below, alive until the event fires.
        unsafe {
            let created = &*(context as *const Created);
            let instance = if instance.is_null() { String::new() } else { instance.to_string().unwrap_or_default() };
            *created.result.lock().unwrap() = Some((result, instance));
            let _ = SetEvent(created.event);
        }
    }
    let hardware_ids: Vec<u16> = "SunnaDisplay\0\0".encode_utf16().collect();
    let description: Vec<u16> = "Sunna Display Adapter\0".encode_utf16().collect();
    let instance_id: Vec<u16> = format!("sunnad-{}\0", std::process::id()).encode_utf16().collect();
    // SAFETY: the strings outlive the call; `created` outlives the callback:
    // it's freed only once the callback has signalled, and leaked otherwise.
    unsafe {
        let created = Box::new(Created { event: CreateEventW(None, true, false, PCWSTR::null())?, result: Mutex::new(None) });
        let info = SW_DEVICE_CREATE_INFO {
            cbSize: std::mem::size_of::<SW_DEVICE_CREATE_INFO>() as u32,
            pszInstanceId: PCWSTR(instance_id.as_ptr()),
            pszzHardwareIds: PCWSTR(hardware_ids.as_ptr()),
            CapabilityFlags: (SWDeviceCapabilitiesRemovable.0
                | SWDeviceCapabilitiesSilentInstall.0
                | SWDeviceCapabilitiesDriverRequired.0) as u32,
            pszDeviceDescription: PCWSTR(description.as_ptr()),
            ..Default::default()
        };
        let device = SwDeviceCreate(
            w("SunnaDisplay").as_pcwstr(),
            w("HTREE\\ROOT\\0").as_pcwstr(),
            &info,
            None,
            Some(on_created),
            Some(&*created as *const Created as *const c_void),
        )
        .context("creating the Sunna display adapter")?;
        let waited = WaitForSingleObject(created.event, 15_000);
        let outcome = created.result.lock().unwrap().take();
        let Some((result, instance)) = outcome else {
            SwDeviceClose(device);
            // The callback may still come: leave it something valid.
            std::mem::forget(created);
            bail!("Windows didn't finish creating the Sunna display adapter ({waited:?})");
        };
        let _ = CloseHandle(created.event);
        if let Err(error) = result.ok() {
            SwDeviceClose(device);
            return Err(error).context("creating the Sunna display adapter (is its driver installed?)");
        }
        let control = match open_control(&instance) {
            Ok(control) => control,
            Err(error) => {
                SwDeviceClose(device);
                return Err(error);
            }
        };
        let adapter = Adapter { device, control };
        let mut info = [0u32; 2];
        control_raw(adapter.control, IOCTL_INFO, &[], &mut info).context("asking the Sunna display driver")?;
        ensure!(info[0] == PROTOCOL, "the Sunna display driver speaks version {}, not {PROTOCOL}", info[0]);
        tracing::info!(%instance, max_monitors = info[1], "Sunna display adapter started");
        Ok(adapter)
    }
}

/// Open the driver's control interface on `instance`, waiting for the
/// driver to register it.
fn open_control(instance: &str) -> anyhow::Result<HANDLE> {
    let instance = w(instance);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        // SAFETY: sizes and buffers match; the list is double-NUL terminated.
        let path = unsafe {
            let mut size = 0;
            let found = CM_Get_Device_Interface_List_SizeW(&mut size, &INTERFACE, instance.as_pcwstr(), CM_GET_DEVICE_INTERFACE_LIST_PRESENT);
            let mut list = vec![0u16; size.max(1) as usize];
            if found == CR_SUCCESS
                && size > 1
                && CM_Get_Device_Interface_ListW(&INTERFACE, instance.as_pcwstr(), &mut list, CM_GET_DEVICE_INTERFACE_LIST_PRESENT) == CR_SUCCESS
            {
                let end = list.iter().position(|&c| c == 0).unwrap_or(list.len());
                (end > 0).then(|| list[..=end].to_vec())
            } else {
                None
            }
        };
        if let Some(path) = path {
            // SAFETY: NUL-terminated path; the handle is returned to the caller.
            return unsafe {
                CreateFileW(
                    PCWSTR(path.as_ptr()),
                    (GENERIC_READ | GENERIC_WRITE).0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    None,
                )
            }
            .context("opening the Sunna display driver");
        }
        ensure!(Instant::now() < deadline, "the Sunna display driver didn't start (is it installed?)");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn control(adapter: &Adapter, code: u32, input: &[u32], output: &mut [u32]) -> anyhow::Result<()> {
    control_raw(adapter.control, code, input, output)
}

fn control_raw(handle: HANDLE, code: u32, input: &[u32], output: &mut [u32]) -> anyhow::Result<()> {
    let mut returned = 0;
    // SAFETY: synchronous request; both buffers outlive it and sizes match.
    unsafe {
        DeviceIoControl(
            handle,
            code,
            (!input.is_empty()).then_some(input.as_ptr().cast()),
            std::mem::size_of_val(input) as u32,
            (!output.is_empty()).then_some(output.as_mut_ptr().cast()),
            std::mem::size_of_val(output) as u32,
            Some(&mut returned),
            None,
        )?;
    }
    ensure!(returned as usize == std::mem::size_of_val(output), "short answer from the Sunna display driver");
    Ok(())
}

/// GDI names of the active displays that are Sunna's.
fn sunna_displays() -> Vec<String> {
    let mut names = Vec::new();
    // SAFETY: QueryDisplayConfig with arrays of the size it asked for;
    // device-info packets are locals with their header sizes set.
    unsafe {
        let (mut paths, mut modes) = (0, 0);
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut paths, &mut modes) != ERROR_SUCCESS {
            return names;
        }
        let mut path_info = vec![DISPLAYCONFIG_PATH_INFO::default(); paths as usize];
        let mut mode_info = vec![DISPLAYCONFIG_MODE_INFO::default(); modes as usize];
        if QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut paths, path_info.as_mut_ptr(), &mut modes, mode_info.as_mut_ptr(), None) != ERROR_SUCCESS {
            return names;
        }
        for path in &path_info[..paths as usize] {
            let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
            target.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
            target.header.size = std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
            target.header.adapterId = path.targetInfo.adapterId;
            target.header.id = path.targetInfo.id;
            if DisplayConfigGetDeviceInfo(&mut target.header) != 0 || wide(&target.monitorFriendlyDeviceName) != MONITOR_NAME {
                continue;
            }
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
            source.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            source.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
            source.header.adapterId = path.sourceInfo.adapterId;
            source.header.id = path.sourceInfo.id;
            if DisplayConfigGetDeviceInfo(&mut source.header) == 0 {
                names.push(wide(&source.viewGdiDeviceName));
            }
        }
    }
    names
}

/// Put `display` in this mode (and remember it for that monitor).
fn set_mode(display: &str, width: u32, height: u32, refresh_hz: u32) -> anyhow::Result<()> {
    crate::windows::follow_input_desktop();
    let mode = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        dmFields: DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY,
        dmPelsWidth: width,
        dmPelsHeight: height,
        dmDisplayFrequency: refresh_hz,
        ..Default::default()
    };
    let name = w(display);
    // SAFETY: a filled-in DEVMODEW and a NUL-terminated display name.
    let result = unsafe { ChangeDisplaySettingsExW(name.as_pcwstr(), Some(&mode), None, CDS_UPDATEREGISTRY, None) };
    ensure!(result == DISP_CHANGE_SUCCESSFUL, "setting {display} to {width}x{height}@{refresh_hz}: {result:?}");
    Ok(())
}

/// A NUL-terminated UTF-16 string.
struct Wide(Vec<u16>);
impl Wide {
    fn as_pcwstr(&self) -> PCWSTR {
        PCWSTR(self.0.as_ptr())
    }
}
fn w(text: &str) -> Wide {
    Wide(text.encode_utf16().chain(Some(0)).collect())
}

fn wide(text: &[u16]) -> String {
    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    String::from_utf16_lossy(&text[..end])
}
