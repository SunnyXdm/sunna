//! NVENC on Linux, on a CUDA context. Each captured BGRA frame is copied to
//! the GPU once and converted to NV12 there by a small kernel that does
//! exactly `yuv::bgra_to_i420`'s BT.709 video-range arithmetic; NVENC then
//! reads that NV12 surface in place. NVENC's own RGB input isn't used: its
//! built-in conversion ignores the colour description and applies BT.601.
//! If the kernel can't be loaded, frames are converted on the CPU into
//! NVENC's own input buffer instead, as before.

use std::ffi::{c_char, c_void, CStr};
use std::ptr;
use std::sync::OnceLock;

use anyhow::{ensure, Context};
use libloading::Library;
use sunna_capture::{PixelFormat, VideoFrame};

use super::*;
use crate::yuv::bgra_to_i420;
use crate::{Codec, EncodedFrame, Encoder};

/// Whether NVENC can really encode `codec` here. The driver libraries being
/// present isn't enough: an old driver (e.g. 470, the last for Kepler cards
/// like the GT 710) or a GPU without that encoder fails in `new`. So this
/// opens a small session once per codec and caches the answer.
pub fn supports(codec: Codec) -> bool {
    static H264: OnceLock<bool> = OnceLock::new();
    static HEVC: OnceLock<bool> = OnceLock::new();
    let cell = match codec {
        Codec::H264 => &H264,
        Codec::Hevc => &HEVC,
        _ => return false,
    };
    *cell.get_or_init(|| {
        // SAFETY: load the standard driver libraries without invoking any symbols.
        let present = unsafe {
            Library::new("libcuda.so.1").is_ok() && Library::new(LIBRARY).is_ok()
        };
        if !present {
            return false;
        }
        match NvencEncoder::new(codec, 640, 360, 30, 2_000_000) {
            Ok(_) => true,
            Err(error) => {
                tracing::info!(?codec, reason = %format!("{error:#}"), "NVENC can't encode this here");
                false
            }
        }
    })
}

/// A device address (`CUdeviceptr`).
type DevicePtr = u64;
type Handle = *mut c_void;

/// The CUDA driver API calls used here.
struct CudaApi {
    ctx_destroy: unsafe extern "C" fn(Handle) -> i32,
    ctx_push: unsafe extern "C" fn(Handle) -> i32,
    ctx_pop: unsafe extern "C" fn(*mut Handle) -> i32,
    ctx_synchronize: unsafe extern "C" fn() -> i32,
    error_name: unsafe extern "C" fn(i32, *mut *const c_char) -> i32,
    mem_alloc: unsafe extern "C" fn(*mut DevicePtr, usize) -> i32,
    mem_alloc_pitch: unsafe extern "C" fn(*mut DevicePtr, *mut usize, usize, usize, u32) -> i32,
    mem_free: unsafe extern "C" fn(DevicePtr) -> i32,
    copy_to_device: unsafe extern "C" fn(DevicePtr, *const c_void, usize) -> i32,
    #[cfg_attr(not(test), allow(dead_code))]
    copy_to_host: unsafe extern "C" fn(*mut c_void, DevicePtr, usize) -> i32,
    module_load: unsafe extern "C" fn(*mut Handle, *const c_void, u32, *mut i32, *mut *mut c_void) -> i32,
    module_unload: unsafe extern "C" fn(Handle) -> i32,
    module_function: unsafe extern "C" fn(*mut Handle, Handle, *const c_char) -> i32,
    #[allow(clippy::type_complexity)]
    launch: unsafe extern "C" fn(
        Handle, u32, u32, u32, u32, u32, u32, u32, Handle, *mut *mut c_void, *mut *mut c_void,
    ) -> i32,
}

struct Cuda {
    context: Handle,
    api: CudaApi,
    _library: Library,
}

impl Cuda {
    fn new() -> anyhow::Result<Self> {
        // SAFETY: symbol signatures match the CUDA driver ABI; the library is retained.
        unsafe {
            let library = Library::new("libcuda.so.1").context("loading libcuda.so.1")?;
            macro_rules! symbol {
                ($name:literal) => {
                    *library
                        .get(concat!($name, "\0").as_bytes())
                        .context(concat!("libcuda has no ", $name))?
                };
            }
            let init: unsafe extern "C" fn(u32) -> i32 = symbol!("cuInit");
            let device_get: unsafe extern "C" fn(*mut i32, i32) -> i32 = symbol!("cuDeviceGet");
            let create: unsafe extern "C" fn(*mut Handle, u32, i32) -> i32 = symbol!("cuCtxCreate_v2");
            let api = CudaApi {
                ctx_destroy: symbol!("cuCtxDestroy_v2"),
                ctx_push: symbol!("cuCtxPushCurrent_v2"),
                ctx_pop: symbol!("cuCtxPopCurrent_v2"),
                ctx_synchronize: symbol!("cuCtxSynchronize"),
                error_name: symbol!("cuGetErrorName"),
                mem_alloc: symbol!("cuMemAlloc_v2"),
                mem_alloc_pitch: symbol!("cuMemAllocPitch_v2"),
                mem_free: symbol!("cuMemFree_v2"),
                copy_to_device: symbol!("cuMemcpyHtoD_v2"),
                copy_to_host: symbol!("cuMemcpyDtoH_v2"),
                module_load: symbol!("cuModuleLoadDataEx"),
                module_unload: symbol!("cuModuleUnload"),
                module_function: symbol!("cuModuleGetFunction"),
                launch: symbol!("cuLaunchKernel"),
            };
            let mut cuda = Self {
                context: ptr::null_mut(),
                api,
                _library: library,
            };
            cuda.check(init(0), "cuInit")?;
            let mut device = 0;
            cuda.check(device_get(&mut device, 0), "cuDeviceGet(0)")?;
            let status = create(&mut cuda.context, 0, device);
            cuda.check(status, "cuCtxCreate_v2")?;
            // Detach from this thread; Encoder: Send permits moving to another one.
            let mut context = ptr::null_mut();
            cuda.check((cuda.api.ctx_pop)(&mut context), "cuCtxPopCurrent_v2")?;
            Ok(cuda)
        }
    }

