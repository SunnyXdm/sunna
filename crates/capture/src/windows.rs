//! Windows capture: DXGI Desktop Duplication (research/03 §1).
//!
//! Each new desktop image is copied on the GPU into a texture of our own (so
//! the duplication frame goes back to Windows at once), then the GPU's video
//! processor scales it to the stream's size and converts it to NV12, BT.709
//! video range: what the encoder takes. Pixels never touch system memory on
//! that path. The raw codec and the fast lane need CPU bytes; for them the
//! processor writes BGRA, which is read back.
//!
//! The pointer isn't in the duplicated image: the viewer draws its own.
//! Windows.Graphics.Capture (hybrid-GPU laptops, single windows) and HDR
//! (FP16 → P010) come later.

use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use bytes::Bytes;
use windows::core::Interface;
use windows::Win32::Foundation::{HANDLE, HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::Power::{
    SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
};
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, GetUserObjectInformationW, OpenInputDesktop, SetThreadDesktop,
    DESKTOP_ACCESS_FLAGS, DESKTOP_CONTROL_FLAGS, HDESK, UOI_NAME,
};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEINPUT,
};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, HWND_BROADCAST, SC_MONITORPOWER, WM_SYSCOMMAND};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};

use crate::{FrameData, FrameSource, NoPicture, PixelFormat, VideoFrame};

/// How long to wait for the desktop to change before sending the last
/// image again. Repeats keep a still screen's stream alive: a viewer that
/// lost a frame or asked for a keyframe gets one within this time.
const REPEAT_AFTER: Duration = Duration::from_millis(100);
/// With no new image for this long, duplicate the display again: a fresh
/// duplication hands over the current image at once if Windows is drawing
/// the display, so a still desktop answers and a dead one doesn't.
const PROBE_AFTER: Duration = Duration::from_secs(2);
/// A fresh duplication with no image after this long: the display isn't
/// being drawn (switched off, or its driver stopped presenting).
const PROBE_GRACE: Duration = Duration::from_secs(1);
/// Converted textures in flight (capture → encoder) before a new one is made.
const POOL: usize = 4;
/// `GENERIC_ALL` for `OpenInputDesktop`.
const GENERIC_ALL: u32 = 0x1000_0000;

/// A captured frame on the GPU: a texture of the capture's D3D11 device,
/// which the encoder must use too. The texture is the encoder's to read
/// until this value (and its clones) are dropped; then capture reuses it.
#[derive(Clone)]
pub struct D3dFrame {
    pub texture: ID3D11Texture2D,
    pub device: ID3D11Device,
    _lease: Arc<()>,
}

impl std::fmt::Debug for D3dFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D3dFrame").finish_non_exhaustive()
    }
}

// SAFETY: D3D11 devices and textures are free-threaded; the capture device
// is multithread-protected, so its immediate context serializes callers.
unsafe impl Send for D3dFrame {}
unsafe impl Sync for D3dFrame {}

impl D3dFrame {
    /// Read the texture back as tightly packed rows (BGRA), or NV12's Y rows
    /// then its interleaved UV rows. Slow: tests and diagnostics only.
    pub fn to_bytes(&self) -> anyhow::Result<Bytes> {
        // SAFETY: valid texture and device; the staging copy is ours alone.
        unsafe {
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            self.texture.GetDesc(&mut desc);
            let rows = match desc.Format {
                DXGI_FORMAT_NV12 => desc.Height as usize * 3 / 2,
                _ => desc.Height as usize,
            };
            let row_bytes = match desc.Format {
                DXGI_FORMAT_NV12 => desc.Width as usize,
                _ => desc.Width as usize * 4,
            };
            let staging_desc = D3D11_TEXTURE2D_DESC {
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
                ..desc
            };
            let mut staging = None;
            self.device
                .CreateTexture2D(&staging_desc, None, Some(&mut staging))
                .context("creating a staging texture")?;
            let staging = staging.context("no staging texture")?;
            let context = self.device.GetImmediateContext()?;
            context.CopyResource(&staging, &self.texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .context("mapping the staging texture")?;
            let mut out = Vec::with_capacity(rows * row_bytes);
            for row in 0..rows {
                let start = (mapped.pData as *const u8).add(row * mapped.RowPitch as usize);
                out.extend_from_slice(std::slice::from_raw_parts(start, row_bytes));
            }
            context.Unmap(&staging, 0);
            Ok(Bytes::from(out))
        }
    }
}

/// One display Windows knows, as DXGI lists it.
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    pub adapter: String,
    pub vendor_id: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
    pub attached: bool,
}

