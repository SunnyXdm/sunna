//! PipeWire's stable C ABI, loaded only when a portal source is opened.
use crate::pod;
use anyhow::{ensure, Context};
use bytes::Bytes;
use libloading::Library;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::os::fd::{IntoRawFd, OwnedFd};
use std::ptr::{null, null_mut};
use std::sync::{Arc, Condvar, Mutex};
use sunna_capture::cursor::{shape_id, CursorImage};

type Ptr = *mut c_void;
#[repr(C)]
struct Hook {
    next: Ptr,
    prev: Ptr,
    funcs: *const c_void,
    data: Ptr,
    removed: Option<unsafe extern "C" fn(*mut Hook)>,
    private: Ptr,
}
#[repr(C)]
struct Events {
    version: u32,
    destroy: Option<unsafe extern "C" fn(Ptr)>,
    state_changed: Option<unsafe extern "C" fn(Ptr, c_int, c_int, *const c_char)>,
    control_info: Option<unsafe extern "C" fn(Ptr, u32, *const c_void)>,
    io_changed: Option<unsafe extern "C" fn(Ptr, u32, Ptr, u32)>,
    param_changed: Option<unsafe extern "C" fn(Ptr, u32, *const c_void)>,
    add_buffer: Option<unsafe extern "C" fn(Ptr, *mut PwBuffer)>,
    remove_buffer: Option<unsafe extern "C" fn(Ptr, *mut PwBuffer)>,
    process: Option<unsafe extern "C" fn(Ptr)>,
    drained: Option<unsafe extern "C" fn(Ptr)>,
    command: Option<unsafe extern "C" fn(Ptr, *const c_void)>,
    trigger_done: Option<unsafe extern "C" fn(Ptr)>,
}
#[repr(C)]
struct PwBuffer {
    buffer: *mut Buffer,
    user_data: Ptr,
    size: u64,
}
#[repr(C)]
struct Buffer {
    n_metas: u32,
    n_datas: u32,
    metas: *mut Meta,
    datas: *mut Data,
}
#[repr(C)]
struct Meta {
    kind: u32,
    size: u32,
    data: Ptr,
}
#[repr(C)]
struct Data {
    kind: u32,
    flags: u32,
    fd: i64,
    mapoffset: u32,
    maxsize: u32,
    data: Ptr,
    chunk: *mut Chunk,
}
#[repr(C)]
struct Chunk {
    offset: u32,
    size: u32,
    stride: i32,
    flags: i32,
}

macro_rules! api {
    ($($name:ident: $ty:ty),* $(,)?) => {
        struct Api { $($name: $ty,)* _library: Library }
        impl Api {
            unsafe fn load() -> anyhow::Result<Self> {
                let library = Library::new("libpipewire-0.3.so.0").context("PipeWire is missing: install libpipewire-0.3.so.0 to share a Wayland desktop")?;
                Ok(Self { $($name: *library.get(concat!(stringify!($name),"\0").as_bytes())?,)* _library: library })
            }
        }
    }
}
api! {
    pw_init: unsafe extern "C" fn(*mut c_int,*mut *mut *mut c_char),
    pw_thread_loop_new: unsafe extern "C" fn(*const c_char,*const c_void)->Ptr,
    pw_thread_loop_get_loop: unsafe extern "C" fn(Ptr)->Ptr,
    pw_thread_loop_start: unsafe extern "C" fn(Ptr)->c_int,
    pw_thread_loop_stop: unsafe extern "C" fn(Ptr),
    pw_thread_loop_lock: unsafe extern "C" fn(Ptr),
    pw_thread_loop_unlock: unsafe extern "C" fn(Ptr),
    pw_thread_loop_destroy: unsafe extern "C" fn(Ptr),
    pw_context_new: unsafe extern "C" fn(Ptr,Ptr,usize)->Ptr,
    pw_context_destroy: unsafe extern "C" fn(Ptr),
    pw_context_connect_fd: unsafe extern "C" fn(Ptr,c_int,Ptr,usize)->Ptr,
    pw_core_disconnect: unsafe extern "C" fn(Ptr)->c_int,
    pw_properties_new_string: unsafe extern "C" fn(*const c_char)->Ptr,
    pw_stream_new: unsafe extern "C" fn(Ptr,*const c_char,Ptr)->Ptr,
    pw_stream_add_listener: unsafe extern "C" fn(Ptr,*mut Hook,*const Events,Ptr),
    pw_stream_connect: unsafe extern "C" fn(Ptr,c_int,u32,u32,*const *const c_void,u32)->c_int,
    pw_stream_update_params: unsafe extern "C" fn(Ptr,*const *const c_void,u32)->c_int,
    pw_stream_dequeue_buffer: unsafe extern "C" fn(Ptr)->*mut PwBuffer,
    pw_stream_queue_buffer: unsafe extern "C" fn(Ptr,*mut PwBuffer)->c_int,
    pw_stream_disconnect: unsafe extern "C" fn(Ptr)->c_int,
    pw_stream_destroy: unsafe extern "C" fn(Ptr),
}