    fn enter(&self) -> anyhow::Result<CurrentContext<'_>> {
        // SAFETY: this context is owned by self and operations are serialized.
        self.check(unsafe { (self.api.ctx_push)(self.context) }, "cuCtxPushCurrent_v2")?;
        Ok(CurrentContext(self))
    }

    fn check(&self, status: i32, operation: &str) -> anyhow::Result<()> {
        if status == 0 {
            return Ok(());
        }
        let mut name = ptr::null();
        // SAFETY: the driver returns a static string, or leaves NULL for unknown codes.
        let name = unsafe {
            (self.api.error_name)(status, &mut name);
            if name.is_null() {
                "unknown".into()
            } else {
                CStr::from_ptr(name).to_string_lossy()
            }
        };
        anyhow::bail!("{operation} failed: {name} ({status})")
    }
}

impl Drop for Cuda {
    fn drop(&mut self) {
        if !self.context.is_null() {
            // SAFETY: all NVENC and CUDA resources are gone; the driver library is still loaded.
            unsafe { (self.api.ctx_destroy)(self.context) };
        }
    }
}

struct CurrentContext<'a>(&'a Cuda);
impl Drop for CurrentContext<'_> {
    fn drop(&mut self) {
        let mut context = ptr::null_mut();
        // SAFETY: matches enter's push and restores the calling thread's context.
        unsafe { (self.0.api.ctx_pop)(&mut context) };
    }
}

/// BGRA → NV12, one thread per 2×2 block of pixels: four luma samples and
/// the block's averaged chroma pair, with `yuv::bgra_to_i420`'s fixed-point
/// BT.709 video-range constants, so both paths produce identical bytes.
/// Hand-written PTX: the driver compiles it for whatever GPU is present, so
/// no CUDA toolkit is needed to build or run Sunna.
fn kernel_ptx() -> String {
    // One source pixel in `src` → its luma in `luma`, adding R, G, B to the
    // block's sums (%r40..%r42).
    let pixel = |src: &str, luma: &str| {
        format!(
            "	bfe.u32 %r32, {src}, 16, 8;
	bfe.u32 %r33, {src}, 8, 8;
	bfe.u32 %r34, {src}, 0, 8;
	add.u32 %r40, %r40, %r32;
	add.u32 %r41, %r41, %r33;
	add.u32 %r42, %r42, %r34;
	mul.lo.u32 {luma}, %r32, 11966;
	mad.lo.u32 {luma}, %r33, 40254, {luma};
	mad.lo.u32 {luma}, %r34, 4064, {luma};
	add.u32 {luma}, {luma}, 32768;
	shr.u32 {luma}, {luma}, 16;
	add.u32 {luma}, {luma}, 16;
"
        )
    };
    // Chroma from the four-pixel sums: shifted 2 more bits to average,
    // clamped to video range like the CPU path.
    let chroma = |out: &str, [r, g, b]: [i32; 3]| {
        format!(
            "	mul.lo.s32 {out}, %r40, {r};
	mad.lo.s32 {out}, %r41, {g}, {out};
	mad.lo.s32 {out}, %r42, {b}, {out};
	add.s32 {out}, {out}, 131072;
	shr.s32 {out}, {out}, 18;
	add.s32 {out}, {out}, 128;
	max.s32 {out}, {out}, 16;
	min.s32 {out}, {out}, 240;
"
        )
    };
    format!(
        ".version 6.0
.target sm_50
.address_size 64

.visible .entry bgra_to_nv12(
	.param .u64 source,
	.param .u32 source_pitch,
	.param .u64 luma,
	.param .u64 chroma,
	.param .u32 pitch,
	.param .u32 half_width,
	.param .u32 half_height
)
{{
	.reg .pred %p<3>;
	.reg .b32 %r<48>;
	.reg .b64 %rd<16>;

	ld.param.u64 %rd1, [source];
	ld.param.u32 %r1, [source_pitch];
	ld.param.u64 %rd2, [luma];
	ld.param.u64 %rd3, [chroma];
	ld.param.u32 %r2, [pitch];
	ld.param.u32 %r3, [half_width];
	ld.param.u32 %r4, [half_height];
	mov.u32 %r5, %ctaid.x;
	mov.u32 %r6, %ntid.x;
	mov.u32 %r7, %tid.x;
	mad.lo.u32 %r8, %r5, %r6, %r7;
	mov.u32 %r5, %ctaid.y;
	mov.u32 %r6, %ntid.y;
	mov.u32 %r7, %tid.y;
	mad.lo.u32 %r9, %r5, %r6, %r7;
	setp.ge.u32 %p1, %r8, %r3;
	setp.ge.u32 %p2, %r9, %r4;
	or.pred %p1, %p1, %p2;
	@%p1 bra DONE;
	cvta.to.global.u64 %rd1, %rd1;
	cvta.to.global.u64 %rd2, %rd2;
	cvta.to.global.u64 %rd3, %rd3;

	// The block's two source rows, two pixels (8 bytes) each.
	shl.b32 %r10, %r9, 1;
	mul.wide.u32 %rd4, %r10, %r1;
	mul.wide.u32 %rd5, %r8, 8;
	add.s64 %rd6, %rd1, %rd4;
	add.s64 %rd6, %rd6, %rd5;
	cvt.u64.u32 %rd7, %r1;
	add.s64 %rd8, %rd6, %rd7;
	ld.global.v2.u32 {{%r11, %r12}}, [%rd6];
	ld.global.v2.u32 {{%r13, %r14}}, [%rd8];
	mov.u32 %r40, 0;
	mov.u32 %r41, 0;
	mov.u32 %r42, 0;
{p0}{p1}{p2}{p3}
	// Luma: two bytes on each of the block's rows.
	mul.wide.u32 %rd9, %r10, %r2;
	mul.wide.u32 %rd10, %r8, 2;
	add.s64 %rd11, %rd2, %rd9;
	add.s64 %rd11, %rd11, %rd10;
	shl.b32 %r25, %r22, 8;
	or.b32 %r25, %r25, %r21;
	st.global.u16 [%rd11], %r25;
	cvt.u64.u32 %rd12, %r2;
	add.s64 %rd11, %rd11, %rd12;
	shl.b32 %r26, %r24, 8;
	or.b32 %r26, %r26, %r23;
	st.global.u16 [%rd11], %r26;

	// Chroma: Cb then Cr, interleaved on the block's chroma row.
{cb}{cr}	shl.b32 %r31, %r31, 8;
	or.b32 %r30, %r30, %r31;
	mul.wide.u32 %rd13, %r9, %r2;
	add.s64 %rd14, %rd3, %rd13;
	add.s64 %rd14, %rd14, %rd10;
	st.global.u16 [%rd14], %r30;
DONE:
	ret;
}}
",
        p0 = pixel("%r11", "%r21"),
        p1 = pixel("%r12", "%r22"),
        p2 = pixel("%r13", "%r23"),
        p3 = pixel("%r14", "%r24"),
        cb = chroma("%r30", [-6596, -22188, 28784]),
        cr = chroma("%r31", [28784, -26145, -2639]),
    )
}

