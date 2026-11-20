//! Add one of Sunna's own virtual displays for `seconds`, then remove it
//! (the driver in `drivers/windows/sunna-display` must be installed).
//!
//! `virtual_display [WxH] [refresh_hz] [seconds]`

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_writer(std::io::stderr).init();
    let args: Vec<String> = std::env::args().collect();
    let (width, height) = args
        .get(1)
        .and_then(|v| v.split_once('x'))
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .unwrap_or((1920, 1080));
    let refresh: u32 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(60);
    let seconds: u64 = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(30);
    let display = sunna_capture::windows_display::VirtualDisplay::create(width, height, refresh)?;
    println!("added {display:?} for {seconds} s");
    for info in sunna_capture::windows::displays()? {
        println!("{info:?}");
    }
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    drop(display);
    println!("removed");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("virtual_display runs on Windows only");
}