#[derive(Clone)]
pub struct Frame {
    pub bytes: Bytes,
    pub width: u32,
    pub height: u32,
    pub serial: u64,
}
#[derive(Default)]
pub struct State {
    pub frame: Option<Frame>,
    pub cursor: Option<CursorImage>,
    pub cursor_position: (i32, i32),
    pub error: Option<String>,
}
#[derive(Default)]
pub struct Shared {
    pub state: Mutex<State>,
    pub changed: Condvar,
}
struct Callbacks {
    api: Arc<Api>,
    stream: Ptr,
    shared: Arc<Shared>,
    format: Option<(u32, u32, u32)>,
    serial: u64,
}
impl Callbacks {
    fn fail(&self, error: impl ToString) {
        self.shared.state.lock().unwrap().error = Some(error.to_string());
        self.shared.changed.notify_all();
    }
}
static EVENTS: Events = Events {
    version: 2,
    destroy: None,
    state_changed: Some(state_changed),
    control_info: None,
    io_changed: None,
    param_changed: Some(param_changed),
    add_buffer: None,
    remove_buffer: None,
    process: Some(process),
    drained: None,
    command: None,
    trigger_done: None,
};
unsafe extern "C" fn state_changed(data: Ptr, _old: c_int, state: c_int, error: *const c_char) {
    let cb = &*(data as *const Callbacks);
    if state == -1 {
        let reason = if error.is_null() {
            "stream error".into()
        } else {
            CStr::from_ptr(error).to_string_lossy()
        };
        cb.fail(format!("PipeWire: {reason}"));
    } else if state == 0 {
        cb.fail("PipeWire screen stream disconnected");
    }
}
unsafe extern "C" fn param_changed(data: Ptr, id: u32, param: *const c_void) {
    if id != 4 || param.is_null() {
        return;
    }
    let cb = &mut *(data as *mut Callbacks);
    let len = (param as *const u32).read_unaligned() as usize;
    if len > 65536 {
        cb.fail("oversized PipeWire format");
        return;
    }
    let bytes = std::slice::from_raw_parts(param.cast::<u8>(), len + 8);
    match pod::parse_format(bytes) {
        Ok(format) => {
            cb.format = Some(format);
            let params = pod::buffer_params();
            let pointers: Vec<_> = params.iter().map(|p| p.as_ptr()).collect();
            if (cb.api.pw_stream_update_params)(cb.stream, pointers.as_ptr(), pointers.len() as u32)
                < 0
            {
                cb.fail("PipeWire buffer negotiation failed");
            }
        }
        Err(error) => cb.fail(error),
    }
}
unsafe extern "C" fn process(data: Ptr) {
    let cb = &mut *(data as *mut Callbacks);
    let mut newest: *mut PwBuffer = null_mut();
    loop {
        let buffer = (cb.api.pw_stream_dequeue_buffer)(cb.stream);
        if buffer.is_null() {
            break;
        }
        // Shapes are incremental, so keep metadata even from discarded frames.
        let has_pixels = if let Some(b) = (*buffer).buffer.as_ref() {
            read_metadata(cb, b);
            b.n_datas > 0
                && !b.datas.is_null()
                && (*b.datas).chunk.as_ref().is_some_and(|c| c.size > 0)
        } else {
            false
        };
        if has_pixels {
            if !newest.is_null() {
                (cb.api.pw_stream_queue_buffer)(cb.stream, newest);
            }
            newest = buffer;
        } else {
            (cb.api.pw_stream_queue_buffer)(cb.stream, buffer);
        }
    }
    if newest.is_null() {
        return;
    }
    if let (Some(buffer), Some((format, w, h))) = ((*newest).buffer.as_ref(), cb.format) {
        if buffer.n_datas > 0 && !buffer.datas.is_null() {
            let data = &*buffer.datas;
            if let Some(chunk) = data.chunk.as_ref() {
                if chunk.size > 0
                    && chunk.flags & 1 == 0
                    && !data.data.is_null()
                    && matches!(data.kind, 1 | 2)
                {
                    let memory =
                        std::slice::from_raw_parts(data.data.cast::<u8>(), data.maxsize as usize);
                    match copy_frame(memory, chunk.offset, chunk.size, chunk.stride, w, h, format) {
                        Ok(bytes) => {
                            cb.serial += 1;
                            let mut state = cb.shared.state.lock().unwrap();
                            state.frame = Some(Frame {
                                bytes: Bytes::from(bytes),
                                width: w,
                                height: h,
                                serial: cb.serial,
                            });
                            if let Some(cursor) = &mut state.cursor {
                                cursor.screen_width = w;
                            }
                        }
                        Err(error) => cb.fail(error),
                    }
                }
            }
        }
    }
    (cb.api.pw_stream_queue_buffer)(cb.stream, newest);
    cb.shared.changed.notify_all();
}

