use anyhow::{ensure, Result};
use sunna_capture::{macos::ScreenSource, FrameSource};
use sunna_host::{HostConfig, StreamConfig};
use sunna_transport::Server;

pub fn config(
    name: String,
    token: String,
    codec: String,
    fps: u32,
    audio: bool,
    clipboard: bool,
) -> Result<HostConfig> {
    ensure!(!token.is_empty(), "Set this Mac’s sharing key in Settings.");
    ensure!(
        (1..=240).contains(&fps),
        "Choose a frame rate from 1 to 240."
    );
    let (w, h) = sunna_capture::macos::main_display_pixel_size();
    Ok(HostConfig {
            require_proof: std::env::var("SUNNA_REQUIRE_PROOF").map_or(false, |v| v == "1"),
        name,
        token,
        width: w.max(2) & !1,
        height: h.max(2) & !1,
        fps,
        codec,
        max_bitrate_bps: 40_000_000,
        fast_lane: sunna_capture::fast_lane_enabled(false),
        about: super::about::detect(),
        display_follows_viewer: false,
        clipboard,
        audio,
        simulate_loss: 0.0,
    })
}

fn capture(s: &StreamConfig) -> Result<Box<dyn FrameSource>> {
    let source = (|| -> Result<_> {
        let mut source = ScreenSource::new(
            Some(s.width),
            Some(s.height),
            s.fps,
            s.fast_lane || s.codec == "raw",
        )?;
        source.next_frame()?;
        Ok(source)
    })();
    if source.is_err() {
        sunna_host::macos::capture_failed();
    }
    Ok(Box::new(source?))
}

/// Check real capture before declaring the host ready.
pub fn check_pipeline(s: &HostConfig) -> Result<()> {
    ensure!(
        sunna_input::macos::accessibility_granted(),
        "Accessibility permission required"
    );
    ensure!(
        sunna_capture::macos::screen_capture_granted(),
        "Screen Recording permission required"
    );
    let _input = sunna_input::macos::MacInjector::new();
    let _encoder =
        sunna_codec::make_encoder(&s.codec, s.width, s.height, s.fps, s.max_bitrate_bps)?;
    check_frame(s)
}

pub fn check_frame(s: &HostConfig) -> Result<()> {
    let mut source = ScreenSource::new(Some(s.width), Some(s.height), s.fps, s.fast_lane)?;
    source.next_frame()?;
    Ok(())
}

pub async fn serve(server: Server, config: HostConfig) -> Result<()> {
    sunna_host::run_host(
        server,
        config,
        Box::new(capture),
        Box::new(|s| sunna_codec::make_encoder(&s.codec, s.width, s.height, s.fps, s.bitrate_bps)),
        Box::new(sunna_input::make_injector),
    )
    .await
}
