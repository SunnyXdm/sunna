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

use sunna_codec::DecodedFrame;
use sunna_proto::messages::InputEvent;
use tokio::sync::mpsc::UnboundedSender;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

use crate::keymap;
#[cfg(target_os = "macos")]
use crate::layer_presenter::LayerPresenter;

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
    let mut app = ViewerApp {
        shared,
        input,
        title,
        stream_size: (width.max(1), height.max(1)),
        window: None,
        surface: None,
        x_lut: Vec::new(),
        lut_key: (0, 0),
        keys_down: Vec::new(),
        #[cfg(target_os = "macos")]
        layer: None,
        presented: 0,
        present_window: Instant::now(),
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

struct ViewerApp {
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
    /// Zero-copy presenter; when set, softbuffer isn't used.
    #[cfg(target_os = "macos")]
    layer: Option<LayerPresenter>,
    presented: u64,
    present_window: Instant,
}

impl ViewerApp {
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
        // Matches the layer presenter: 1:1 when the stream fits, otherwise
        // scaled down to fit.
        let scale = (win_w / stream_w).min(win_h / stream_h).min(1.0);
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
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: FrameReady) {
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
            WindowEvent::Focused(false) => {
                for scancode in std::mem::take(&mut self.keys_down) {
                    let _ = self.input.send(InputEvent::Key {
                        scancode,
                        pressed: false,
                        repeat: false,
                    });
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
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