fn copy_frame(
    memory: &[u8],
    offset: u32,
    size: u32,
    stride: i32,
    w: u32,
    h: u32,
    format: u32,
) -> anyhow::Result<Vec<u8>> {
    ensure!(!memory.is_empty() && w > 0 && h > 0, "empty PipeWire frame");
    let row = w as usize * 4;
    let rows_in_chunk = size as usize / h as usize;
    let stride = if stride == 0 {
        row as i64
    } else if (stride.unsigned_abs() as usize) < row
        && rows_in_chunk >= row
        && (size as usize).is_multiple_of(h as usize)
    {
        // Mutter now and then gives the stride in pixels (seen during the
        // overview's animation); the chunk's size is right: whole rows.
        rows_in_chunk as i64
    } else {
        stride as i64
    };
    ensure!(
        stride.unsigned_abs() >= row as u64,
        "invalid PipeWire row stride {stride} for {w}x{h} (chunk offset {offset}, size {size}; buffer {})",
        memory.len()
    );
    let needed = stride.unsigned_abs() * (h as u64 - 1) + row as u64;
    ensure!(
        needed <= (size as usize).min(memory.len()) as u64,
        "truncated PipeWire frame"
    );
    let offset = offset as usize % memory.len();
    let mut out = vec![0u8; row * h as usize];
    for y in 0..h as usize {
        let start = offset as i64 + y as i64 * stride;
        ensure!(
            start >= 0 && start as usize + row <= memory.len(),
            "PipeWire frame outside mapped buffer"
        );
        out[y * row..(y + 1) * row].copy_from_slice(&memory[start as usize..start as usize + row]);
    }
    if matches!(format, pod::RGBX | pod::RGBA) {
        for pixel in out.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
    }
    Ok(out)
}
fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}
fn cursor_bitmap(bytes: &[u8], screen_width: u32) -> Option<CursorImage> {
    if u32_at(bytes, 0)? == 0 {
        return None;
    }
    let offset = u32_at(bytes, 24)? as usize;
    if offset < 28 {
        return None;
    }
    let format = u32_at(bytes, offset)?;
    if !matches!(format, pod::BGRA | pod::RGBA | pod::BGRX | pod::RGBX) {
        return None;
    }
    let (w, h) = (u32_at(bytes, offset + 4)?, u32_at(bytes, offset + 8)?);
    let stride = u32_at(bytes, offset + 12)? as i32;
    let pixels = u32_at(bytes, offset + 16)? as usize;
    let hot = (u32_at(bytes, 16)? as i32, u32_at(bytes, 20)? as i32);
    if pixels == 0 {
        return Some(CursorImage {
            id: shape_id(1, 1, (0, 0), &[0; 4]),
            width: 1,
            height: 1,
            hot_x: 0,
            hot_y: 0,
            screen_width,
            rgba: vec![0; 4],
        });
    }
    if w == 0 || h == 0 || w > 256 || h > 256 || pixels < 20 {
        return None;
    }
    let start = offset.checked_add(pixels)?;
    if start >= bytes.len() {
        return None;
    }
    let mut rgba = copy_frame(
        bytes,
        start as u32,
        bytes.len() as u32,
        stride,
        w,
        h,
        format,
    )
    .ok()?;
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
        if matches!(format, pod::BGRX | pod::RGBX) {
            pixel[3] = 255;
        }
    }
    let hot = (
        hot.0.clamp(0, w as i32 - 1) as u16,
        hot.1.clamp(0, h as i32 - 1) as u16,
    );
    Some(CursorImage {
        id: shape_id(w as usize, h as usize, hot, &rgba),
        width: w as u16,
        height: h as u16,
        hot_x: hot.0,
        hot_y: hot.1,
        screen_width,
        rgba,
    })
}
unsafe fn read_metadata(cb: &Callbacks, buffer: &Buffer) {
    if buffer.metas.is_null() {
        return;
    }
    for meta in std::slice::from_raw_parts(buffer.metas, buffer.n_metas as usize) {
        if meta.kind != 5 || meta.size < 28 || meta.data.is_null() {
            continue;
        }
        let bytes = std::slice::from_raw_parts(meta.data.cast::<u8>(), meta.size as usize);
        if u32_at(bytes, 0) == Some(0) {
            continue;
        }
        let mut state = cb.shared.state.lock().unwrap();
        state.cursor_position = (
            u32_at(bytes, 8).unwrap() as i32,
            u32_at(bytes, 12).unwrap() as i32,
        );
        if let Some(image) = cursor_bitmap(bytes, cb.format.map_or(0, |(_, w, _)| w)) {
            state.cursor = Some(image);
        }
    }
}