/// The GPU side of the conversion: frames are uploaded to `source`, the
/// kernel writes `nv12` (luma rows, then interleaved chroma rows, `pitch`
/// apart), which is registered with the NVENC session. Zero/null fields
/// haven't been created; `release` frees whatever has.
struct GpuInput {
    module: Handle,
    function: Handle,
    source: DevicePtr,
    nv12: DevicePtr,
    pitch: usize,
    registered: NV_ENC_REGISTERED_PTR,
    width: u32,
    height: u32,
}

impl GpuInput {
    /// Load the kernel and allocate both surfaces; the context must be current.
    fn new(cuda: &Cuda, width: u32, height: u32) -> anyhow::Result<Self> {
        let mut input = Self {
            module: ptr::null_mut(),
            function: ptr::null_mut(),
            source: 0,
            nv12: 0,
            pitch: 0,
            registered: ptr::null_mut(),
            width,
            height,
        };
        if let Err(error) = input.create(cuda) {
            input.release(cuda, None);
            return Err(error);
        }
        Ok(input)
    }

    fn create(&mut self, cuda: &Cuda) -> anyhow::Result<()> {
        let ptx = std::ffi::CString::new(kernel_ptx()).expect("PTX has no NUL");
        let mut log = vec![0u8; 4096];
        // CU_JIT_ERROR_LOG_BUFFER, CU_JIT_ERROR_LOG_BUFFER_SIZE_BYTES: the
        // size is passed as the pointer value itself.
        let mut options = [5, 6];
        let mut values = [log.as_mut_ptr().cast::<c_void>(), log.len() as *mut c_void];
        // SAFETY: NUL-terminated PTX; option arrays and the log outlive the call.
        let status = unsafe {
            (cuda.api.module_load)(
                &mut self.module,
                ptx.as_ptr().cast(),
                options.len() as u32,
                options.as_mut_ptr(),
                values.as_mut_ptr(),
            )
        };
        cuda.check(status, "compiling the colour conversion kernel")
            .with_context(|| {
                let end = log.iter().position(|&b| b == 0).unwrap_or(log.len());
                String::from_utf8_lossy(&log[..end]).into_owned()
            })?;
        // SAFETY: live module; the name is NUL-terminated.
        cuda.check(
            unsafe { (cuda.api.module_function)(&mut self.function, self.module, c"bgra_to_nv12".as_ptr()) },
            "cuModuleGetFunction",
        )?;
        let (width, height) = (self.width as usize, self.height as usize);
        // SAFETY: writes the new device address into our field.
        cuda.check(unsafe { (cuda.api.mem_alloc)(&mut self.source, width * height * 4) }, "cuMemAlloc")?;
        // Luma rows, then half as many chroma rows below them.
        // SAFETY: writes the new device address and its pitch into our fields.
        cuda.check(
            unsafe { (cuda.api.mem_alloc_pitch)(&mut self.nv12, &mut self.pitch, width, height * 3 / 2, 16) },
            "cuMemAllocPitch",
        )?;
        Ok(())
    }

