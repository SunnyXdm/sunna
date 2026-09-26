//! Stream viewer window.
//!
//! The decode thread stores the latest decoded frame (latest-wins, no queue)
//! and wakes the event loop. On macOS, hardware-decoded IOSurfaces go straight
//! to a CALayer (see layer_presenter.rs): no copies, GPU scaling, shown as
//! soon as they arrive. Elsewhere (and as a fallback) a softbuffer CPU blit
//! nearest-neighbour scales BGRA into the window.

use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sunna_client::LiveStats;
use sunna_codec::DecodedFrame;
use sunna_proto::messages::InputEvent;
use tokio::sync::mpsc::UnboundedSender;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::keyboard::{self, Hotkey};
use crate::keymap;
#[cfg(target_os = "macos")]
use crate::layer_presenter::LayerPresenter;
#[cfg(target_os = "macos")]
use crate::mac_keyboard::KeyboardCapture;

/// How much to scale the stream to show it in `area` (both in pixels).
/// 1:1 when the stream was sized for this screen (a Mac host matches the
/// viewer) so text stays pixel-exact; otherwise aspect-fit, which also scales
/// a much smaller stream (a 1080p Linux desktop on a Retina screen) up to
/// fill instead of leaving it in a small box.
pub fn stream_scale(stream: (f64, f64), area: (f64, f64)) -> f64 {
    let fit = (area.0 / stream.0).min(area.1 / stream.1);
    let fits = fit >= 1.0;
    let nearly_fills = stream.0 >= area.0 * 0.85 || stream.1 >= area.1 * 0.85;
    if fits && nearly_fills {
        1.0
    } else {
        fit
    }
}

/// `SUNNA_WINDOWED=1` opens a window instead of full screen.
pub fn windowed() -> bool {
    std::env::var("SUNNA_WINDOWED").is_ok_and(|value| value == "1")
}

/// Shared between the network thread (writer) and the viewer (reader).
#[derive(Default)]
pub struct SharedFrame {
    pub latest: Mutex<Option<DecodedFrame>>,
    /// Fast-lane tile batches waiting to be drawn (in arrival order).
    pub tiles: Mutex<Vec<sunna_proto::tiles::TileBatch>>,
    /// Session numbers for the stats bar.
    pub live: Arc<Mutex<LiveStats>>,
    /// Viewer hotkeys caught by the keyboard tap, for the event loop.
    pub hotkeys: Mutex<Vec<Hotkey>>,
}

/// How long a notice stays up.
#[cfg(target_os = "macos")]
const NOTICE_TIME: Duration = Duration::from_secs(6);
#[cfg(target_os = "macos")]
const HOTKEY_HELP: &str = "⌃⌥G release keyboard  ·  ⌃⌥F full screen  ·  ⌃⌥S stats  ·  ⌃⌥Q disconnect";

#[cfg(target_os = "macos")]
const ACCESSIBILITY_HELP: &str = "To send ⌘Tab, ⌘Space and other shortcuts to the remote, allow your terminal in System Settings → Privacy & Security → Accessibility, then reconnect  ·  ⌃⌥Q disconnect";

/// One line for the stats bar (⌃⌥S), Parsec-style.
pub fn stats_text(live: &LiveStats) -> String {
    if live.codec.is_empty() {
        return "connecting…".into();
    }
    let mut parts = vec![
        format!("{} {}×{}", live.codec.to_uppercase(), live.width, live.height),
        format!("{} fps", live.fps),
    ];
    match &live.host {
        Some(host) => parts.push(format!(
            "{:.1} / {:.0} Mbps",
            live.mbps,
            host.target_kbps as f64 / 1000.0
        )),
        None => parts.push(format!("{:.1} Mbps", live.mbps)),
    }
    if let Some((p50, p95)) = live.latency_ms {
        parts.push(format!("latency {p50:.0} ms (p95 {p95:.0})"));
    }
    if let Some(host) = &live.host {
        parts.push(format!("encode {:.1} ms", host.encode_us_p50 as f64 / 1000.0));
    }
    if let Some(decode) = live.decode_ms_p50 {
        parts.push(format!("decode {decode:.1} ms"));
    }
    if let Some(rtt) = live.rtt_ms {
        parts.push(format!("rtt {rtt:.0} ms"));
    }
    if live.dropped > 0 {
        parts.push(format!("lost {}", live.dropped));
    }
    if live.tile_batches > 0 {
        parts.push(format!("tiles {}/s", live.tile_batches));
    }
    parts.push("⌃⌥S hide".into());
    parts.join("  ·  ")
}