/// Every output of every adapter, in DXGI order.
pub fn displays() -> anyhow::Result<Vec<DisplayInfo>> {
    let mut found = Vec::new();
    for (adapter, output) in outputs()? {
        // SAFETY: plain descriptor queries on live DXGI objects.
        let (adapter_desc, desc) = unsafe { (adapter.GetDesc1()?, output.GetDesc()?) };
        let rect = desc.DesktopCoordinates;
        found.push(DisplayInfo {
            adapter: wide(&adapter_desc.Description),
            vendor_id: adapter_desc.VendorId,
            name: wide(&desc.DeviceName),
            width: (rect.right - rect.left) as u32,
            height: (rect.bottom - rect.top) as u32,
            left: rect.left,
            top: rect.top,
            attached: desc.AttachedToDesktop.as_bool(),
        });
    }
    Ok(found)
}

/// The size of the display Sunna shares (see [`pick_output`]).
pub fn main_display_pixel_size() -> anyhow::Result<(u32, u32)> {
    let (_, output) = pick_output()?;
    // SAFETY: descriptor query on a live output.
    let rect = unsafe { output.GetDesc()? }.DesktopCoordinates;
    Ok(((rect.right - rect.left) as u32, (rect.bottom - rect.top) as u32))
}

fn wide(text: &[u16]) -> String {
    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    String::from_utf16_lossy(&text[..end])
}

fn outputs() -> anyhow::Result<Vec<(IDXGIAdapter1, IDXGIOutput)>> {
    // SAFETY: DXGI enumeration; errors end each list.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().context("CreateDXGIFactory1")?;
        let mut found = Vec::new();
        let mut a = 0;
        while let Ok(adapter) = factory.EnumAdapters1(a) {
            let mut o = 0;
            while let Ok(output) = adapter.EnumOutputs(o) {
                found.push((adapter.clone(), output));
                o += 1;
            }
            a += 1;
        }
        Ok(found)
    }
}

/// The display to capture whenever it's attached (Sunna's own virtual
/// display), ahead of `SUNNA_DISPLAY` and the primary one.
static PREFERRED: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Capture `name` (a GDI name like `\\.\DISPLAY7`) when it's attached.
pub fn prefer_display(name: Option<String>) {
    *PREFERRED.lock().unwrap() = name;
}

/// The preferred display if attached; else `SUNNA_DISPLAY=<n>` picks the
/// n-th attached display (0-based, in [`displays`] order); otherwise the
/// primary display, the one at (0, 0).
fn pick_output() -> anyhow::Result<(IDXGIAdapter1, IDXGIOutput)> {
    let attached: Vec<_> = outputs()?
        .into_iter()
        .filter(|(_, output)| {
            // SAFETY: descriptor query on a live output.
            unsafe { output.GetDesc() }.is_ok_and(|desc| desc.AttachedToDesktop.as_bool())
        })
        .collect();
    let preferred = PREFERRED.lock().unwrap().clone();
    if let Some(index) = preferred.and_then(|name| {
        attached.iter().position(|(_, output)| {
            // SAFETY: descriptor query on a live output.
            unsafe { output.GetDesc() }.is_ok_and(|desc| wide(&desc.DeviceName) == name)
        })
    }) {
        return Ok(attached.into_iter().nth(index).unwrap());
    }
    if let Some(index) = std::env::var("SUNNA_DISPLAY").ok().and_then(|v| v.parse::<usize>().ok()) {
        return attached
            .into_iter()
            .nth(index)
            .with_context(|| format!("SUNNA_DISPLAY={index}: no such display"));
    }
    let primary = attached.iter().position(|(_, output)| {
        // SAFETY: descriptor query on a live output.
        unsafe { output.GetDesc() }.is_ok_and(|desc| {
            let RECT { left, top, .. } = desc.DesktopCoordinates;
            left == 0 && top == 0
        })
    });
    attached
        .into_iter()
        .nth(primary.unwrap_or(0))
        .context("no display is attached to the desktop (a monitor, a dummy plug or a virtual display is needed)")
}

