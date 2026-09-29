//! Alt+Tab, the Super key and the desktop's other shortcuts go to the remote
//! while the viewer is focused (Linux): an active keyboard grab on X11, the
//! keyboard-shortcuts-inhibit protocol on Wayland (the compositor may ask the
//! user once). Keys still arrive through the window, so the viewer's own
//! hotkeys (Ctrl+Alt+G gives the shortcuts back) keep working.

use std::cell::{Cell, RefCell};

use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use winit::window::Window;

enum Backend {
    X11(Box<X11Grab>),
    Wayland(Box<WaylandInhibit>),
}

pub struct KeyboardCapture {
    backend: RefCell<Backend>,
    enabled: Cell<bool>,
    focused: Cell<bool>,
}

impl KeyboardCapture {
    /// Capture is on from the start (as on the Mac); it takes hold while the
    /// window is focused.
    pub fn new(window: &Window) -> anyhow::Result<Self> {
        let display = window.display_handle()?.as_raw();
        let handle = window.window_handle()?.as_raw();
        let backend = match (display, handle) {
            (RawDisplayHandle::Xlib(display), RawWindowHandle::Xlib(handle)) => {
                let display = display.display.ok_or_else(|| anyhow::anyhow!("no X display"))?;
                Backend::X11(Box::new(X11Grab::new(display.as_ptr(), handle.window)?))
            }
            (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(handle)) => {
                Backend::Wayland(Box::new(WaylandInhibit::new(display.display.as_ptr(), handle.surface.as_ptr())?))
            }
            _ => anyhow::bail!("keyboard capture needs X11 or Wayland"),
        };
        let capture = Self { backend: RefCell::new(backend), enabled: Cell::new(true), focused: Cell::new(window.has_focus()) };
        capture.apply();
        Ok(capture)
    }

    pub fn enabled(&self) -> bool {
        self.enabled.get()
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
        self.apply();
    }

    pub fn set_focused(&self, focused: bool) {
        self.focused.set(focused);
        self.apply();
    }

    fn apply(&self) {
        let want = self.enabled.get() && self.focused.get();
        match &mut *self.backend.borrow_mut() {
            Backend::X11(grab) => grab.set(want),
            Backend::Wayland(inhibit) => inhibit.set(want),
        }
    }
}

// ---- X11: an active keyboard grab ---------------------------------------------------

struct X11Grab {
    xlib: x11_dl::xlib::Xlib,
    display: *mut x11_dl::xlib::Display,
    window: u64,
    grabbed: bool,
}

impl X11Grab {
    fn new(display: *mut std::ffi::c_void, window: u64) -> anyhow::Result<Self> {
        let xlib = x11_dl::xlib::Xlib::open().map_err(|error| anyhow::anyhow!("libX11: {error}"))?;
        Ok(Self { xlib, display: display.cast(), window, grabbed: false })
    }

    fn set(&mut self, on: bool) {
        use x11_dl::xlib::{CurrentTime, GrabModeAsync, GrabSuccess, True};
        if on == self.grabbed {
            return;
        }
        // SAFETY: winit's display and our window, used on the main thread.
        unsafe {
            if on {
                let result = (self.xlib.XGrabKeyboard)(self.display, self.window, True, GrabModeAsync, GrabModeAsync, CurrentTime);
                self.grabbed = result == GrabSuccess;
                if !self.grabbed {
                    tracing::warn!(result, "couldn't grab the keyboard (another app holds it)");
                }
            } else {
                (self.xlib.XUngrabKeyboard)(self.display, CurrentTime);
                self.grabbed = false;
            }
            (self.xlib.XFlush)(self.display);
        }
        tracing::info!(grabbed = self.grabbed, "X11 keyboard grab");
    }
}

// ---- Wayland: keyboard-shortcuts-inhibit ----------------------------------------------

use wayland_client::protocol::{wl_registry, wl_seat::WlSeat, wl_surface::WlSurface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::client::zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1 as InhibitManager;
use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::client::zwp_keyboard_shortcuts_inhibitor_v1::{self, ZwpKeyboardShortcutsInhibitorV1 as Inhibitor};

#[derive(Default)]
struct Globals {
    manager: Option<InhibitManager>,
    seat: Option<WlSeat>,
    active: bool,
}

struct WaylandInhibit {
    connection: Connection,
    queue: EventQueue<Globals>,
    globals: Globals,
    surface: WlSurface,
    inhibitor: Option<Inhibitor>,
}

impl WaylandInhibit {
    fn new(display: *mut std::ffi::c_void, surface: *mut std::ffi::c_void) -> anyhow::Result<Self> {
        // SAFETY: winit's live wl_display; we only add our own queue to it.
        let backend = unsafe { wayland_backend::client::Backend::from_foreign_display(display.cast()) };
        let connection = Connection::from_backend(backend);
        let mut queue = connection.new_event_queue();
        let handle = queue.handle();
        connection.display().get_registry(&handle, ());
        let mut globals = Globals::default();
        queue.roundtrip(&mut globals)?;
        anyhow::ensure!(globals.manager.is_some(), "this compositor doesn't let apps take its shortcuts");
        anyhow::ensure!(globals.seat.is_some(), "no Wayland seat");
        // SAFETY: winit's wl_surface for this window, alive as long as the window.
        let id = unsafe { wayland_backend::client::ObjectId::from_ptr(WlSurface::interface(), surface.cast()) }?;
        let surface = WlSurface::from_id(&connection, id)?;
        Ok(Self { connection, queue, globals, surface, inhibitor: None })
    }

    fn set(&mut self, on: bool) {
        if on == self.inhibitor.is_some() {
            return;
        }
        if on {
            if let (Some(manager), Some(seat)) = (&self.globals.manager, &self.globals.seat) {
                self.inhibitor = Some(manager.inhibit_shortcuts(&self.surface, seat, &self.queue.handle(), ()));
            }
        } else if let Some(inhibitor) = self.inhibitor.take() {
            inhibitor.destroy();
        }
        let _ = self.connection.flush();
        let _ = self.queue.dispatch_pending(&mut self.globals);
        tracing::info!(inhibiting = self.inhibitor.is_some(), active = self.globals.active, "Wayland shortcuts inhibit");
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for Globals {
    fn event(globals: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, handle: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, .. } = event {
            match interface.as_str() {
                "zwp_keyboard_shortcuts_inhibit_manager_v1" => globals.manager = Some(registry.bind(name, 1, handle, ())),
                "wl_seat" if globals.seat.is_none() => globals.seat = Some(registry.bind(name, 1, handle, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<InhibitManager, ()> for Globals {
    fn event(_: &mut Self, _: &InhibitManager, _: <InhibitManager as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlSeat, ()> for Globals {
    fn event(_: &mut Self, _: &WlSeat, _: <WlSeat as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<Inhibitor, ()> for Globals {
    fn event(globals: &mut Self, _: &Inhibitor, event: zwp_keyboard_shortcuts_inhibitor_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwp_keyboard_shortcuts_inhibitor_v1::Event::Active => globals.active = true,
            zwp_keyboard_shortcuts_inhibitor_v1::Event::Inactive => globals.active = false,
            _ => {}
        }
    }
}