    /// Upload one BGRA frame and convert it; returns once the NV12 surface
    /// is complete. The context must be current.
    fn convert(&self, cuda: &Cuda, bgra: &[u8]) -> anyhow::Result<()> {
        let size = self.width as usize * self.height as usize * 4;
        ensure!(bgra.len() >= size, "frame smaller than {}x{}", self.width, self.height);
        // SAFETY: `source` holds `size` bytes and `bgra` at least as many.
        cuda.check(
            unsafe { (cuda.api.copy_to_device)(self.source, bgra.as_ptr().cast(), size) },
            "uploading the frame",
        )?;
        let mut source = self.source;
        let mut source_pitch = self.width * 4;
        let mut luma = self.nv12;
        let mut chroma = self.nv12 + (self.pitch * self.height as usize) as u64;
        let mut pitch = self.pitch as u32;
        let (mut half_width, mut half_height) = (self.width / 2, self.height / 2);
        let mut args: [*mut c_void; 7] = [
            (&mut source as *mut DevicePtr).cast(),
            (&mut source_pitch as *mut u32).cast(),
            (&mut luma as *mut DevicePtr).cast(),
            (&mut chroma as *mut DevicePtr).cast(),
            (&mut pitch as *mut u32).cast(),
            (&mut half_width as *mut u32).cast(),
            (&mut half_height as *mut u32).cast(),
        ];
        const BLOCK: (u32, u32) = (32, 8);
        // SAFETY: the arguments match the kernel's parameter list and live
        // through the launch; every thread stays inside both surfaces.
        let status = unsafe {
            (cuda.api.launch)(
                self.function,
                half_width.div_ceil(BLOCK.0),
                half_height.div_ceil(BLOCK.1),
                1,
                BLOCK.0,
                BLOCK.1,
                1,
                0,
                ptr::null_mut(),
                args.as_mut_ptr(),
                ptr::null_mut(),
            )
        };
        cuda.check(status, "cuLaunchKernel")?;
        // NVENC reads the surface on its own stream: finish the kernel first.
        // SAFETY: no arguments; the context is current.
        cuda.check(unsafe { (cuda.api.ctx_synchronize)() }, "cuCtxSynchronize")
    }

    /// Free everything created so far. Unregistering needs the NVENC session
    /// it was registered with; the context must be current.
    fn release(&mut self, cuda: &Cuda, nvenc: Option<(&NV_ENCODE_API_FUNCTION_LIST, *mut c_void)>) {
        // SAFETY: each handle was created here and is freed once (then zeroed).
        unsafe {
            if let (false, Some((api, session))) = (self.registered.is_null(), nvenc) {
                api.nvEncUnregisterResource.unwrap()(session, self.registered);
            }
            if self.nv12 != 0 {
                (cuda.api.mem_free)(self.nv12);
            }
            if self.source != 0 {
                (cuda.api.mem_free)(self.source);
            }
            if !self.module.is_null() {
                (cuda.api.module_unload)(self.module);
            }
        }
        self.registered = ptr::null_mut();
        (self.nv12, self.source, self.module) = (0, 0, ptr::null_mut());
    }
}

pub struct NvencEncoder {
    api: NV_ENCODE_API_FUNCTION_LIST,
    session: *mut c_void,
    /// The GPU conversion, normally.
    gpu: Option<GpuInput>,
    /// Otherwise NVENC's own (CPU-writable) IYUV input buffer.
    cpu_input: NV_ENC_INPUT_PTR,
    output: NV_ENC_OUTPUT_PTR,
    config: Box<NV_ENC_CONFIG>,
    init: NV_ENC_INITIALIZE_PARAMS,
    codec: Codec,
    force_keyframe: bool,
    cuda: Cuda,
    _library: Library,
}

// SAFETY: exclusive &mut access serializes encoding; each operation binds our
// CUDA context to its calling thread. The boxed config pointer stays stable.
unsafe impl Send for NvencEncoder {}

impl NvencEncoder {
    pub fn new(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        bitrate: u32,
    ) -> anyhow::Result<Self> {
        Self::with_conversion(codec, width, height, fps, bitrate, true)
    }

    /// `gpu: false` converts on the CPU even where the kernel would work.
    fn with_conversion(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        bitrate: u32,
        gpu: bool,
    ) -> anyhow::Result<Self> {
        ensure!(
            matches!(codec, Codec::H264 | Codec::Hevc),
            "NVENC supports h264 and hevc"
        );
        ensure!(
            width > 0 && height > 0 && width.is_multiple_of(2) && height.is_multiple_of(2),
            "NVENC 4:2:0 requires nonzero even dimensions, got {width}x{height}"
        );
        ensure!(
            fps > 0 && bitrate >= fps,
            "NVENC requires positive fps and at least one bit per frame"
        );
        let (library, api) = load_api()?;
        let mut encoder = Self {
            api,
            session: ptr::null_mut(),
            gpu: None,
            cpu_input: ptr::null_mut(),
            output: ptr::null_mut(),
            config: Box::new(params!(NV_ENC_CONFIG, NV_ENC_CONFIG_VER)),
            init: params!(NV_ENC_INITIALIZE_PARAMS, NV_ENC_INITIALIZE_PARAMS_VER),
            codec,
            force_keyframe: true,
            cuda: Cuda::new()?,
            _library: library,
        };
        // Drop owns every handle as soon as it's created, including on partial failure.
        encoder.initialize(width, height, fps, bitrate, gpu)?;
        Ok(encoder)
    }