/// Per-monitor DPI awareness: `DuplicateOutput1` refuses processes without it.
fn dpi_aware() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: process-wide setting; fails harmlessly if already set.
        let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    });
}

/// Ask Windows to keep the display (and the PC) on, as a video player does:
/// a display that has gone to sleep isn't drawn, so there's nothing to
/// capture. Lasts while the calling thread lives: call it from the thread
/// that captures.
fn keep_display_on() {
    // SAFETY: plain flag call on the current thread.
    unsafe {
        SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED);
    }
}

/// Turn the displays back on if Windows has switched them off for idleness
/// (it stops drawing them then, so capture gets nothing). `SUNNA_WAKE=nudge`
/// also sends a zero-distance mouse move, which wakes them where the power
/// message alone doesn't; the pointer doesn't move.
fn wake_displays() {
    // SAFETY: a posted broadcast message and an optional zero-move input.
    unsafe {
        let _ = PostMessageW(Some(HWND_BROADCAST), WM_SYSCOMMAND, WPARAM(SC_MONITORPOWER as usize), LPARAM(-1));
        if std::env::var("SUNNA_WAKE").as_deref() == Ok("nudge") {
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: 0, dwFlags: MOUSEEVENTF_MOVE, time: 0, dwExtraInfo: 0 },
                },
            };
            SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        }
    }
}

/// Follow the desktop that has the keyboard: the normal one, or the secure
/// one (lock screen, UAC prompt, Ctrl+Alt+Del) when that's showing. Only a
/// process running as SYSTEM may open the secure desktop. Cheap enough to
/// call often: it switches this thread only when the desktop changed.
pub(crate) fn follow_input_desktop() {
    thread_local! {
        static CURRENT: std::cell::Cell<Option<HDESK>> = const { std::cell::Cell::new(None) };
    }
    // SAFETY: desktop handles opened here are closed here, except the one
    // this thread is set to, which is closed when it's replaced.
    unsafe {
        let Ok(input) = OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_ACCESS_FLAGS(GENERIC_ALL)) else {
            return;
        };
        let current = CURRENT.with(|c| c.get());
        if current.is_some_and(|current| desktop_name(current) == desktop_name(input)) {
            let _ = CloseDesktop(input);
            return;
        }
        if SetThreadDesktop(input).is_ok() {
            if let Some(old) = current {
                let _ = CloseDesktop(old);
            }
            CURRENT.with(|c| c.set(Some(input)));
        } else {
            let _ = CloseDesktop(input);
        }
    }
}

fn desktop_name(desktop: HDESK) -> String {
    let mut name = [0u16; 64];
    let mut needed = 0;
    // SAFETY: the buffer and its size match.
    let _ = unsafe {
        GetUserObjectInformationW(
            HANDLE(desktop.0),
            UOI_NAME,
            Some(name.as_mut_ptr().cast()),
            std::mem::size_of_val(&name) as u32,
            Some(&mut needed),
        )
    };
    wide(&name)
}

/// A D3D11 device on `adapter`, safe to share between the capture and
/// encoder threads.
pub fn create_device(adapter: &IDXGIAdapter1) -> anyhow::Result<ID3D11Device> {
    let mut device = None;
    // SAFETY: standard device creation; out-pointer is a local.
    unsafe {
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
        .context("D3D11CreateDevice")?;
    }
    let device = device.context("D3D11CreateDevice returned no device")?;
    // SAFETY: interface query and flag set on a live device.
    unsafe {
        let _ = device.cast::<ID3D11Multithread>()?.SetMultithreadProtected(true);
    }
    Ok(device)
}