pub struct PipeWire {
    api: Arc<Api>,
    loop_: Ptr,
    context: Ptr,
    core: Ptr,
    stream: Ptr,
    _hook: Box<Hook>,
    callbacks: Box<Callbacks>,
    running: bool,
    pub shared: Arc<Shared>,
}
// Only the PipeWire loop accesses callbacks; destruction holds its lock.
unsafe impl Send for PipeWire {}
impl PipeWire {
    pub fn new(fd: OwnedFd, node: u32, fps: u32) -> anyhow::Result<Self> {
        unsafe {
            // Keep the library and its process-wide initialization alive together.
            static API: std::sync::OnceLock<Result<Arc<Api>, String>> = std::sync::OnceLock::new();
            let api = API
                .get_or_init(|| {
                    let api = Arc::new(Api::load().map_err(|e| format!("{e:#}"))?);
                    (api.pw_init)(null_mut(), null_mut());
                    Ok(api)
                })
                .as_ref()
                .map_err(|e| anyhow::anyhow!("{e}"))?
                .clone();
            let shared = Arc::new(Shared::default());
            let mut pw = Self {
                api: api.clone(),
                loop_: null_mut(),
                context: null_mut(),
                core: null_mut(),
                stream: null_mut(),
                _hook: Box::new(std::mem::zeroed()),
                callbacks: Box::new(Callbacks {
                    api: api.clone(),
                    stream: null_mut(),
                    shared: shared.clone(),
                    format: None,
                    serial: 0,
                }),
                running: false,
                shared,
            };
            pw.loop_ = (api.pw_thread_loop_new)(c"sunna-pipewire".as_ptr(), null());
            ensure!(!pw.loop_.is_null(), "couldn't create PipeWire loop");
            pw.context =
                (api.pw_context_new)((api.pw_thread_loop_get_loop)(pw.loop_), null_mut(), 0);
            ensure!(!pw.context.is_null(), "couldn't create PipeWire context");
            pw.core = (api.pw_context_connect_fd)(pw.context, fd.into_raw_fd(), null_mut(), 0);
            ensure!(
                !pw.core.is_null(),
                "couldn't connect to portal's PipeWire remote"
            );
            let props = (api.pw_properties_new_string)(
                c"media.type=Video media.category=Capture media.role=Screen".as_ptr(),
            );
            ensure!(!props.is_null(), "couldn't allocate PipeWire properties");
            pw.stream = (api.pw_stream_new)(pw.core, c"Sunna screen".as_ptr(), props);
            ensure!(
                !pw.stream.is_null(),
                "couldn't create PipeWire screen stream"
            );
            pw.callbacks.stream = pw.stream;
            (api.pw_stream_add_listener)(
                pw.stream,
                &mut *pw._hook,
                &EVENTS,
                (&mut *pw.callbacks as *mut Callbacks).cast(),
            );
            let format = pod::enum_format(fps);
            let params = [format.as_ptr()];
            ensure!(
                (api.pw_stream_connect)(pw.stream, 0, node, 1 | 4, params.as_ptr(), 1) >= 0,
                "couldn't connect PipeWire screen stream"
            );
            ensure!(
                (api.pw_thread_loop_start)(pw.loop_) >= 0,
                "couldn't start PipeWire loop"
            );
            pw.running = true;
            Ok(pw)
        }
    }
}
impl Drop for PipeWire {
    fn drop(&mut self) {
        unsafe {
            if self.loop_.is_null() {
                return;
            }
            (self.api.pw_thread_loop_lock)(self.loop_);
            if !self.stream.is_null() {
                (self.api.pw_stream_disconnect)(self.stream);
                (self.api.pw_stream_destroy)(self.stream);
            }
            if !self.core.is_null() {
                (self.api.pw_core_disconnect)(self.core);
            }
            if !self.context.is_null() {
                (self.api.pw_context_destroy)(self.context);
            }
            (self.api.pw_thread_loop_unlock)(self.loop_);
            if self.running {
                (self.api.pw_thread_loop_stop)(self.loop_);
            }
            (self.api.pw_thread_loop_destroy)(self.loop_);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn c_layout_matches_pipewire_0_3_headers() {
        use std::mem::{offset_of, size_of};
        assert_eq!(
            [
                size_of::<Hook>(),
                size_of::<Events>(),
                size_of::<PwBuffer>(),
                size_of::<Buffer>(),
                size_of::<Meta>(),
                size_of::<Data>(),
                size_of::<Chunk>()
            ],
            [48, 96, 24, 24, 16, 40, 16]
        );
        assert_eq!(
            [
                offset_of!(Events, process),
                offset_of!(Events, param_changed),
                offset_of!(Data, data),
                offset_of!(Data, chunk)
            ],
            [64, 40, 24, 32]
        );
    }
    #[test]
    fn padded_offset_and_negative_stride_frames() {
        let data = [9, 9, 9, 9, 1, 2, 3, 4, 0, 0, 0, 0, 5, 6, 7, 8];
        assert_eq!(
            copy_frame(&data, 4, 12, 8, 1, 2, pod::RGBA).unwrap(),
            [3, 2, 1, 4, 7, 6, 5, 8]
        );
        assert_eq!(
            copy_frame(&data, 12, 12, -8, 1, 2, pod::BGRA).unwrap(),
            [5, 6, 7, 8, 1, 2, 3, 4]
        );
        assert!(copy_frame(&data, 8, 12, 8, 1, 2, pod::BGRA).is_err());
    }
    #[test]
    fn a_stride_in_pixels_is_read_from_the_chunk_size() {
        // Two rows of two pixels, tightly packed; the stride says 2 (pixels).
        let data: Vec<u8> = (0..16).collect();
        assert_eq!(copy_frame(&data, 0, 16, 2, 2, 2, pod::BGRX).unwrap(), data);
        // Not whole rows: still refused.
        assert!(copy_frame(&data, 0, 12, 2, 2, 2, pod::BGRX).is_err());
    }
    #[test]
    fn cursor_offsets_and_shape_identity() {
        let mut words = vec![1u32, 0, 100, 200, 0, 0, 32, 0, pod::BGRA, 1, 1, 4, 24, 0];
        let mut bytes: Vec<u8> = words.drain(..).flat_map(u32::to_ne_bytes).collect();
        bytes.extend([10, 20, 30, 128]);
        let first = cursor_bitmap(&bytes, 3840).unwrap();
        assert_eq!(first.rgba, [30, 20, 10, 128]);
        bytes[8..12].copy_from_slice(&300u32.to_ne_bytes());
        assert_eq!(first.id, cursor_bitmap(&bytes, 3840).unwrap().id);
        assert!(cursor_bitmap(&bytes[..bytes.len() - 1], 3840).is_none());
    }
}
