//! A moving test pattern for measuring Windows capture: a full-screen
//! window, redrawn every display refresh, with a sweeping bar, colour
//! patches of known sRGB values and the frame number as 16 black/white
//! blocks along the top (readable from a decoded frame). Prints the input
//! it receives, so a host's keyboard and mouse injection can be checked
//! without touching anything else on the desktop.
//!
//! `test_pattern [seconds]`

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    use std::time::{Duration, Instant};

    use windows::core::w;
    use windows::Win32::Foundation::*;
    use windows::Win32::Graphics::Dwm::DwmFlush;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;

    /// Patches drawn left to right: their sRGB values are what a viewer
    /// should get back.
    const PATCHES: [(u8, u8, u8); 6] = [
        (255, 255, 255),
        (0, 0, 0),
        (235, 64, 52),
        (45, 212, 191),
        (40, 90, 200),
        (128, 128, 128),
    ];

    /// Report the input this window receives, to check a host's injection.
    extern "system" fn proc(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        let scan = (l.0 >> 16) & 0x1ff;
        let (x, y) = ((l.0 & 0xffff) as i16, ((l.0 >> 16) & 0xffff) as i16);
        match message {
            WM_KEYDOWN | WM_SYSKEYDOWN => println!("input: key down vk={:#04x} scan={scan:#05x}", w.0),
            WM_KEYUP | WM_SYSKEYUP => println!("input: key up vk={:#04x} scan={scan:#05x}", w.0),
            WM_CHAR => println!("input: char {:?}", char::from_u32(w.0 as u32)),
            WM_LBUTTONDOWN => println!("input: left button down at {x},{y}"),
            WM_LBUTTONUP => println!("input: left button up at {x},{y}"),
            WM_RBUTTONDOWN => println!("input: right button down at {x},{y}"),
            WM_MOUSEWHEEL => println!("input: wheel {}", (w.0 >> 16) as i16),
            _ => {}
        }
        // SAFETY: default handling for every message.
        unsafe { DefWindowProcW(window, message, w, l) }
    }

    let seconds: u64 = std::env::args().nth(1).and_then(|v| v.parse().ok()).unwrap_or(30);
    // SAFETY: plain Win32 window creation and GDI drawing on this thread.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance.into(),
            lpszClassName: w!("SunnaTestPattern"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let (width, height) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let window = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("SunnaTestPattern"),
            w!("Sunna test pattern"),
            WS_POPUP | WS_VISIBLE,
            0,
            0,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        let dc = GetDC(Some(window));
        let started = Instant::now();
        let mut frame: u32 = 0;
        while started.elapsed() < Duration::from_secs(seconds) {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let fill = |x: i32, y: i32, w: i32, h: i32, (r, g, b): (u8, u8, u8)| {
                let brush = CreateSolidBrush(COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16));
                FillRect(dc, &RECT { left: x, top: y, right: x + w, bottom: y + h }, brush);
                let _ = DeleteObject(brush.into());
            };
            fill(0, 0, width, height, (16, 16, 24));
            let patch = width / PATCHES.len() as i32;
            for (i, colour) in PATCHES.iter().enumerate() {
                fill(i as i32 * patch, height / 2, patch, height / 4, *colour);
            }
            let bar = (frame as i32 * 8) % width;
            fill(bar, height / 8, 16, height / 4, (255, 255, 255));
            let block = width / 16;
            for bit in 0..16 {
                let on = frame >> (15 - bit) & 1 == 1;
                fill(bit * block, 0, block, height / 16, if on { (255, 255, 255) } else { (0, 0, 0) });
            }
            let _ = GdiFlush();
            let _ = DwmFlush();
            frame += 1;
        }
        ReleaseDC(Some(window), dc);
        let _ = DestroyWindow(window);
        println!("test pattern: {frame} frames in {:.1} s", started.elapsed().as_secs_f64());
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("test_pattern runs on Windows only");
}