/// A D3D11 device on the first adapter from `vendor_id` (0x10DE NVIDIA,
/// 0x1002 AMD, 0x8086 Intel), for probing encoders before any capture.
pub fn device_for_vendor(vendor_id: u32) -> anyhow::Result<ID3D11Device> {
    // SAFETY: DXGI enumeration.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut a = 0;
        while let Ok(adapter) = factory.EnumAdapters1(a) {
            if adapter.GetDesc1()?.VendorId == vendor_id {
                return create_device(&adapter);
            }
            a += 1;
        }
    }
    bail!("no adapter from vendor {vendor_id:#06x}")
}

/// A display can be duplicated only once per process, but the host opens
/// a session's new capture (new size or codec) before closing the old one.
/// So each capture registers its duplication here by display, and a new one
/// releases any other on the same display first.
#[derive(Clone)]
struct Duplication(IDXGIOutputDuplication);
// SAFETY: DXGI objects are free-threaded; access is serialized by the mutex.
unsafe impl Send for Duplication {}
type Slot = Arc<std::sync::Mutex<Option<Duplication>>>;
/// Each capture's slot, by display name.
type Owners = Vec<(String, std::sync::Weak<std::sync::Mutex<Option<Duplication>>>)>;
static OWNERS: std::sync::Mutex<Owners> = std::sync::Mutex::new(Vec::new());

fn take_over(display: &str, mine: &Slot) {
    let mut owners = OWNERS.lock().unwrap();
    owners.retain(|(_, owner)| owner.strong_count() > 0);
    for (name, owner) in owners.iter() {
        if let Some(owner) = owner.upgrade().filter(|owner| name == display && !Arc::ptr_eq(owner, mine)) {
            *owner.lock().unwrap() = None;
        }
    }
    if !owners.iter().any(|(_, owner)| owner.upgrade().is_some_and(|owner| Arc::ptr_eq(&owner, mine))) {
        owners.push((display.to_string(), Arc::downgrade(mine)));
    }
}

struct PoolSlot {
    texture: ID3D11Texture2D,
    view: ID3D11VideoProcessorOutputView,
    lease: Arc<()>,
}

/// The video processor and its output pool, for one input size.
struct Converter {
    input_size: (u32, u32),
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    /// Our copy of the desktop image (the processor reads from it).
    input: ID3D11Texture2D,
    input_view: ID3D11VideoProcessorInputView,
    slots: Vec<PoolSlot>,
}

pub struct ScreenSource {
    output: IDXGIOutput,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    video: ID3D11VideoDevice,
    video_context: ID3D11VideoContext,
    duplication: Slot,
    /// The display's name, e.g. `\\.\DISPLAY1`.
    output_name: String,
    converter: Option<Converter>,
    width: u32,
    height: u32,
    fps: u32,
    format: DXGI_FORMAT,
    /// Read frames back to the CPU (raw codec, fast lane, software encoder).
    cpu: bool,
    frame_id: u64,
    last: Option<FrameData>,
    min_interval: Duration,
    last_delivered: Instant,
    /// When the desktop last gave a new image (repeats don't count).
    last_image: Instant,
    /// When the display was last duplicated again to check it's drawn.
    probed: Option<Instant>,
    /// Since when it hasn't been: no picture is sent meanwhile.
    stalled_since: Option<Instant>,
    /// Nothing acquired from the current duplication yet: its first image is
    /// the whole desktop, even when it reports no new present.
    fresh: bool,
    stats: Stats,
}

/// Why no new image came, for diagnosing a display that isn't being drawn.
#[derive(Debug, Default)]
struct Stats {
    timeouts: u64,
    pointer_only: u64,
    lost: u64,
    /// Why duplicating the display last failed, if it did.
    failed: Option<String>,
}

// SAFETY: COM objects here are free-threaded and the device is
// multithread-protected; the source is used by one thread at a time.
unsafe impl Send for ScreenSource {}

