//! Windows capture + NVENC timing, without a viewer:
//! `windows_bench [h264|hevc] [frames] [WxH] [fps]`. Prints per-stage
//! percentiles and writes the bitstream to `bench.<codec>` and one converted
//! NV12 frame to `bench.nv12` for checking colours elsewhere.

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    use std::io::Write;
    use std::time::Instant;

    use sunna_capture::FrameSource;
    use sunna_codec::{Codec, Encoder};

    tracing_subscriber::fmt().with_writer(std::io::stderr).init();
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("caps") {
        for (codec, values) in sunna_codec::nvenc::capabilities()? {
            println!("{codec}: {values:?}");
        }
        return Ok(());
    }
    let codec = match args.get(1).map(String::as_str) {
        Some("hevc") => Codec::Hevc,
        _ => Codec::H264,
    };
    let frames: usize = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(300);
    let size = args.get(3).and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))
    });
    let fps: u32 = args.get(4).and_then(|v| v.parse().ok()).unwrap_or(60);

    for display in sunna_capture::windows::displays()? {
        println!("{display:?}");
    }
    println!("nvenc h264={} hevc={}", sunna_codec::nvenc::supports(Codec::H264), sunna_codec::nvenc::supports(Codec::Hevc));
    let mut source = sunna_capture::windows::ScreenSource::new(size.map(|s| s.0), size.map(|s| s.1), fps, false)?;
    let (width, height) = (source.width(), source.height());
    let mut encoder = sunna_codec::nvenc::NvencEncoder::new(codec, width, height, fps, 30_000_000)?;
    let mut out = std::fs::File::create(format!("bench.{}", codec.name()))?;
    let (mut wait, mut encode, mut sizes) = (Vec::new(), Vec::new(), Vec::new());
    let started = Instant::now();
    for i in 0..frames {
        let before = Instant::now();
        let frame = source.next_frame()?;
        wait.push(before.elapsed().as_micros() as u64);
        if i == 0 {
            std::fs::write("bench.nv12", frame.data.to_cpu()?)?;
        }
        let encoded = encoder.encode(&frame)?.expect("NVENC never drops");
        encode.push(encoded.encode_us);
        sizes.push(encoded.data.len() as u64);
        out.write_all(&encoded.data)?;
    }
    let elapsed = started.elapsed().as_secs_f64();
    let stats = |name: &str, values: &mut Vec<u64>, unit: f64| {
        values.sort_unstable();
        let at = |p: f64| values[((values.len() - 1) as f64 * p) as usize] as f64 / unit;
        println!(
            "{name}: p50 {:.2} p95 {:.2} p99 {:.2} max {:.2}",
            at(0.5),
            at(0.95),
            at(0.99),
            at(1.0)
        );
    };
    println!("{width}x{height} {} {frames} frames in {elapsed:.1} s ({:.1} fps)", codec.name(), frames as f64 / elapsed);
    stats("capture wait ms", &mut wait, 1000.0);
    stats("encode ms", &mut encode, 1000.0);
    stats("frame KB", &mut sizes, 1000.0);
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("windows_bench runs on Windows only");
}