    fn check(&self, status: NVENCSTATUS, operation: &str) -> anyhow::Result<()> {
        check_status(status, operation, &self.api, self.session)
    }

    fn initialize(
        &mut self,
        width: u32,
        height: u32,
        fps: u32,
        bitrate: u32,
        gpu: bool,
    ) -> anyhow::Result<()> {
        let _current = self.cuda.enter()?;
        self.session = open_session(
            &self.api,
            self.cuda.context,
            _NV_ENC_DEVICE_TYPE_NV_ENC_DEVICE_TYPE_CUDA,
        )?;
        configure(
            &self.api,
            self.session,
            self.codec,
            width,
            height,
            fps,
            bitrate,
            &mut self.config,
            &mut self.init,
        )?;
        let mut output = params!(
            NV_ENC_CREATE_BITSTREAM_BUFFER,
            NV_ENC_CREATE_BITSTREAM_BUFFER_VER
        );
        // SAFETY: the initialized session allocates an owned bitstream buffer.
        let status =
            unsafe { self.api.nvEncCreateBitstreamBuffer.unwrap()(self.session, &mut output) };
        self.output = output.bitstreamBuffer;
        self.check(status, "nvEncCreateBitstreamBuffer")?;
        if gpu {
            match self.start_gpu(width, height) {
                Ok(input) => {
                    self.gpu = Some(input);
                    return Ok(());
                }
                Err(error) => tracing::warn!(
                    reason = %format!("{error:#}"),
                    "NVENC: converting colours on the CPU instead of the GPU"
                ),
            }
        }
        let mut input = params!(NV_ENC_CREATE_INPUT_BUFFER, NV_ENC_CREATE_INPUT_BUFFER_VER);
        input.width = width;
        input.height = height;
        input.bufferFmt = _NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_IYUV;
        // SAFETY: the initialized session allocates an owned, CPU-lockable input buffer.
        let status = unsafe { self.api.nvEncCreateInputBuffer.unwrap()(self.session, &mut input) };
        self.cpu_input = input.inputBuffer;
        self.check(status, "nvEncCreateInputBuffer")
    }

    /// The GPU conversion, its NV12 surface registered with the session.
    /// The context must be current.
    fn start_gpu(&self, width: u32, height: u32) -> anyhow::Result<GpuInput> {
        let mut input = GpuInput::new(&self.cuda, width, height)?;
        let mut register = params!(NV_ENC_REGISTER_RESOURCE, NV_ENC_REGISTER_RESOURCE_VER);
        register.resourceType = _NV_ENC_INPUT_RESOURCE_TYPE_NV_ENC_INPUT_RESOURCE_TYPE_CUDADEVICEPTR;
        register.width = width;
        register.height = height;
        register.pitch = input.pitch as u32;
        register.resourceToRegister = input.nv12 as *mut c_void;
        register.bufferFormat = _NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_NV12;
        register.bufferUsage = _NV_ENC_BUFFER_USAGE_NV_ENC_INPUT_IMAGE;
        // SAFETY: live session; the surface stays allocated until unregistered in release.
        let status = unsafe { self.api.nvEncRegisterResource.unwrap()(self.session, &mut register) };
        if let Err(error) = self.check(status, "nvEncRegisterResource") {
            input.release(&self.cuda, None);
            return Err(error);
        }
        input.registered = register.registeredResource;
        Ok(input)
    }

    fn reconfigure_bitrate(&mut self, bitrate: u32) -> anyhow::Result<()> {
        let _current = self.cuda.enter()?;
        reconfigure_bitrate(&self.api, self.session, &mut self.config, &self.init, bitrate)
    }

    /// Convert on the CPU into NVENC's IYUV buffer; returns its pitch.
    /// The context must be current.
    fn fill_cpu_input(&self, bgra: &[u8], width: usize, height: usize) -> anyhow::Result<u32> {
        let mut input = params!(NV_ENC_LOCK_INPUT_BUFFER, NV_ENC_LOCK_INPUT_BUFFER_VER);
        input.inputBuffer = self.cpu_input;
        // SAFETY: input belongs to this session and the previous encode completed synchronously.
        self.check(
            unsafe { self.api.nvEncLockInputBuffer.unwrap()(self.session, &mut input) },
            "nvEncLockInputBuffer",
        )?;
        let lock = InputLock(self);
        let pitch = input.pitch as usize;
        ensure!(
            pitch >= width && pitch.is_multiple_of(2) && !input.bufferDataPtr.is_null(),
            "NVENC returned invalid IYUV input pitch/pointer"
        );
        let luma = pitch
            .checked_mul(height)
            .context("NVENC input size overflow")?;
        let chroma = (pitch / 2) * (height / 2);
        let size = luma
            .checked_add(chroma * 2)
            .filter(|&n| n <= isize::MAX as usize)
            .context("NVENC input size overflow")?;
        // SAFETY: NVENC's locked IYUV allocation is Y, U, V with pitches p, p/2, p/2;
        // the lock gives exclusive access until unlock, including during scoped workers.
        let planes =
            unsafe { std::slice::from_raw_parts_mut(input.bufferDataPtr.cast::<u8>(), size) };
        let (y, uv) = planes.split_at_mut(luma);
        let (u, v) = uv.split_at_mut(chroma);
        bgra_to_i420(bgra, width, height, pitch, y, u, v);
        lock.unlock()?;
        Ok(input.pitch)
    }
}