impl ScreenSource {
    /// Capture the display chosen by [`pick_output`], scaled to
    /// `width`×`height` (default: its own size). `cpu` delivers BGRA bytes
    /// instead of NV12 textures.
    pub fn new(width: Option<u32>, height: Option<u32>, fps: u32, cpu: bool) -> anyhow::Result<Self> {
        dpi_aware();
        wake_displays();
        follow_input_desktop();
        let (adapter, output) = pick_output()?;
        // SAFETY: descriptor queries on live DXGI objects.
        let (adapter_desc, output_desc) = unsafe { (adapter.GetDesc1()?, output.GetDesc()?) };
        let rect = output_desc.DesktopCoordinates;
        let native = ((rect.right - rect.left) as u32, (rect.bottom - rect.top) as u32);
        let width = width.unwrap_or(native.0).max(2) & !1;
        let height = height.unwrap_or(native.1).max(2) & !1;
        // The device must be on the adapter that drives the output, or
        // duplication fails (or, on hybrid laptops, crosses GPUs).
        let device = create_device(&adapter)?;
        // SAFETY: interface queries on the live device.
        let (context, video, video_context) = unsafe {
            let context = device.GetImmediateContext()?;
            let video: ID3D11VideoDevice = device.cast().context("no D3D11 video device")?;
            let video_context: ID3D11VideoContext = context.cast()?;
            (context, video, video_context)
        };
        tracing::info!(
            adapter = %wide(&adapter_desc.Description),
            display = %wide(&output_desc.DeviceName),
            native = format!("{}x{}", native.0, native.1),
            stream = format!("{width}x{height}@{fps}"),
            cpu,
            "Windows desktop duplication"
        );
        let mut source = Self {
            output,
            device,
            context,
            video,
            video_context,
            duplication: Arc::new(std::sync::Mutex::new(None)),
            output_name: wide(&output_desc.DeviceName),
            converter: None,
            width,
            height,
            fps: fps.max(1),
            format: if cpu { DXGI_FORMAT_B8G8R8A8_UNORM } else { DXGI_FORMAT_NV12 },
            cpu,
            frame_id: 0,
            last: None,
            // A little under the frame period, so capture doesn't drift
            // behind a display that refreshes at exactly the stream rate.
            min_interval: Duration::from_secs_f64(0.9 / fps.max(1) as f64),
            last_delivered: Instant::now() - Duration::from_secs(1),
            last_image: Instant::now(),
            probed: None,
            stalled_since: None,
            fresh: true,
            stats: Stats::default(),
        };
        source.duplicate()?;
        Ok(source)
    }

    fn duplicate(&mut self) -> anyhow::Result<()> {
        *self.duplication.lock().unwrap() = None;
        take_over(&self.output_name, &self.duplication);
        let output5: IDXGIOutput5 = self.output.cast().context("IDXGIOutput5 (Windows 10 1703+)")?;
        // SAFETY: device lives on the output's adapter.
        let duplication = unsafe {
            output5.DuplicateOutput1(&self.device, 0, &[DXGI_FORMAT_B8G8R8A8_UNORM])
        };
        let duplication = match duplication {
            Ok(duplication) => duplication,
            Err(error) if error.code() == E_ACCESSDENIED_HR => {
                // The secure desktop is showing and we aren't on it yet.
                follow_input_desktop();
                // SAFETY: as above.
                unsafe { output5.DuplicateOutput1(&self.device, 0, &[DXGI_FORMAT_B8G8R8A8_UNORM]) }
                    .context("DuplicateOutput1 (the lock screen or a UAC prompt needs Sunna running as SYSTEM)")?
            }
            Err(error) => return Err(error).context("DuplicateOutput1"),
        };
        // SAFETY: descriptor query on the live duplication.
        let mode = unsafe { duplication.GetDesc() }.ModeDesc;
        // Once per capture: later ones are restarts.
        if self.frame_id == 0 && mode.RefreshRate.Denominator > 0 {
            tracing::info!(
                display = %self.output_name,
                refresh_hz = format!("{:.2}", mode.RefreshRate.Numerator as f64 / mode.RefreshRate.Denominator as f64),
                size = format!("{}x{}", mode.Width, mode.Height),
                "duplicating"
            );
        }
        *self.duplication.lock().unwrap() = Some(Duplication(duplication));
        self.fresh = true;
        Ok(())
    }

