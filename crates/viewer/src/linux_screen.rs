//! This computer's screen size in pixels (Linux), asked of the display
//! server before any window exists, so the viewer can ask the host for a
//! stream that shows 1:1 (as the Mac does): the primary monitor through
//! XRandR on X11, the first output's current mode on Wayland.

use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, QueueHandle};

pub fn main_display_pixel_size() -> Option<(u32, u32)> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        if let Some(size) = wayland() {
            return Some(size);
        }
    }
    x11()
}

fn x11() -> Option<(u32, u32)> {
    let xlib = x11_dl::xlib::Xlib::open().ok()?;
    let xrandr = x11_dl::xrandr::Xrandr::open().ok()?;
    // SAFETY: our own connection, opened and closed here.
    unsafe {
        let display = (xlib.XOpenDisplay)(std::ptr::null());
        if display.is_null() {
            return None;
        }
        let root = (xlib.XDefaultRootWindow)(display);
        let mut count = 0;
        let monitors = (xrandr.XRRGetMonitors)(display, root, 1, &mut count);
        let mut size = None;
        if !monitors.is_null() && count > 0 {
            let list = std::slice::from_raw_parts(monitors, count as usize);
            let pick = list.iter().find(|m| m.primary != 0).unwrap_or(&list[0]);
            size = Some((pick.width as u32, pick.height as u32));
            (xrandr.XRRFreeMonitors)(monitors);
        }
        if size.is_none() {
            let screen = (xlib.XDefaultScreen)(display);
            size = Some(((xlib.XDisplayWidth)(display, screen) as u32, (xlib.XDisplayHeight)(display, screen) as u32));
        }
        (xlib.XCloseDisplay)(display);
        size.filter(|(w, h)| *w > 0 && *h > 0)
    }
}

#[derive(Default)]
struct Outputs {
    sizes: Vec<(u32, u32)>,
}

fn wayland() -> Option<(u32, u32)> {
    let connection = Connection::connect_to_env().ok()?;
    let mut queue = connection.new_event_queue();
    let handle = queue.handle();
    connection.display().get_registry(&handle, ());
    let mut outputs = Outputs::default();
    queue.roundtrip(&mut outputs).ok()?;
    queue.roundtrip(&mut outputs).ok()?;
    outputs.sizes.into_iter().next()
}

impl Dispatch<wl_registry::WlRegistry, ()> for Outputs {
    fn event(_: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, handle: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, .. } = event {
            if interface == "wl_output" {
                registry.bind::<wl_output::WlOutput, _, _>(name, 2, handle, ());
            }
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for Outputs {
    fn event(outputs: &mut Self, _: &wl_output::WlOutput, event: wl_output::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_output::Event::Mode { flags, width, height, .. } = event {
            let current = matches!(flags, wayland_client::WEnum::Value(f) if f.contains(wl_output::Mode::Current));
            if current && width > 0 && height > 0 {
                outputs.sizes.push((width as u32, height as u32));
            }
        }
    }
}
