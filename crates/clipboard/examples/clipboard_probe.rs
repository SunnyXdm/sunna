//! Development only: read the system clipboard as Sunna does, every half
//! second for `seconds`, and print what it would share.
//!
//! `clipboard_probe [seconds]`

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("debug").with_writer(std::io::stderr).init();
    let seconds: u64 = std::env::args().nth(1).and_then(|v| v.parse().ok()).unwrap_or(10);
    let mut clipboard = sunna_clipboard::system_clipboard()?;
    let mut last = None;
    for _ in 0..seconds * 2 {
        let count = clipboard.change_count();
        if last != Some(count) {
            last = Some(count);
            let content = clipboard.read();
            println!(
                "change {count:#x}: {}",
                match &content {
                    Some(sunna_clipboard::ClipboardContent::Text(text)) => format!("text {text:?}"),
                    Some(sunna_clipboard::ClipboardContent::Png(png)) => format!("png, {} bytes", png.len()),
                    None => "nothing".into(),
                }
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Ok(())
}