    /// Check the display is still drawn: wake it, and duplicate it again
    /// (see `PROBE_AFTER`). If another display has taken its place, or this
    /// one can't be duplicated any more, start over on the current one.
    fn probe(&mut self) {
        wake_displays();
        follow_input_desktop();
        self.probed = Some(Instant::now());
        let name = pick_output()
            .ok()
            // SAFETY: descriptor query on a live output.
            .and_then(|(_, output)| unsafe { output.GetDesc() }.ok())
            .map(|desc| wide(&desc.DeviceName));
        // A display that can't be duplicated any more may have been removed
        // and added again under the same name: start over on it too.
        if name.is_some() && (name.as_ref() != Some(&self.output_name) || self.stats.failed.is_some()) {
            tracing::info!(from = %self.output_name, to = ?name, "display replaced or unusable; starting capture over");
            let mut fresh = match Self::new(Some(self.width), Some(self.height), self.fps, self.cpu) {
                Ok(fresh) => fresh,
                Err(error) => {
                    self.stats.failed = Some(format!("{error:#}"));
                    return;
                }
            };
            fresh.frame_id = self.frame_id;
            fresh.probed = self.probed;
            fresh.stalled_since = self.stalled_since;
            fresh.last_image = self.last_image;
            *self = fresh;
            return;
        }
        *self.duplication.lock().unwrap() = None;
    }

    fn converter(&mut self, input_size: (u32, u32)) -> anyhow::Result<&mut Converter> {
        if self.converter.as_ref().is_none_or(|c| c.input_size != input_size) {
            self.converter = None;
            self.converter = Some(self.new_converter(input_size)?);
        }
        Ok(self.converter.as_mut().unwrap())
    }