impl Encoder for NvencEncoder {
    fn encode(&mut self, frame: &VideoFrame) -> anyhow::Result<Option<EncodedFrame>> {
        ensure!(
            frame.format == PixelFormat::Bgra8,
            "NVENC path expects BGRA"
        );
        ensure!(
            frame.width == self.init.encodeWidth && frame.height == self.init.encodeHeight,
            "NVENC expects {}x{}, got {}x{}",
            self.init.encodeWidth,
            self.init.encodeHeight,
            frame.width,
            frame.height
        );
        let started = std::time::Instant::now();
        let bytes = frame.data.to_cpu()?;
        let (width, height) = (frame.width as usize, frame.height as usize);
        let size = width
            .checked_mul(height)
            .and_then(|n| n.checked_mul(4))
            .context("NVENC frame size overflow")?;
        ensure!(bytes.len() >= size, "frame smaller than {width}x{height}");
        let _current = self.cuda.enter()?;
        let mut pic = params!(NV_ENC_PIC_PARAMS, NV_ENC_PIC_PARAMS_VER);
        let mapped = if let Some(gpu) = &self.gpu {
            gpu.convert(&self.cuda, &bytes)?;
            let mut map = params!(NV_ENC_MAP_INPUT_RESOURCE, NV_ENC_MAP_INPUT_RESOURCE_VER);
            map.registeredResource = gpu.registered;
            // SAFETY: registered resource of this session, not currently mapped.
            self.check(
                unsafe { self.api.nvEncMapInputResource.unwrap()(self.session, &mut map) },
                "nvEncMapInputResource",
            )?;
            pic.inputBuffer = map.mappedResource;
            pic.inputPitch = gpu.pitch as u32;
            pic.bufferFmt = map.mappedBufferFmt;
            Some(map.mappedResource)
        } else {
            pic.inputPitch = self.fill_cpu_input(&bytes, width, height)?;
            pic.inputBuffer = self.cpu_input;
            pic.bufferFmt = _NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_IYUV;
            None
        };
        pic.inputWidth = frame.width;
        pic.inputHeight = frame.height;
        pic.outputBitstream = self.output;
        pic.pictureStruct = _NV_ENC_PIC_STRUCT_NV_ENC_PIC_STRUCT_FRAME;
        pic.frameIdx = frame.frame_id as u32;
        pic.inputTimeStamp = frame.capture_ts_us;
        if self.force_keyframe {
            pic.encodePicFlags = _NV_ENC_PIC_FLAGS_NV_ENC_PIC_FLAG_FORCEIDR
                | _NV_ENC_PIC_FLAGS_NV_ENC_PIC_FLAG_OUTPUT_SPSPPS;
        }
        // SAFETY: input and output belong to this session; no encode is pending.
        let encoded = self
            .check(
                unsafe { self.api.nvEncEncodePicture.unwrap()(self.session, &mut pic) },
                "nvEncEncodePicture",
            )
            .and_then(|()| read_bitstream(&self.api, self.session, self.output));
        if let Some(mapped) = mapped {
            // SAFETY: matches the successful map above; the encode has finished.
            unsafe { self.api.nvEncUnmapInputResource.unwrap()(self.session, mapped) };
        }
        let (data, keyframe) = encoded?;
        self.force_keyframe = false;
        Ok(Some(EncodedFrame {
            frame_id: frame.frame_id,
            codec: self.codec,
            keyframe,
            data,
            capture_ts_us: frame.capture_ts_us,
            encode_done_ts_us: sunna_proto::now_us(),
            encode_us: started.elapsed().as_micros() as u64,
            width: frame.width,
            height: frame.height,
            format: frame.format,
        }))
    }

    fn set_target_bitrate(&mut self, bits_per_second: u32) {
        // The trait cannot return errors; leave the old rate intact if rejected.
        if let Err(error) = NvencEncoder::reconfigure_bitrate(self, bits_per_second) {
            tracing::warn!(%error, "couldn't apply a new NVENC bitrate");
        }
    }

    fn request_keyframe(&mut self) {
        self.force_keyframe = true;
    }
}

/// Holds NVENC's input buffer locked; unlocks it when dropped.
struct InputLock<'a>(&'a NvencEncoder);

impl InputLock<'_> {
    fn unlock(self) -> anyhow::Result<()> {
        let encoder = self.0;
        std::mem::forget(self);
        // SAFETY: this guard owned the one successful lock on the live encoder.
        encoder.check(
            unsafe { encoder.api.nvEncUnlockInputBuffer.unwrap()(encoder.session, encoder.cpu_input) },
            "nvEncUnlockInputBuffer",
        )
    }
}

impl Drop for InputLock<'_> {
    fn drop(&mut self) {
        // SAFETY: as in unlock, on an early return.
        unsafe { self.0.api.nvEncUnlockInputBuffer.unwrap()(self.0.session, self.0.cpu_input) };
    }
}

