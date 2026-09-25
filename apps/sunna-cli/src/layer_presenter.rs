//! macOS presenter: decoded IOSurfaces become the contents of a CALayer.
//!
//! Zero copies from decode to screen: VideoToolbox writes BGRA into an
//! IOSurface, the layer displays that surface, and Core Animation scales it
//! to the window on the GPU (aspect-fit, linear filtering). Contents are set
//! as soon as a frame arrives, not on the next winit redraw.

use std::collections::VecDeque;
use std::ffi::CString;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2::{class, msg_send};
use sunna_capture::macos::SurfaceFrame;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// Frames kept alive after being shown. Core Animation may still be reading
/// a surface for a frame or two after it's replaced; holding it (and its use
/// count) stops the decoder's pool from overwriting it on screen.
const KEEP_ALIVE: usize = 4;

pub struct LayerPresenter {
    layer: Retained<AnyObject>,
    recent: VecDeque<SurfaceFrame>,
}

fn ns_string(text: &str) -> Retained<AnyObject> {
    let text = CString::new(text).expect("no interior NUL");
    unsafe { msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()] }
}

impl LayerPresenter {
    pub fn new(window: &Window) -> anyhow::Result<Self> {
        let RawWindowHandle::AppKit(handle) = window.window_handle()?.as_raw() else {
            anyhow::bail!("not an AppKit window");
        };
        let view = handle.ns_view.as_ptr() as *mut AnyObject;
        let layer: Retained<AnyObject> = unsafe { msg_send![class!(CALayer), new] };
        unsafe {
            let gravity = ns_string("resizeAspect"); // kCAGravityResizeAspect
            let _: () = msg_send![&*layer, setContentsGravity: &*gravity];
            // Layer-hosting view: set the layer first, then turn layers on.
            let _: () = msg_send![view, setLayer: &*layer];
            let _: () = msg_send![view, setWantsLayer: Bool::YES];
        }
        Ok(Self {
            layer,
            recent: VecDeque::with_capacity(KEEP_ALIVE + 1),
        })
    }

    /// Show `frame` now. Must run on the main thread.
    pub fn show(&mut self, frame: SurfaceFrame) {
        unsafe {
            let _: () = msg_send![class!(CATransaction), begin];
            // No implicit cross-fade between frames.
            let _: () = msg_send![class!(CATransaction), setDisableActions: Bool::YES];
            let _: () = msg_send![&*self.layer, setContents: frame.iosurface() as *mut AnyObject];
            let _: () = msg_send![class!(CATransaction), commit];
        }
        self.recent.push_back(frame);
        while self.recent.len() > KEEP_ALIVE {
            self.recent.pop_front();
        }
    }
}