    fn new_converter(&self, input_size: (u32, u32)) -> anyhow::Result<Converter> {
        let rate = DXGI_RATIONAL { Numerator: self.fps, Denominator: 1 };
        let content = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: rate,
            InputWidth: input_size.0,
            InputHeight: input_size.1,
            OutputFrameRate: rate,
            OutputWidth: self.width,
            OutputHeight: self.height,
            Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
        };
        // SAFETY: D3D11 object creation with locals for every descriptor.
        unsafe {
            let enumerator = self
                .video
                .CreateVideoProcessorEnumerator(&content)
                .context("CreateVideoProcessorEnumerator")?;
            let processor = self
                .video
                .CreateVideoProcessor(&enumerator, 0)
                .context("CreateVideoProcessor")?;
            let input = self.texture(input_size, DXGI_FORMAT_B8G8R8A8_UNORM, D3D11_BIND_RENDER_TARGET.0 as u32)?;
            let input_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: 0 },
                },
            };
            let mut input_view = None;
            self.video
                .CreateVideoProcessorInputView(&input, &enumerator, &input_desc, Some(&mut input_view))
                .context("CreateVideoProcessorInputView")?;
            // Full-range sRGB in; BT.709 video range out (what the encoders'
            // VUI says). Same matrix the other hosts use.
            if let Ok(context1) = self.video_context.cast::<ID3D11VideoContext1>() {
                context1.VideoProcessorSetStreamColorSpace1(&processor, 0, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709);
                context1.VideoProcessorSetOutputColorSpace1(
                    &processor,
                    if self.cpu { DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709 } else { DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709 },
                );
            }
            // No driver "enhancements" (sharpening, contrast) on desktop pixels.
            self.video_context.VideoProcessorSetStreamAutoProcessingMode(&processor, 0, false);
            self.video_context.VideoProcessorSetStreamFrameFormat(&processor, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
            Ok(Converter {
                input_size,
                enumerator,
                processor,
                input,
                input_view: input_view.context("no input view")?,
                slots: Vec::new(),
            })
        }
    }

    fn texture(&self, size: (u32, u32), format: DXGI_FORMAT, bind: u32) -> anyhow::Result<ID3D11Texture2D> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: size.0,
            Height: size.1,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: bind,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut texture = None;
        // SAFETY: descriptor and out-pointer are locals.
        unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut texture)) }
            .with_context(|| format!("CreateTexture2D {}x{} {format:?}", size.0, size.1))?;
        texture.context("no texture")
    }

    /// A pool texture the encoder no longer holds, making one if all are busy.
    fn free_slot(&mut self, input_size: (u32, u32)) -> anyhow::Result<usize> {
        self.converter(input_size)?;
        let converter = self.converter.as_ref().unwrap();
        if let Some(index) = converter.slots.iter().position(|s| Arc::strong_count(&s.lease) == 1) {
            return Ok(index);
        }
        if converter.slots.len() >= POOL * 2 {
            bail!("the encoder is holding {} frames", converter.slots.len());
        }
        let texture = self.texture((self.width, self.height), self.format, D3D11_BIND_RENDER_TARGET.0 as u32)?;
        let desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
            ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
            },
        };
        let mut view = None;
        // SAFETY: texture is a render target on this device.
        unsafe {
            self.video
                .CreateVideoProcessorOutputView(&texture, &converter.enumerator, &desc, Some(&mut view))
                .context("CreateVideoProcessorOutputView")?;
        }
        let converter = self.converter.as_mut().unwrap();
        converter.slots.push(PoolSlot { texture, view: view.context("no output view")?, lease: Arc::new(()) });
        Ok(converter.slots.len() - 1)
    }

    /// Wait for the next desktop image and convert it. `Ok(None)`: nothing
    /// changed within `timeout`.
    fn acquire(&mut self, timeout: Duration) -> anyhow::Result<Option<FrameData>> {
        if self.duplication.lock().unwrap().is_none() {
            // The display may be gone or not drawable for now (removed, a
            // desktop we can't open): that's no picture, not the end.
            if let Err(error) = self.duplicate() {
                self.stats.failed = Some(format!("{error:#}"));
                std::thread::sleep(timeout);
                return Ok(None);
            }
            self.stats.failed = None;
        }
        let Some(Duplication(duplication)) = self.duplication.lock().unwrap().clone() else {
            return Ok(None);
        };
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource = None;
        // SAFETY: out-pointers are locals; a successful acquire is always
        // released below.
        let acquired = unsafe {
            duplication.AcquireNextFrame(timeout.as_millis() as u32, &mut info, &mut resource)
        };
        match acquired {
            Ok(()) => {}
            Err(error) if error.code() == DXGI_ERROR_WAIT_TIMEOUT => {
                self.stats.timeouts += 1;
                return Ok(None);
            }
            Err(error) if error.code() == DXGI_ERROR_ACCESS_LOST || error.code() == DXGI_ERROR_INVALID_CALL => {
                // Mode change, full-screen app switch, or the desktop
                // changed (lock screen, UAC): duplicate again.
                self.stats.lost += 1;
                tracing::info!(code = %error.code(), "desktop duplication lost; restarting it");
                *self.duplication.lock().unwrap() = None;
                follow_input_desktop();
                std::thread::sleep(Duration::from_millis(20));
                return Ok(None);
            }
            Err(error) => return Err(error).context("AcquireNextFrame"),
        }
        let result = self.convert(&info, resource);
        // SAFETY: matches the successful AcquireNextFrame above.
        let _ = unsafe { duplication.ReleaseFrame() };
        result
    }

    fn convert(&mut self, info: &DXGI_OUTDUPL_FRAME_INFO, resource: Option<IDXGIResource>) -> anyhow::Result<Option<FrameData>> {
        // Only the pointer moved: no new image. A new duplication's first
        // surface, though, holds the whole desktop: use it.
        let fresh = std::mem::replace(&mut self.fresh, false);
        if info.LastPresentTime == 0 && !fresh {
            self.stats.pointer_only += 1;
            return Ok(None);
        }
        let desktop: ID3D11Texture2D = resource.context("no desktop image")?.cast()?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: descriptor query on a live texture.
        unsafe { desktop.GetDesc(&mut desc) };
        let input_size = (desc.Width, desc.Height);
        let slot = self.free_slot(input_size)?;
        let converter = self.converter.as_ref().unwrap();
        let slot = &converter.slots[slot];
        let stream = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: true.into(),
            OutputIndex: 0,
            InputFrameOrField: 0,
            PastFrames: 0,
            FutureFrames: 0,
            ppPastSurfaces: std::ptr::null_mut(),
            pInputSurface: std::mem::ManuallyDrop::new(Some(converter.input_view.clone())),
            ppFutureSurfaces: std::ptr::null_mut(),
            ppPastSurfacesRight: std::ptr::null_mut(),
            pInputSurfaceRight: std::mem::ManuallyDrop::new(None),
            ppFutureSurfacesRight: std::ptr::null_mut(),
        };
        // SAFETY: all resources belong to this device; the stream's view
        // reference is dropped explicitly after the blit.
        let blit = unsafe {
            self.context.CopyResource(&converter.input, &desktop);
            let blit = self.video_context.VideoProcessorBlt(&converter.processor, &slot.view, 0, std::slice::from_ref(&stream));
            self.context.Flush();
            let _ = std::mem::ManuallyDrop::into_inner(stream.pInputSurface);
            blit
        };
        blit.context("VideoProcessorBlt")?;
        let frame = D3dFrame {
            texture: slot.texture.clone(),
            device: self.device.clone(),
            _lease: Arc::clone(&slot.lease),
        };
        if self.cpu {
            return Ok(Some(FrameData::Cpu(frame.to_bytes()?)));
        }
        Ok(Some(FrameData::D3d11(frame)))
    }
}