impl Drop for NvencEncoder {
    fn drop(&mut self) {
        let _current = self.cuda.enter().ok();
        if let Some(mut gpu) = self.gpu.take() {
            gpu.release(&self.cuda, Some((&self.api, self.session)));
        }
        // SAFETY: handles are owned, calls are serialized, and libraries/context are
        // still alive. Null handles mark resources not reached during initialization.
        unsafe {
            if !self.cpu_input.is_null() {
                (self.api.nvEncDestroyInputBuffer.unwrap())(self.session, self.cpu_input);
            }
            if !self.output.is_null() {
                (self.api.nvEncDestroyBitstreamBuffer.unwrap())(self.session, self.output);
            }
            if !self.session.is_null() {
                (self.api.nvEncDestroyEncoder.unwrap())(self.session);
            }
        }
        // Cuda::drop runs after this body, before either driver library is unloaded.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openh264_codec::OpenH264Decoder;
    use crate::Decoder;
    use bytes::Bytes;
    use sunna_capture::FrameData;

    fn frame(width: u32, height: u32, id: u64) -> VideoFrame {
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        for (n, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let (x, y) = (n % width as usize, n / width as usize);
            pixel.copy_from_slice(&[
                (x + id as usize * 8) as u8,
                (y + id as usize * 3) as u8,
                (x / 8 + y / 8 + id as usize) as u8,
                255,
            ]);
        }
        VideoFrame {
            frame_id: id,
            width,
            height,
            format: PixelFormat::Bgra8,
            data: FrameData::Cpu(Bytes::from(pixels)),
            capture_ts_us: id * 16_667,
        }
    }

    #[test]
    fn kernel_is_well_formed() {
        let ptx = kernel_ptx();
        assert_eq!(ptx.matches('{').count(), ptx.matches('}').count());
        assert!(!ptx.contains("{src}") && !ptx.contains("{luma}") && !ptx.contains("{out}"));
        // Every register used is declared: %r0..%r47, %rd0..%rd15, %p0..%p2.
        for (prefix, limit) in [("%rd", 16), ("%r", 48), ("%p", 3)] {
            for (at, _) in ptx.match_indices(prefix) {
                let rest = &ptx[at + prefix.len()..];
                let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
                if prefix == "%r" && rest.starts_with('d') || digits.is_empty() {
                    continue;
                }
                assert!(digits.parse::<u32>().unwrap() < limit, "{prefix}{digits} undeclared");
            }
        }
    }

    #[test]
    fn gpu_less_fallback() {
        // On a machine with a working NVENC the ignored test exercises the
        // hardware path. Without one, probing must not panic, and both codecs
        // must report unsupported.
        if supports(Codec::H264) || supports(Codec::Hevc) {
            return;
        }
        assert!(NvencEncoder::new(Codec::H264, 64, 64, 60, 1_000_000).is_err());
        assert!(crate::make_encoder("hevc", 64, 64, 60, 1_000_000).is_err());
        let mut encoder = crate::make_encoder("h264", 64, 64, 60, 1_000_000).unwrap();
        let encoded = encoder.encode(&frame(64, 64, 0)).unwrap().unwrap();
        assert!(encoded.keyframe);
        assert_eq!(encoded.codec, Codec::H264);
        let decoded = OpenH264Decoder::new()
            .unwrap()
            .decode(0, 0, true, &encoded.data)
            .unwrap();
        assert_eq!((decoded.width, decoded.height), (64, 64));
    }

    /// The GPU kernel must produce exactly the CPU converter's bytes.
    /// `cargo test --release -p sunna-codec nvenc -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn gpu_conversion_matches_cpu() {
        let cuda = Cuda::new().unwrap();
        let _current = cuda.enter().unwrap();
        // Odd multiples of the thread block, a pitch with padding, every byte value.
        for (width, height) in [(2, 2), (1918, 1080), (1710, 1112), (66, 18)] {
            let mut seed = 0x9e37_79b9_u32 ^ width;
            let bgra: Vec<u8> = (0..width * height * 4)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    seed as u8
                })
                .collect();
            let mut gpu = GpuInput::new(&cuda, width, height).unwrap();
            gpu.convert(&cuda, &bgra).unwrap();
            let mut nv12 = vec![0u8; gpu.pitch * height as usize * 3 / 2];
            // SAFETY: the surface is exactly this large.
            let status = unsafe { (cuda.api.copy_to_host)(nv12.as_mut_ptr().cast(), gpu.nv12, nv12.len()) };
            cuda.check(status, "download").unwrap();
            gpu.release(&cuda, None);
            let (w, h) = (width as usize, height as usize);
            let (mut y, mut u, mut v) = (vec![0; w * h], vec![0; w * h / 4], vec![0; w * h / 4]);
            bgra_to_i420(&bgra, w, h, w, &mut y, &mut u, &mut v);
            let pitch = nv12.len() / (h * 3 / 2);
            for row in 0..h {
                assert_eq!(&nv12[row * pitch..][..w], &y[row * w..][..w], "{width}x{height} luma row {row}");
            }
            for row in 0..h / 2 {
                let line = &nv12[(h + row) * pitch..][..w];
                for x in 0..w / 2 {
                    let at = row * w / 2 + x;
                    assert_eq!([line[x * 2], line[x * 2 + 1]], [u[at], v[at]], "{width}x{height} chroma {x},{row}");
                }
            }
            println!("{width}x{height}: GPU NV12 matches the CPU converter (pitch {pitch})");
        }
    }

    /// Encode known colours on the GPU path and decode them: BT.709 video
    /// range, as the stream's colour description says.
    #[test]
    #[ignore]
    fn gpu_colours_survive_encoding() {
        // White, black, red and a teal: BGRA.
        const PATCHES: [[u8; 4]; 4] = [[255, 255, 255, 255], [0, 0, 0, 255], [0, 0, 255, 255], [191, 212, 45, 255]];
        let (width, height) = (256u32, 64u32);
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for (n, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            pixel.copy_from_slice(&PATCHES[(n % width as usize) / 64]);
        }
        let input = VideoFrame {
            frame_id: 0,
            width,
            height,
            format: PixelFormat::Bgra8,
            data: FrameData::Cpu(Bytes::from(pixels)),
            capture_ts_us: 0,
        };
        let mut encoder = NvencEncoder::new(Codec::H264, width, height, 60, 20_000_000).unwrap();
        assert!(encoder.gpu.is_some(), "the GPU conversion didn't start");
        let encoded = encoder.encode(&input).unwrap().unwrap();
        let decoded = OpenH264Decoder::new().unwrap().decode(0, 0, true, &encoded.data).unwrap();
        let yuv = decoded.data.to_cpu().unwrap();
        let (w, h) = (width as usize, height as usize);
        for (i, patch) in PATCHES.iter().enumerate() {
            let (mut ey, mut eu, mut ev) = ([0u8; 4], [0u8; 1], [0u8; 1]);
            bgra_to_i420(&patch.repeat(4), 2, 2, 2, &mut ey, &mut eu, &mut ev);
            let expected = [ey[0], eu[0], ev[0]];
            let (x, y) = (i * 64 + 32, 32);
            let got = [
                yuv[y * w + x],
                yuv[w * h + (y / 2) * (w / 2) + x / 2],
                yuv[w * h * 5 / 4 + (y / 2) * (w / 2) + x / 2],
            ];
            for c in 0..3 {
                assert!(got[c].abs_diff(expected[c]) <= 2, "patch {i}: got {got:?}, expected {expected:?}");
            }
        }
    }

    /// CPU seconds this process has used so far (all threads).
    fn cpu_seconds() -> f64 {
        let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
        let fields: Vec<&str> = stat.rsplit_once(')').unwrap().1.split_whitespace().collect();
        // utime and stime, in clock ticks (100 per second on Linux).
        (fields[11].parse::<f64>().unwrap() + fields[12].parse::<f64>().unwrap()) / 100.0
    }

    /// `cargo test --release -p sunna-codec nvenc -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn nvenc_timing() {
        const FRAMES: u64 = 240;
        // Distinct frames made up front, so only encoding is measured.
        let inputs: Vec<VideoFrame> = (0..8).map(|i| frame(1920, 1080, i)).collect();
        for gpu in [true, false] {
            for codec in [Codec::H264, Codec::Hevc] {
                let encoder =
                    NvencEncoder::with_conversion(codec, 1920, 1080, 60, 25_000_000, gpu).unwrap();
                assert_eq!(encoder.gpu.is_some(), gpu);
                let inputs = &inputs;
                // Exercise Encoder: Send, including context binding and teardown.
                std::thread::scope(|scope| {
                    scope.spawn(move || {
                        let mut encoder = encoder;
                        let mut decoder = (codec == Codec::H264).then(|| OpenH264Decoder::new().unwrap());
                        let (mut times, mut total_bytes, mut cpu) = (Vec::new(), 0, 0.0);
                        for i in 0..FRAMES {
                            if i == 20 {
                                encoder.set_target_bitrate(15_000_000);
                                // The trait returns (), so verify that reconfiguration succeeded.
                                assert_eq!(encoder.config.rcParams.averageBitRate, 15_000_000);
                                assert_eq!(encoder.config.rcParams.vbvBufferSize, 250_000);
                            }
                            if i == 30 {
                                encoder.request_keyframe();
                            }
                            let mut input = inputs[i as usize % inputs.len()].clone();
                            input.frame_id = i;
                            input.capture_ts_us = i * 16_667;
                            let before = cpu_seconds();
                            let encoded = encoder.encode(&input).unwrap().unwrap();
                            cpu += cpu_seconds() - before;
                            assert_eq!(
                                encoded.keyframe,
                                i == 0 || i == 30,
                                "unexpected IDR at frame {i}"
                            );
                            assert_eq!(encoded.frame_id, i);
                            assert_eq!(encoded.capture_ts_us, input.capture_ts_us);
                            assert_eq!(encoded.codec, codec);
                            times.push(encoded.encode_us);
                            total_bytes += encoded.data.len();
                            if let Some(decoder) = decoder.as_mut().filter(|_| i < 60) {
                                let decoded = decoder
                                    .decode(i, encoded.capture_ts_us, encoded.keyframe, &encoded.data)
                                    .unwrap();
                                assert_eq!((decoded.width, decoded.height), (1920, 1080));
                            }
                        }
                        times.sort_unstable();
                        let at = |p: f64| times[((times.len() - 1) as f64 * p) as usize] as f64 / 1000.0;
                        println!(
                            "{} {} conversion: p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms, \
                             {:.1} ms CPU/frame, {:.0} bytes/frame",
                            codec.name(),
                            if gpu { "GPU" } else { "CPU" },
                            at(0.5),
                            at(0.99),
                            at(1.0),
                            cpu * 1000.0 / FRAMES as f64,
                            total_bytes as f64 / FRAMES as f64
                        );
                    });
                });
            }
        }
    }
}
