//! Stream viewer window: winit + softbuffer CPU blit.
//!
//! Milestone 0 renderer — deliberately the simplest thing that can show pixels:
//! the network thread stores the latest decoded BGRA frame (latest-wins, no
//! queue) and wakes the event loop; redraw nearest-neighbor scales it into the
//! window's buffer. The zero-copy wgpu presenter with VRR-aware pacing
//! replaces this later (research/03 §8); the plumbing shape is already right.

use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use sunna_codec::DecodedFrame;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

/// Shared between the network thread (writer) and the viewer (reader).
#[derive(Default)]
pub struct SharedFrame {
    pub latest: Mutex<Option<DecodedFrame>>,
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
    title: String,
    width: u32,
    height: u32,
) -> anyhow::Result<()> {
    let mut app = ViewerApp {
        shared,
        title,
        stream_size: (width.max(1), height.max(1)),
        window: None,
        surface: None,
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

struct ViewerApp {
    shared: Arc<SharedFrame>,
    title: String,
    stream_size: (u32, u32),
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
}

impl ViewerApp {
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

        // Nearest-neighbor scale BGRA bytes into the 0RGB u32 buffer.
        let (dst_w, dst_h) = (size.width as usize, size.height as usize);
        let (src_w, src_h) = (frame.width as usize, frame.height as usize);
        let src = &frame.data;
        if src.len() < src_w * src_h * 4 {
            return; // malformed frame; never index out of bounds
        }
        for y in 0..dst_h {
            let sy = y * src_h / dst_h;
            let src_row = sy * src_w * 4;
            let dst_row = y * dst_w;
            for x in 0..dst_w {
                let sx = x * src_w / dst_w;
                let i = src_row + sx * 4;
                buffer[dst_row + x] =
                    (src[i + 2] as u32) << 16 | (src[i + 1] as u32) << 8 | src[i] as u32;
            }
        }
        if let Err(error) = buffer.present() {
            tracing::warn!(%error, "present failed");
        }
    }
}

impl ApplicationHandler<FrameReady> for ViewerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(&self.title)
            .with_inner_size(LogicalSize::new(
                self.stream_size.0 as f64,
                self.stream_size.1 as f64,
            ));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                tracing::error!(%error, "failed to create window");
                event_loop.exit();
                return;
            }
        };
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
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::Resized(_) => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}