/// `E_ACCESSDENIED`, as DuplicateOutput returns it on the secure desktop.
const E_ACCESSDENIED_HR: windows::core::HRESULT = windows::core::HRESULT(0x8007_0005_u32 as i32);

impl FrameSource for ScreenSource {
    fn next_frame(&mut self) -> anyhow::Result<VideoFrame> {
        // On the thread that captures (the request is per thread, and the
        // source may have been made on another).
        if self.frame_id == 0 {
            keep_display_on();
        }
        // Cap the rate at the stream's: a 144 Hz desktop streamed at 60.
        let since = self.last_delivered.elapsed();
        if since < self.min_interval {
            std::thread::sleep(self.min_interval - since);
        }
        let started = Instant::now();
        let data = loop {
            if self.last_image.elapsed() >= PROBE_AFTER
                && self.probed.is_none_or(|probed| probed.elapsed() >= PROBE_AFTER)
            {
                self.probe();
            }
            // Stays stalled through later probes, until an image comes.
            let stalled = self.stalled_since.is_some()
                || self
                    .probed
                    .is_some_and(|probed| probed > self.last_image && probed.elapsed() >= PROBE_GRACE);
            if stalled {
                if self.stalled_since.is_none() {
                    self.stalled_since = Some(Instant::now());
                    tracing::warn!(
                        display = %self.output_name,
                        stats = ?self.stats,
                        "Windows isn't drawing this display: no picture until it does"
                    );
                }
                // Hand control back now and then, so the session can end.
                if started.elapsed() >= Duration::from_secs(1) {
                    return Err(NoPicture.into());
                }
            } else if started.elapsed() >= REPEAT_AFTER {
                if let Some(last) = &self.last {
                    break last.clone();
                }
            }
            let wait = if stalled || self.last.is_none() {
                REPEAT_AFTER
            } else {
                REPEAT_AFTER.saturating_sub(started.elapsed()).max(Duration::from_millis(1))
            };
            if let Some(data) = self.acquire(wait)? {
                self.last_image = Instant::now();
                if let Some(since) = self.stalled_since.take() {
                    tracing::info!(after_s = since.elapsed().as_secs(), "the display is drawn again");
                }
                break data;
            }
        };
        self.last = Some(data.clone());
        self.last_delivered = Instant::now();
        let frame = VideoFrame {
            frame_id: self.frame_id,
            width: self.width,
            height: self.height,
            format: if self.cpu { PixelFormat::Bgra8 } else { PixelFormat::Nv12 },
            data,
            capture_ts_us: sunna_proto::now_us(),
        };
        self.frame_id += 1;
        Ok(frame)
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn fps(&self) -> u32 {
        self.fps
    }
}