/// Wake signal sent by the network thread after storing a frame.
#[derive(Debug)]
pub struct FrameReady;

pub fn create_event_loop() -> anyhow::Result<EventLoop<FrameReady>> {
    Ok(EventLoop::<FrameReady>::with_user_event().build()?)
}

pub fn run_viewer(
    event_loop: EventLoop<FrameReady>,
    shared: Arc<SharedFrame>,
    input: UnboundedSender<InputEvent>,
    title: String,
    width: u32,
    height: u32,
) -> anyhow::Result<()> {
    let proxy = event_loop.create_proxy();
    let mut app = ViewerApp {
        proxy,
        notice: None,
        #[cfg(target_os = "macos")]
        capture: None,
        shared,
        input,
        title,
        stream_size: (width.max(1), height.max(1)),
        window: None,
        surface: None,
        x_lut: Vec::new(),
        lut_key: (0, 0),
        keys_down: Vec::new(),
        modifiers: ModifiersState::empty(),
        stats_visible: std::env::var("SUNNA_STATS").map_or(true, |value| value != "0"),
        stats_updated: Instant::now() - Duration::from_secs(1),
        hotkey_down: None,
        #[cfg(target_os = "macos")]
        layer: None,
        presented: 0,
        present_window: Instant::now(),
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

struct ViewerApp {
    /// Wakes the event loop (the keyboard tap uses it for hotkeys).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    proxy: EventLoopProxy<FrameReady>,
    /// Bottom notice and when it goes away.
    notice: Option<(String, Instant)>,
    /// Sends ⌘Tab and other system shortcuts to the host while focused.
    #[cfg(target_os = "macos")]
    capture: Option<KeyboardCapture>,
    shared: Arc<SharedFrame>,
    input: UnboundedSender<InputEvent>,
    title: String,
    stream_size: (u32, u32),
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    /// Precomputed source byte offsets per destination column (nearest
    /// neighbor); rebuilt only when source/window widths change.
    x_lut: Vec<usize>,
    lut_key: (usize, usize),
    /// Keys we've sent as pressed, released on focus loss: the key-up for,
    /// say, Cmd during Cmd-Tab goes to another app and would leave it stuck.
    keys_down: Vec<u16>,
    modifiers: ModifiersState,
    /// Stats bar shown (toggled with ⌃⌥S; `SUNNA_STATS=0` starts hidden).
    stats_visible: bool,
    stats_updated: Instant,
    /// A viewer hotkey's key is held (window key path): its key-up is ours
    /// too, not the host's.
    hotkey_down: Option<KeyCode>,
    /// Zero-copy presenter; when set, softbuffer isn't used.
    #[cfg(target_os = "macos")]
    layer: Option<LayerPresenter>,
    presented: u64,
    present_window: Instant,
}

impl ViewerApp {
    /// Redraw the stats bar, at most twice a second unless `force`d. Frames
    /// arrive at least every ~100 ms (the host's idle refresh), which drives
    /// this; only the macOS presenter draws a bar so far.
    fn refresh_stats(&mut self, force: bool) {
        if !force && self.stats_updated.elapsed() < Duration::from_millis(500) {
            return;
        }
        self.stats_updated = Instant::now();
        let text = self
            .stats_visible
            .then(|| stats_text(&self.shared.live.lock().unwrap()));
        #[cfg(target_os = "macos")]
        if let Some(layer) = self.layer.as_mut() {
            layer.show_stats(text.as_deref());
        }
        #[cfg(not(target_os = "macos"))]
        let _ = text;
    }

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn show_notice(&mut self, text: impl Into<String>, time: Duration) {
        let text = text.into();
        #[cfg(target_os = "macos")]
        if let Some(layer) = self.layer.as_mut() {
            layer.show_notice(Some(&text));
        }
        tracing::info!(notice = %text, "viewer notice");
        self.notice = Some((text, Instant::now() + time));
    }

    fn expire_notice(&mut self) {
        if self.notice.as_ref().is_some_and(|(_, until)| Instant::now() >= *until) {
            self.notice = None;
            #[cfg(target_os = "macos")]
            if let Some(layer) = self.layer.as_mut() {
                layer.show_notice(None);
            }
        }
    }

    fn apply_hotkey(&mut self, event_loop: &ActiveEventLoop, hotkey: Hotkey) {
        match hotkey {
            Hotkey::ToggleStats => {
                self.stats_visible = !self.stats_visible;
                self.refresh_stats(true);
            }
            Hotkey::ToggleFullscreen => {
                if let Some(window) = &self.window {
                    let full = window.fullscreen().is_some();
                    window.set_fullscreen((!full).then_some(Fullscreen::Borderless(None)));
                }
            }
            Hotkey::ToggleCapture => self.toggle_capture(),
            Hotkey::Disconnect => {
                tracing::info!("disconnect requested (⌃⌥Q)");
                event_loop.exit();
            }
        }
    }

    /// ⌃⌥G from the window's key events, which only arrive while capture
    /// is off: turn it on.
    fn toggle_capture(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let Some(capture) = &self.capture else {
                self.show_notice(ACCESSIBILITY_HELP, NOTICE_TIME * 2);
                return;
            };
            capture.set_enabled(!capture.enabled());
            self.capture_changed();
        }
    }

    /// Announce the capture state. Turning it on also releases keys held
    /// through the window's key events: their key-ups now go to the tap.
    fn capture_changed(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let enabled = self.capture.as_ref().is_some_and(|capture| capture.enabled());
            if enabled {
                self.release_window_keys();
                self.show_notice(
                    "Keyboard captured: ⌘Tab, ⌘Space and other shortcuts go to the remote  ·  ⌃⌥G gives them back to this Mac",
                    NOTICE_TIME,
                );
            } else {
                self.show_notice(
                    "Keyboard released: shortcuts stay on this Mac  ·  ⌃⌥G to capture again",
                    NOTICE_TIME,
                );
            }
        }
    }

    /// Take ⌘Tab and the other system shortcuts for the host (macOS; needs
    /// the Accessibility permission).
    fn install_capture(&mut self) {
        #[cfg(target_os = "macos")]
        match KeyboardCapture::new(self.input.clone(), Arc::clone(&self.shared), self.proxy.clone()) {
            Ok(capture) => {
                self.capture = Some(capture);
                self.show_notice(HOTKEY_HELP, NOTICE_TIME);
            }
            Err(error) => {
                tracing::warn!(%error, "keyboard capture unavailable; macOS keeps its shortcuts");
                self.show_notice(ACCESSIBILITY_HELP, NOTICE_TIME * 2);
            }
        }
    }

    fn release_window_keys(&mut self) {
        for scancode in std::mem::take(&mut self.keys_down) {
            let _ = self.input.send(InputEvent::Key {
                scancode,
                pressed: false,
                repeat: false,
            });
        }
    }

    fn uses_layer(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.layer.is_some()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// Where the stream appears in the window, in physical pixels: the
    /// layer presenter letterboxes (aspect-fit); the CPU blit stretches.
    fn content_rect(&self, window: (f64, f64)) -> (f64, f64, f64, f64) {
        let (win_w, win_h) = window;
        if !self.uses_layer() {
            return (0.0, 0.0, win_w, win_h);
        }
        let (stream_w, stream_h) = (self.stream_size.0 as f64, self.stream_size.1 as f64);
        let scale = stream_scale((stream_w, stream_h), (win_w, win_h));
        let (w, h) = (stream_w * scale, stream_h * scale);
        ((win_w - w) / 2.0, (win_h - h) / 2.0, w, h)
    }

    fn note_presented(&mut self) {
        self.presented += 1;
        let elapsed = self.present_window.elapsed();
        if elapsed >= Duration::from_secs(1) {
            tracing::info!(
                fps = self.presented,
                layer = self.uses_layer(),
                "viewer present"
            );
            self.presented = 0;
            self.present_window = Instant::now();
        }
    }

    fn render(&mut self) {
        let (Some(window), Some(surface)) = (self.window.as_ref(), self.surface.as_mut()) else {
            return;
        };
        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };
        let frame = self.shared.latest.lock().unwrap().clone();
        let Some(frame) = frame else { return };
        if let Err(error) = surface.resize(width, height) {
            tracing::warn!(%error, "surface resize failed");
            return;
        }
        let mut buffer = match surface.buffer_mut() {
            Ok(buffer) => buffer,
            Err(error) => {
                tracing::warn!(%error, "surface buffer unavailable");
                return;
            }
        };

        let (dst_w, dst_h) = (size.width as usize, size.height as usize);
        let (src_w, src_h) = (frame.width as usize, frame.height as usize);
        let Ok(bytes) = frame.data.to_cpu() else { return };
        let src = &bytes[..];
        if src.len() < src_w * src_h * 4 || buffer.len() < dst_w * dst_h {
            return; // malformed frame; never index out of bounds
        }

        // BGRA little-endian bytes ARE the 0RGB u32 layout softbuffer wants
        // (b|g<<8|r<<16), so the size-matched path is a straight conversion
        // copy and the scaler is nearest-neighbor with a precomputed column
        // LUT — no per-pixel division. CPU blit is Milestone 0; wgpu replaces it.
        if dst_w == src_w && dst_h == src_h {
            for (dst, chunk) in buffer.iter_mut().zip(src.chunks_exact(4)) {
                *dst = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], 0]);
            }
        } else {
            if self.lut_key != (src_w, dst_w) {
                self.x_lut = (0..dst_w).map(|x| (x * src_w / dst_w) * 4).collect();
                self.lut_key = (src_w, dst_w);
            }
            for y in 0..dst_h {
                let sy = y * src_h / dst_h;
                let src_row = &src[sy * src_w * 4..(sy + 1) * src_w * 4];
                let dst_row = &mut buffer[y * dst_w..(y + 1) * dst_w];
                for (dst, &sx) in dst_row.iter_mut().zip(self.x_lut.iter()) {
                    dst_row_write(dst, src_row, sx);
                }
            }
        }
        if let Err(error) = buffer.present() {
            tracing::warn!(%error, "present failed");
        }
        self.note_presented();
    }
}

