//! Development only: borrow an already-installed Parsec Virtual Display
//! driver to add one virtual display for `seconds`, then remove it, so
//! capture can be tested on a PC with no monitor. Sunna never ships or
//! relies on this; its own virtual display driver replaces it.
//! Protocol from github.com/nomi-san/parsec-vdd (MIT). The display stays only
//! while it's pinged, so it disappears if this process dies.
//!
//! `parsec_display [seconds]`

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    use std::time::{Duration, Instant};

    use anyhow::{bail, Context};
    use windows::core::{GUID, PCWSTR};
    use windows::Win32::Devices::DeviceAndDriverInstallation::*;
    use windows::Win32::Foundation::*;
    use windows::Win32::Storage::FileSystem::*;
    use windows::Win32::System::Threading::CreateEventW;
    use windows::Win32::System::IO::{DeviceIoControl, GetOverlappedResultEx, OVERLAPPED};

    const ADAPTER: GUID = GUID::from_u128(0x00b41627_04c4_429e_a26e_0265cf50c8fa);
    const ADD: u32 = 0x0022e004;
    const REMOVE: u32 = 0x0022a008;
    const UPDATE: u32 = 0x0022a00c;
    const VERSION: u32 = 0x0022e010;

    let seconds: u64 = std::env::args().nth(1).and_then(|v| v.parse().ok()).unwrap_or(30);

    // SAFETY: SetupAPI enumeration and CreateFile with locals; the device
    // handle lives until the end of main.
    let device = unsafe {
        let info = SetupDiGetClassDevsW(Some(&ADAPTER), PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE)?;
        let mut interface = SP_DEVICE_INTERFACE_DATA {
            cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        SetupDiEnumDeviceInterfaces(info, None, &ADAPTER, 0, &mut interface)
            .context("no Parsec Virtual Display Adapter here")?;
        let mut size = 0;
        let _ = SetupDiGetDeviceInterfaceDetailW(info, &interface, None, 0, Some(&mut size), None);
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        let detail = buffer.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
        (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
        SetupDiGetDeviceInterfaceDetailW(info, &interface, Some(detail), size, None, None)?;
        let path = PCWSTR((*detail).DevicePath.as_ptr());
        let handle = CreateFileW(
            path,
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_NO_BUFFERING | FILE_FLAG_OVERLAPPED | FILE_FLAG_WRITE_THROUGH,
            None,
        )?;
        let _ = SetupDiDestroyDeviceInfoList(info);
        handle
    };

    let control = |code: u32, input: &[u8]| -> anyhow::Result<u32> {
        let mut in_buffer = [0u8; 32];
        in_buffer[..input.len()].copy_from_slice(input);
        let mut out = 0u32;
        let mut transferred = 0;
        // SAFETY: overlapped I/O on our handle with buffers that outlive the wait.
        unsafe {
            let event = CreateEventW(None, true, false, PCWSTR::null())?;
            let mut overlapped = OVERLAPPED { hEvent: event, ..Default::default() };
            let _ = DeviceIoControl(
                device,
                code,
                Some(in_buffer.as_ptr().cast()),
                in_buffer.len() as u32,
                Some((&mut out as *mut u32).cast()),
                4,
                None,
                Some(&mut overlapped),
            );
            let done = GetOverlappedResultEx(device, &overlapped, &mut transferred, 5000, false);
            let _ = CloseHandle(event);
            if done.is_err() {
                bail!("IOCTL {code:#x} failed");
            }
        }
        Ok(out)
    };

    println!("Parsec VDA driver minor version {}", control(VERSION, &[])?);
    let index = control(ADD, &[])?;
    control(UPDATE, &[])?;
    println!("added virtual display {index} for {seconds} s");
    let until = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < until {
        control(UPDATE, &[])?;
        std::thread::sleep(Duration::from_millis(50));
    }
    let big_endian = (index as u16).to_be_bytes();
    control(REMOVE, &big_endian)?;
    control(UPDATE, &[])?;
    println!("removed virtual display {index}");
    // SAFETY: our handle, closed once.
    unsafe {
        let _ = CloseHandle(device);
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("parsec_display runs on Windows only");
}