#[inline(always)]
fn dst_row_write(dst: &mut u32, src_row: &[u8], sx: usize) {
    *dst = u32::from_le_bytes([src_row[sx], src_row[sx + 1], src_row[sx + 2], 0]);
}

impl ApplicationHandler<FrameReady> for ViewerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        // macOS (GPU-scaled layer): fit the stream's aspect into most of the
        // screen, scaling up or down. Elsewhere: the stream's size in
        // physical pixels (1:1 → the CPU blit's fast path), capped below the
        // monitor.
        let (mut width, mut height) = self.stream_size;
        if let Some(monitor) = event_loop.primary_monitor() {
            let monitor_size = monitor.size();
            if monitor_size.width > 0 && monitor_size.height > 0 {
                let (max_w, max_h) = (monitor_size.width * 9 / 10, monitor_size.height * 8 / 10);
                if cfg!(target_os = "macos") {
                    let scale = (max_w as f64 / width as f64).min(max_h as f64 / height as f64);
                    width = (width as f64 * scale) as u32;
                    height = (height as f64 * scale) as u32;
                } else {
                    width = width.min(max_w);
                    height = height.min(max_h);
                }
            }
        }
        let mut attributes = Window::default_attributes()
            .with_title(&self.title)
            .with_inner_size(PhysicalSize::new(width, height));
        if cfg!(target_os = "macos") && !windowed() {
            attributes =
                attributes.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                tracing::error!(%error, "failed to create window");
                event_loop.exit();
                return;
            }
        };
        tracing::info!(
            stream = format!("{}x{}", self.stream_size.0, self.stream_size.1),
            window = format!("{}x{}", window.inner_size().width, window.inner_size().height),
            scale_factor = window.scale_factor(),
            "viewer window"
        );
        #[cfg(target_os = "macos")]
        match LayerPresenter::new(&window, self.stream_size) {
            Ok(layer) => {
                tracing::info!("presenting via CALayer (zero-copy IOSurface)");
                self.layer = Some(layer);
                self.window = Some(window);
                self.install_capture();
                return;
            }
            Err(error) => tracing::warn!(%error, "layer presenter unavailable, using CPU blit"),
        }

        let context = match softbuffer::Context::new(window.clone()) {
            Ok(context) => context,
            Err(error) => {
                tracing::error!(%error, "failed to create render context");
                event_loop.exit();
                return;
            }
        };
        match softbuffer::Surface::new(&context, window.clone()) {
            Ok(surface) => self.surface = Some(surface),
            Err(error) => {
                tracing::error!(%error, "failed to create render surface");
                event_loop.exit();
                return;
            }
        }
        self.window = Some(window);
        self.install_capture();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: FrameReady) {
        let hotkeys = std::mem::take(&mut *self.shared.hotkeys.lock().unwrap());
        for hotkey in hotkeys {
            match hotkey {
                // The tap has already released capture by the time it gets
                // here (so the rest of the chord went to macOS); just say so.
                Hotkey::ToggleCapture => self.capture_changed(),
                other => self.apply_hotkey(event_loop, other),
            }
        }
        self.expire_notice();
        #[cfg(target_os = "macos")]
        if self.layer.is_some() {
            // Show immediately rather than waiting for the next redraw. Video
            // first: it retires tiles it already contains; tiles newer than it
            // then go on top.
            let frame = self.shared.latest.lock().unwrap().take();
            if let Some(frame) = frame {
                match frame.data {
                    sunna_capture::FrameData::Surface(surface) => {
                        if let Some(layer) = self.layer.as_mut() {
                            layer.show(surface, frame.capture_ts_us);
                        }
                        self.note_presented();
                    }
                    sunna_capture::FrameData::Cpu(_) => {
                        tracing::warn!("CPU frame with the layer presenter; dropped");
                    }
                }
            }
            let batches = std::mem::take(&mut *self.shared.tiles.lock().unwrap());
            if let (Some(layer), Some(window)) = (self.layer.as_mut(), self.window.as_ref()) {
                for batch in batches {
                    layer.add_tiles(window, batch);
                }
            }
            self.refresh_stats(false);
            return;
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                if !self.uses_layer() {
                    self.render();
                }
            }
            WindowEvent::Resized(size) => {
                #[cfg(target_os = "macos")]
                if let (Some(layer), Some(window)) = (self.layer.as_mut(), self.window.as_ref()) {
                    layer.fit(window, (size.width, size.height));
                }
                #[cfg(not(target_os = "macos"))]
                let _ = size;
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            // Input forwarding: window coordinates → normalized host
            // coordinates; keys → mac virtual keycodes (see keymap.rs).
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(window) = &self.window {
                    let size = window.inner_size();
                    if size.width > 0 && size.height > 0 {
                        let (x0, y0, w, h) =
                            self.content_rect((size.width as f64, size.height as f64));
                        let _ = self.input.send(InputEvent::MouseMoveAbs {
                            x: ((position.x - x0) / w).clamp(0.0, 1.0) as f32,
                            y: ((position.y - y0) / h).clamp(0.0, 1.0) as f32,
                        });
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                use sunna_proto::messages::MouseButton as Proto;
                let button = match button {
                    winit::event::MouseButton::Left => Proto::Left,
                    winit::event::MouseButton::Right => Proto::Right,
                    winit::event::MouseButton::Middle => Proto::Middle,
                    winit::event::MouseButton::Back => Proto::X1,
                    winit::event::MouseButton::Forward => Proto::X2,
                    winit::event::MouseButton::Other(_) => return,
                };
                let _ = self.input.send(InputEvent::MouseButton {
                    button,
                    pressed: state == ElementState::Pressed,
                });
            }
            WindowEvent::MouseWheel { delta, phase, .. } => {
                use sunna_proto::messages::GesturePhase;
                // Pixel deltas come from trackpads/Magic Mouse and carry a
                // phase; the host replays them as continuous scrolling.
                let (dx, dy, phase) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * 32.0, y * 32.0, None),
                    MouseScrollDelta::PixelDelta(position) => (
                        position.x as f32,
                        position.y as f32,
                        Some(match phase {
                            TouchPhase::Started => GesturePhase::Begin,
                            TouchPhase::Moved => GesturePhase::Update,
                            TouchPhase::Ended => GesturePhase::End,
                            TouchPhase::Cancelled => GesturePhase::Cancel,
                        }),
                    ),
                };
                let _ = self.input.send(InputEvent::Scroll {
                    dx,
                    dy,
                    phase,
                    momentum: false,
                });
            }
            WindowEvent::Focused(focused) => {
                #[cfg(target_os = "macos")]
                if let Some(capture) = &self.capture {
                    capture.set_focused(focused);
                }
                if !focused {
                    self.release_window_keys();
                    self.hotkey_down = None;
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. } => {
                // Viewer hotkeys (⌃⌥ + S/F/G/Q) never reach the host. This
                // path only sees keys the tap didn't take: capture off, or
                // no Accessibility permission.
                if let PhysicalKey::Code(code) = event.physical_key {
                    let pressed = event.state == ElementState::Pressed;
                    // A hotkey's key is ours from press to release, repeats too.
                    if self.hotkey_down == Some(code) {
                        if !pressed {
                            self.hotkey_down = None;
                        }
                        return;
                    }
                    let flags = [
                        (self.modifiers.control_key(), keyboard::flags::CONTROL),
                        (self.modifiers.alt_key(), keyboard::flags::OPTION),
                        (self.modifiers.super_key(), keyboard::flags::COMMAND),
                    ]
                    .into_iter()
                    .filter(|(held, _)| *held)
                    .fold(0, |flags, (_, bit)| flags | bit);
                    // Only a fresh press can be a hotkey: a key already held
                    // (and sent) when ⌃⌥ joins it stays the host's.
                    let hotkey = keymap::mac_keycode(code)
                        .filter(|vk| pressed && !event.repeat && !self.keys_down.contains(vk))
                        .and_then(|vk| Hotkey::from_key(vk, flags));
                    if let Some(hotkey) = hotkey {
                        self.hotkey_down = Some(code);
                        self.apply_hotkey(event_loop, hotkey);
                        return;
                    }
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    if let Some(scancode) = keymap::mac_keycode(code) {
                        let pressed = event.state == ElementState::Pressed;
                        if pressed {
                            if !self.keys_down.contains(&scancode) {
                                self.keys_down.push(scancode);
                            }
                        } else {
                            self.keys_down.retain(|&key| key != scancode);
                        }
                        let _ = self.input.send(InputEvent::Key {
                            scancode,
                            pressed,
                            repeat: event.repeat,
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_line() {
        assert_eq!(stats_text(&LiveStats::default()), "connecting…");
        let live = LiveStats {
            codec: "hevc".into(),
            width: 3360,
            height: 2100,
            fps: 60,
            mbps: 12.34,
            latency_ms: Some((55.2, 70.9)),
            decode_ms_p50: Some(7.25),
            rtt_ms: Some(14.0),
            host: Some(sunna_proto::messages::HostStats {
                target_kbps: 20_000,
                encode_us_p50: 26_100,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            stats_text(&live),
            "HEVC 3360×2100  ·  60 fps  ·  12.3 / 20 Mbps  ·  latency 55 ms (p95 71)  ·  \
             encode 26.1 ms  ·  decode 7.2 ms  ·  rtt 14 ms  ·  ⌃⌥S hide"
        );
    }
}
