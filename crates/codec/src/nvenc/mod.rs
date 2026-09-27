//! Synchronous NVIDIA encoding on Linux, loaded at runtime so hosts without
//! the driver can still use OpenH264. P1 + one-frame CBR keeps latency down.

#[allow(unused_imports)]
mod sys;

use std::ffi::{c_void, CStr};
use std::ptr;
use std::sync::OnceLock;

use anyhow::{bail, ensure, Context};
use bytes::Bytes;
use libloading::Library;
use sunna_capture::{PixelFormat, VideoFrame};

use crate::yuv::bgra_to_i420;
use crate::{Codec, EncodedFrame, Encoder};
use sys::*;

// nvEncodeAPI.h 12.1: these macros and GUIDs aren't in the bindings.
const fn struct_version(version: u32) -> u32 {
    NVENCAPI_VERSION | (version << 16) | (0x7 << 28)
}
const NV_ENCODE_API_FUNCTION_LIST_VER: u32 = struct_version(2);
const NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER: u32 = struct_version(1);
const NV_ENC_CONFIG_VER: u32 = struct_version(8) | (1 << 31);
const NV_ENC_RC_PARAMS_VER: u32 = struct_version(1);
const NV_ENC_PRESET_CONFIG_VER: u32 = struct_version(4) | (1 << 31);
const NV_ENC_INITIALIZE_PARAMS_VER: u32 = struct_version(6) | (1 << 31);
const NV_ENC_RECONFIGURE_PARAMS_VER: u32 = struct_version(1) | (1 << 31);
const NV_ENC_CREATE_INPUT_BUFFER_VER: u32 = struct_version(1);
const NV_ENC_CREATE_BITSTREAM_BUFFER_VER: u32 = struct_version(1);
const NV_ENC_LOCK_INPUT_BUFFER_VER: u32 = struct_version(1);
const NV_ENC_PIC_PARAMS_VER: u32 = struct_version(6) | (1 << 31);
const NV_ENC_LOCK_BITSTREAM_VER: u32 = struct_version(1) | (1 << 31);
const NVENC_INFINITE_GOPLENGTH: u32 = 0xffff_ffff;
const NV_ENC_CODEC_H264_GUID: GUID = GUID {
    Data1: 0x6bc82762,
    Data2: 0x4e63,
    Data3: 0x4ca4,
    Data4: [0xaa, 0x85, 0x1e, 0x50, 0xf3, 0x21, 0xf6, 0xbf],
};
const NV_ENC_CODEC_HEVC_GUID: GUID = GUID {
    Data1: 0x790cdc88,
    Data2: 0x4522,
    Data3: 0x4d7b,
    Data4: [0x94, 0x25, 0xbd, 0xa9, 0x97, 0x5f, 0x76, 0x03],
};
const NV_ENC_H264_PROFILE_HIGH_GUID: GUID = GUID {
    Data1: 0xe7cbc309,
    Data2: 0x4f7a,
    Data3: 0x4b89,
    Data4: [0xaf, 0x2a, 0xd5, 0x37, 0xc9, 0x2b, 0xe3, 0x10],
};
const NV_ENC_HEVC_PROFILE_MAIN_GUID: GUID = GUID {
    Data1: 0xb514c39a,
    Data2: 0xb55b,
    Data3: 0x40fa,
    Data4: [0x87, 0x8f, 0xf1, 0x25, 0x3b, 0x4d, 0xfd, 0xec],
};
const NV_ENC_PRESET_P1_GUID: GUID = GUID {
    Data1: 0xfc0a8d3e,
    Data2: 0x45f8,
    Data3: 0x4cf8,
    Data4: [0x80, 0xc7, 0x29, 0x88, 0x71, 0x59, 0x0e, 0xbf],
};
const TUNING: NV_ENC_TUNING_INFO = NV_ENC_TUNING_INFO_NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY;
const INPUT_FORMAT: NV_ENC_BUFFER_FORMAT = _NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_IYUV;

macro_rules! params {
    ($ty:ty, $version:expr) => {{
        // SAFETY: used only with NVENC C structs; zero is valid for every field.
        let mut value: $ty = unsafe { std::mem::zeroed() };
        value.version = $version;
        value
    }};
}

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
            Library::new("libcuda.so.1").is_ok() && Library::new("libnvidia-encode.so.1").is_ok()
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

type CuInit = unsafe extern "C" fn(u32) -> i32;
type CuDeviceGet = unsafe extern "C" fn(*mut i32, i32) -> i32;
type CuCtxCreate = unsafe extern "C" fn(*mut *mut c_void, u32, i32) -> i32;
type CuCtxDestroy = unsafe extern "C" fn(*mut c_void) -> i32;
type CuCtxPush = unsafe extern "C" fn(*mut c_void) -> i32;
type CuCtxPop = unsafe extern "C" fn(*mut *mut c_void) -> i32;
type GetMaxVersion = unsafe extern "C" fn(*mut u32) -> NVENCSTATUS;
type CreateInstance = unsafe extern "C" fn(*mut NV_ENCODE_API_FUNCTION_LIST) -> NVENCSTATUS;

struct Cuda {
    context: *mut c_void,
    destroy: CuCtxDestroy,
    push: CuCtxPush,
    pop: CuCtxPop,
    _library: Library,
}

impl Cuda {
    fn new() -> anyhow::Result<Self> {
        // SAFETY: symbol signatures match the CUDA driver ABI; the library is retained.
        unsafe {
            let library = Library::new("libcuda.so.1").context("loading libcuda.so.1")?;
            let init = *library.get::<CuInit>(b"cuInit\0")?;
            let device_get = *library.get::<CuDeviceGet>(b"cuDeviceGet\0")?;
            let create = *library.get::<CuCtxCreate>(b"cuCtxCreate_v2\0")?;
            let mut cuda = Self {
                context: ptr::null_mut(),
                destroy: *library.get::<CuCtxDestroy>(b"cuCtxDestroy_v2\0")?,
                push: *library.get::<CuCtxPush>(b"cuCtxPushCurrent_v2\0")?,
                pop: *library.get::<CuCtxPop>(b"cuCtxPopCurrent_v2\0")?,
                _library: library,
            };
            cuda_check(init(0), "cuInit")?;
            let mut device = 0;
            cuda_check(device_get(&mut device, 0), "cuDeviceGet(0)")?;
            cuda_check(create(&mut cuda.context, 0, device), "cuCtxCreate_v2")?;
            // Detach from this thread; Encoder: Send permits moving to another one.
            let mut context = ptr::null_mut();
            cuda_check((cuda.pop)(&mut context), "cuCtxPopCurrent_v2")?;
            Ok(cuda)
        }
    }

    fn enter(&self) -> anyhow::Result<CurrentContext<'_>> {
        // SAFETY: this context is owned by self and operations are serialized.
        cuda_check(unsafe { (self.push)(self.context) }, "cuCtxPushCurrent_v2")?;
        Ok(CurrentContext(self))
    }
}

impl Drop for Cuda {
    fn drop(&mut self) {
        if !self.context.is_null() {
            // SAFETY: all NVENC resources are gone; the driver library is still loaded.
            unsafe { (self.destroy)(self.context) };
        }
    }
}

struct CurrentContext<'a>(&'a Cuda);
impl Drop for CurrentContext<'_> {
    fn drop(&mut self) {
        let mut context = ptr::null_mut();
        // SAFETY: matches enter's push and restores the calling thread's context.
        unsafe { (self.0.pop)(&mut context) };
    }
}

fn cuda_check(status: i32, operation: &str) -> anyhow::Result<()> {
    ensure!(status == 0, "{operation} failed: CUDA error {status}");
    Ok(())
}

pub struct NvencEncoder {
    api: NV_ENCODE_API_FUNCTION_LIST,
    session: *mut c_void,
    input: NV_ENC_INPUT_PTR,
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
        ensure!(
            matches!(codec, Codec::H264 | Codec::Hevc),
            "NVENC supports h264 and hevc"
        );
        ensure!(
            width > 0 && height > 0 && width.is_multiple_of(2) && height.is_multiple_of(2),
            "NVENC IYUV requires nonzero even dimensions, got {width}x{height}"
        );
        ensure!(
            fps > 0 && bitrate >= fps,
            "NVENC requires positive fps and at least one bit per frame"
        );
        let mut api = params!(NV_ENCODE_API_FUNCTION_LIST, NV_ENCODE_API_FUNCTION_LIST_VER);
        // SAFETY: symbols use the NVENC C ABI; library stays alive through encoder teardown.
        let library = unsafe {
            let library =
                Library::new("libnvidia-encode.so.1").context("loading libnvidia-encode.so.1")?;
            let max_version =
                library.get::<GetMaxVersion>(b"NvEncodeAPIGetMaxSupportedVersion\0")?;
            let mut version = 0;
            check_status(
                max_version(&mut version),
                "NvEncodeAPIGetMaxSupportedVersion",
                &api,
                ptr::null_mut(),
            )?;
            // This API uses major << 4 | minor, unlike NVENCAPI_VERSION.
            ensure!(
                version >= (12 << 4 | 1),
                "NVENC driver supports API {}.{}, but Sunna requires 12.1 or newer",
                version >> 4,
                version & 15
            );
            let create = library.get::<CreateInstance>(b"NvEncodeAPICreateInstance\0")?;
            check_status(
                create(&mut api),
                "NvEncodeAPICreateInstance",
                &api,
                ptr::null_mut(),
            )?;
            library
        };
        macro_rules! require {
            ($($name:ident),+ $(,)?) => {$(
                ensure!(api.$name.is_some(), "NVENC driver omitted {}", stringify!($name));
            )+};
        }
        require!(
            nvEncOpenEncodeSessionEx,
            nvEncGetEncodePresetConfigEx,
            nvEncInitializeEncoder,
            nvEncCreateInputBuffer,
            nvEncDestroyInputBuffer,
            nvEncCreateBitstreamBuffer,
            nvEncDestroyBitstreamBuffer,
            nvEncLockInputBuffer,
            nvEncUnlockInputBuffer,
            nvEncEncodePicture,
            nvEncLockBitstream,
            nvEncUnlockBitstream,
            nvEncReconfigureEncoder,
            nvEncDestroyEncoder
        );
        let mut encoder = Self {
            api,
            session: ptr::null_mut(),
            input: ptr::null_mut(),
            output: ptr::null_mut(),
            config: Box::new(params!(NV_ENC_CONFIG, NV_ENC_CONFIG_VER)),
            init: params!(NV_ENC_INITIALIZE_PARAMS, NV_ENC_INITIALIZE_PARAMS_VER),
            codec,
            force_keyframe: true,
            cuda: Cuda::new()?,
            _library: library,
        };
        // Drop owns every handle as soon as it's created, including on partial failure.
        encoder.initialize(width, height, fps, bitrate)?;
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
    ) -> anyhow::Result<()> {
        let _current = self.cuda.enter()?;
        let mut open = params!(
            NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS,
            NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER
        );
        open.device = self.cuda.context;
        open.deviceType = _NV_ENC_DEVICE_TYPE_NV_ENC_DEVICE_TYPE_CUDA;
        open.apiVersion = NVENCAPI_VERSION;
        // SAFETY: validated function pointer, live CUDA context, writable session handle.
        let status =
            unsafe { self.api.nvEncOpenEncodeSessionEx.unwrap()(&mut open, &mut self.session) };
        self.check(status, "nvEncOpenEncodeSessionEx")?;
        let guid = if self.codec == Codec::H264 {
            NV_ENC_CODEC_H264_GUID
        } else {
            NV_ENC_CODEC_HEVC_GUID
        };
        let mut preset = params!(NV_ENC_PRESET_CONFIG, NV_ENC_PRESET_CONFIG_VER);
        preset.presetCfg.version = NV_ENC_CONFIG_VER;
        // SAFETY: session is live and preset has both required version fields set.
        self.check(
            unsafe {
                self.api.nvEncGetEncodePresetConfigEx.unwrap()(
                    self.session,
                    guid,
                    NV_ENC_PRESET_P1_GUID,
                    TUNING,
                    &mut preset,
                )
            },
            "nvEncGetEncodePresetConfigEx",
        )?;
        *self.config = preset.presetCfg;
        self.config.version = NV_ENC_CONFIG_VER;
        self.config.gopLength = NVENC_INFINITE_GOPLENGTH;
        self.config.frameIntervalP = 1;
        self.config.frameFieldMode =
            _NV_ENC_PARAMS_FRAME_FIELD_MODE_NV_ENC_PARAMS_FRAME_FIELD_MODE_FRAME;
        let rc = &mut self.config.rcParams;
        rc.version = NV_ENC_RC_PARAMS_VER;
        rc.rateControlMode = _NV_ENC_PARAMS_RC_MODE_NV_ENC_PARAMS_RC_CBR;
        rc.set_enableLookahead(0);
        rc.lookaheadDepth = 0;
        rc.set_zeroReorderDelay(1);
        set_bitrate(rc, bitrate, fps);
        // SAFETY: access only the union member for the codec queried above.
        unsafe {
            if self.codec == Codec::H264 {
                self.config.profileGUID = NV_ENC_H264_PROFILE_HIGH_GUID;
                let h264 = &mut self.config.encodeCodecConfig.h264Config;
                h264.idrPeriod = NVENC_INFINITE_GOPLENGTH;
                h264.set_repeatSPSPPS(1);
                h264.set_disableSPSPPS(0);
                h264.chromaFormatIDC = 1;
                set_vui(&mut h264.h264VUIParameters);
            } else {
                self.config.profileGUID = NV_ENC_HEVC_PROFILE_MAIN_GUID;
                let hevc = &mut self.config.encodeCodecConfig.hevcConfig;
                hevc.idrPeriod = NVENC_INFINITE_GOPLENGTH;
                hevc.set_repeatSPSPPS(1);
                hevc.set_disableSPSPPS(0);
                hevc.set_chromaFormatIDC(1);
                hevc.set_pixelBitDepthMinus8(0);
                set_vui(&mut hevc.hevcVUIParameters);
            }
        }
        self.init.encodeGUID = guid;
        self.init.presetGUID = NV_ENC_PRESET_P1_GUID;
        self.init.tuningInfo = TUNING;
        self.init.encodeWidth = width;
        self.init.encodeHeight = height;
        self.init.darWidth = width;
        self.init.darHeight = height;
        self.init.frameRateNum = fps;
        self.init.frameRateDen = 1;
        self.init.enableEncodeAsync = 0;
        self.init.enablePTD = 1;
        self.init.encodeConfig = &mut *self.config;
        // SAFETY: init and its stable boxed config remain alive throughout the session.
        let status =
            unsafe { self.api.nvEncInitializeEncoder.unwrap()(self.session, &mut self.init) };
        self.check(status, "nvEncInitializeEncoder")?;
        let mut input = params!(NV_ENC_CREATE_INPUT_BUFFER, NV_ENC_CREATE_INPUT_BUFFER_VER);
        input.width = width;
        input.height = height;
        input.bufferFmt = INPUT_FORMAT;
        // SAFETY: the initialized session allocates an owned, CPU-lockable input buffer.
        let status = unsafe { self.api.nvEncCreateInputBuffer.unwrap()(self.session, &mut input) };
        self.input = input.inputBuffer;
        self.check(status, "nvEncCreateInputBuffer")?;
        let mut output = params!(
            NV_ENC_CREATE_BITSTREAM_BUFFER,
            NV_ENC_CREATE_BITSTREAM_BUFFER_VER
        );
        // SAFETY: the initialized session allocates an owned bitstream buffer.
        let status =
            unsafe { self.api.nvEncCreateBitstreamBuffer.unwrap()(self.session, &mut output) };
        self.output = output.bitstreamBuffer;
        self.check(status, "nvEncCreateBitstreamBuffer")
    }

    fn reconfigure_bitrate(&mut self, bitrate: u32) -> anyhow::Result<()> {
        let old = self.config.rcParams.averageBitRate;
        if (bitrate as f64 - old as f64).abs() / (old as f64).max(1.0) < 0.05 {
            return Ok(());
        }
        ensure!(
            bitrate >= self.init.frameRateNum,
            "NVENC bitrate must provide at least one bit per frame"
        );
        let _current = self.cuda.enter()?;
        let mut config = *self.config;
        set_bitrate(&mut config.rcParams, bitrate, self.init.frameRateNum);
        let mut params = params!(NV_ENC_RECONFIGURE_PARAMS, NV_ENC_RECONFIGURE_PARAMS_VER);
        params.reInitEncodeParams = self.init;
        params.reInitEncodeParams.encodeConfig = &mut config;
        params.set_resetEncoder(0);
        params.set_forceIDR(0);
        // SAFETY: reconfiguration is synchronous; the candidate config lives through the call.
        self.check(
            unsafe { self.api.nvEncReconfigureEncoder.unwrap()(self.session, &mut params) },
            "nvEncReconfigureEncoder",
        )?;
        *self.config = config;
        Ok(())
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
        let mut input = params!(NV_ENC_LOCK_INPUT_BUFFER, NV_ENC_LOCK_INPUT_BUFFER_VER);
        input.inputBuffer = self.input;
        // SAFETY: input belongs to this session and the previous encode completed synchronously.
        self.check(
            unsafe { self.api.nvEncLockInputBuffer.unwrap()(self.session, &mut input) },
            "nvEncLockInputBuffer",
        )?;
        let lock = BufferLock {
            encoder: self,
            buffer: self.input,
            input: true,
        };
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
        bgra_to_i420(&bytes, width, height, pitch, y, u, v);
        lock.unlock()?;
        let mut pic = params!(NV_ENC_PIC_PARAMS, NV_ENC_PIC_PARAMS_VER);
        pic.inputWidth = frame.width;
        pic.inputHeight = frame.height;
        pic.inputPitch = input.pitch;
        pic.inputBuffer = self.input;
        pic.outputBitstream = self.output;
        pic.bufferFmt = INPUT_FORMAT;
        pic.pictureStruct = _NV_ENC_PIC_STRUCT_NV_ENC_PIC_STRUCT_FRAME;
        pic.frameIdx = frame.frame_id as u32;
        pic.inputTimeStamp = frame.capture_ts_us;
        if self.force_keyframe {
            pic.encodePicFlags = _NV_ENC_PIC_FLAGS_NV_ENC_PIC_FLAG_FORCEIDR
                | _NV_ENC_PIC_FLAGS_NV_ENC_PIC_FLAG_OUTPUT_SPSPPS;
        }
        // SAFETY: buffers belong to this session; input is unlocked and no encode is pending.
        self.check(
            unsafe { self.api.nvEncEncodePicture.unwrap()(self.session, &mut pic) },
            "nvEncEncodePicture",
        )?;
        let mut output = params!(NV_ENC_LOCK_BITSTREAM, NV_ENC_LOCK_BITSTREAM_VER);
        output.outputBitstream = self.output;
        output.set_doNotWait(0);
        // SAFETY: blocking lock waits for the submitted frame to finish.
        self.check(
            unsafe { self.api.nvEncLockBitstream.unwrap()(self.session, &mut output) },
            "nvEncLockBitstream",
        )?;
        let lock = BufferLock {
            encoder: self,
            buffer: self.output,
            input: false,
        };
        ensure!(
            !output.bitstreamBufferPtr.is_null() && output.bitstreamSizeInBytes > 0,
            "NVENC returned an empty bitstream"
        );
        // SAFETY: NVENC provides this many readable bytes, valid until the lock is released.
        let data = unsafe {
            std::slice::from_raw_parts(
                output.bitstreamBufferPtr.cast::<u8>(),
                output.bitstreamSizeInBytes as usize,
            )
        };
        let data = Bytes::copy_from_slice(data);
        lock.unlock()?;
        self.force_keyframe = false;
        Ok(Some(EncodedFrame {
            frame_id: frame.frame_id,
            codec: self.codec,
            keyframe: output.pictureType == _NV_ENC_PIC_TYPE_NV_ENC_PIC_TYPE_IDR,
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
        if let Err(error) = self.reconfigure_bitrate(bits_per_second) {
            tracing::warn!(%error, "couldn't apply a new NVENC bitrate");
        }
    }

    fn request_keyframe(&mut self) {
        self.force_keyframe = true;
    }
}

struct BufferLock<'a> {
    encoder: &'a NvencEncoder,
    buffer: *mut c_void,
    input: bool,
}

impl BufferLock<'_> {
    fn unlock(mut self) -> anyhow::Result<()> {
        let buffer = std::mem::replace(&mut self.buffer, ptr::null_mut());
        self.release(buffer)
    }

    fn release(&self, buffer: *mut c_void) -> anyhow::Result<()> {
        let api = &self.encoder.api;
        let (unlock, name) = if self.input {
            (
                api.nvEncUnlockInputBuffer.unwrap(),
                "nvEncUnlockInputBuffer",
            )
        } else {
            (api.nvEncUnlockBitstream.unwrap(), "nvEncUnlockBitstream")
        };
        // SAFETY: this guard owns exactly one successful lock on the live encoder.
        self.encoder
            .check(unsafe { unlock(self.encoder.session, buffer) }, name)
    }
}

impl Drop for BufferLock<'_> {
    fn drop(&mut self) {
        if !self.buffer.is_null() {
            let _ = self.release(self.buffer);
        }
    }
}

impl Drop for NvencEncoder {
    fn drop(&mut self) {
        let _current = self.cuda.enter().ok();
        // SAFETY: handles are owned, calls are serialized, and libraries/context are
        // still alive. Null handles mark resources not reached during initialization.
        unsafe {
            if !self.input.is_null() {
                (self.api.nvEncDestroyInputBuffer.unwrap())(self.session, self.input);
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

fn set_bitrate(rc: &mut NV_ENC_RC_PARAMS, bitrate: u32, fps: u32) {
    rc.averageBitRate = bitrate;
    rc.maxBitRate = bitrate;
    rc.vbvBufferSize = bitrate / fps;
    rc.vbvInitialDelay = bitrate / fps;
}

fn set_vui(vui: &mut NV_ENC_CONFIG_H264_VUI_PARAMETERS) {
    vui.videoSignalTypePresentFlag = 1;
    vui.videoFormat = 5; // unspecified source format
    vui.videoFullRangeFlag = 0;
    vui.colourDescriptionPresentFlag = 1;
    vui.colourPrimaries = 1; // BT.709
    vui.transferCharacteristics = 13; // sRGB
    vui.colourMatrix = 1; // BT.709, matching bgra_to_i420
}

fn check_status(
    status: NVENCSTATUS,
    operation: &str,
    api: &NV_ENCODE_API_FUNCTION_LIST,
    session: *mut c_void,
) -> anyhow::Result<()> {
    if status == _NVENCSTATUS_NV_ENC_SUCCESS {
        return Ok(());
    }
    let mut detail = String::new();
    if let Some(last_error) = api.nvEncGetLastErrorString.filter(|_| !session.is_null()) {
        // SAFETY: live session; the driver owns a NUL-terminated error string (or NULL).
        unsafe {
            let message = last_error(session);
            if !message.is_null() {
                detail = CStr::from_ptr(message).to_string_lossy().into_owned();
            }
        }
    }
    bail!(
        "{operation} failed: {} ({status}): {detail}",
        status_name(status)
    )
}

fn status_name(status: NVENCSTATUS) -> &'static str {
    match status {
        _NVENCSTATUS_NV_ENC_SUCCESS => "NV_ENC_SUCCESS",
        _NVENCSTATUS_NV_ENC_ERR_NO_ENCODE_DEVICE => "NV_ENC_ERR_NO_ENCODE_DEVICE",
        _NVENCSTATUS_NV_ENC_ERR_UNSUPPORTED_DEVICE => "NV_ENC_ERR_UNSUPPORTED_DEVICE",
        _NVENCSTATUS_NV_ENC_ERR_INVALID_ENCODERDEVICE => "NV_ENC_ERR_INVALID_ENCODERDEVICE",
        _NVENCSTATUS_NV_ENC_ERR_INVALID_DEVICE => "NV_ENC_ERR_INVALID_DEVICE",
        _NVENCSTATUS_NV_ENC_ERR_DEVICE_NOT_EXIST => "NV_ENC_ERR_DEVICE_NOT_EXIST",
        _NVENCSTATUS_NV_ENC_ERR_INVALID_PTR => "NV_ENC_ERR_INVALID_PTR",
        _NVENCSTATUS_NV_ENC_ERR_INVALID_EVENT => "NV_ENC_ERR_INVALID_EVENT",
        _NVENCSTATUS_NV_ENC_ERR_INVALID_PARAM => "NV_ENC_ERR_INVALID_PARAM",
        _NVENCSTATUS_NV_ENC_ERR_INVALID_CALL => "NV_ENC_ERR_INVALID_CALL",
        _NVENCSTATUS_NV_ENC_ERR_OUT_OF_MEMORY => "NV_ENC_ERR_OUT_OF_MEMORY",
        _NVENCSTATUS_NV_ENC_ERR_ENCODER_NOT_INITIALIZED => "NV_ENC_ERR_ENCODER_NOT_INITIALIZED",
        _NVENCSTATUS_NV_ENC_ERR_UNSUPPORTED_PARAM => "NV_ENC_ERR_UNSUPPORTED_PARAM",
        _NVENCSTATUS_NV_ENC_ERR_LOCK_BUSY => "NV_ENC_ERR_LOCK_BUSY",
        _NVENCSTATUS_NV_ENC_ERR_NOT_ENOUGH_BUFFER => "NV_ENC_ERR_NOT_ENOUGH_BUFFER",
        _NVENCSTATUS_NV_ENC_ERR_INVALID_VERSION => "NV_ENC_ERR_INVALID_VERSION",
        _NVENCSTATUS_NV_ENC_ERR_MAP_FAILED => "NV_ENC_ERR_MAP_FAILED",
        _NVENCSTATUS_NV_ENC_ERR_NEED_MORE_INPUT => "NV_ENC_ERR_NEED_MORE_INPUT",
        _NVENCSTATUS_NV_ENC_ERR_ENCODER_BUSY => "NV_ENC_ERR_ENCODER_BUSY",
        _NVENCSTATUS_NV_ENC_ERR_EVENT_NOT_REGISTERD => "NV_ENC_ERR_EVENT_NOT_REGISTERD",
        _NVENCSTATUS_NV_ENC_ERR_GENERIC => "NV_ENC_ERR_GENERIC",
        _NVENCSTATUS_NV_ENC_ERR_INCOMPATIBLE_CLIENT_KEY => "NV_ENC_ERR_INCOMPATIBLE_CLIENT_KEY",
        _NVENCSTATUS_NV_ENC_ERR_UNIMPLEMENTED => "NV_ENC_ERR_UNIMPLEMENTED",
        _NVENCSTATUS_NV_ENC_ERR_RESOURCE_REGISTER_FAILED => "NV_ENC_ERR_RESOURCE_REGISTER_FAILED",
        _NVENCSTATUS_NV_ENC_ERR_RESOURCE_NOT_REGISTERED => "NV_ENC_ERR_RESOURCE_NOT_REGISTERED",
        _NVENCSTATUS_NV_ENC_ERR_RESOURCE_NOT_MAPPED => "NV_ENC_ERR_RESOURCE_NOT_MAPPED",
        _NVENCSTATUS_NV_ENC_ERR_NEED_MORE_OUTPUT => "NV_ENC_ERR_NEED_MORE_OUTPUT",
        _ => "NV_ENC_ERR_UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openh264_codec::OpenH264Decoder;
    use crate::Decoder;
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
    fn gpu_less_fallback() {
        // On a driver-equipped host the ignored test exercises the hardware path.
        // This VM has neither library: probing must be cheap and must not panic.
        if available() {
            return;
        }
        assert!(!available());
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

    /// `cargo test --release -p sunna-codec nvenc -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn nvenc_timing() {
        for codec in [Codec::H264, Codec::Hevc] {
            let encoder = NvencEncoder::new(codec, 1920, 1080, 60, 25_000_000).unwrap();
            // Exercise Encoder: Send, including context binding and teardown.
            std::thread::spawn(move || {
                let mut encoder = encoder;
                let mut decoder = (codec == Codec::H264).then(|| OpenH264Decoder::new().unwrap());
                let (mut total_us, mut max_us, mut total_bytes) = (0, 0, 0);
                for i in 0..60 {
                    if i == 20 {
                        encoder.set_target_bitrate(15_000_000);
                        // The trait returns (), so verify that reconfiguration succeeded.
                        assert_eq!(encoder.config.rcParams.averageBitRate, 15_000_000);
                        assert_eq!(encoder.config.rcParams.vbvBufferSize, 250_000);
                    }
                    if i == 30 {
                        encoder.request_keyframe();
                    }
                    let input = frame(1920, 1080, i);
                    let encoded = encoder.encode(&input).unwrap().unwrap();
                    assert_eq!(
                        encoded.keyframe,
                        i == 0 || i == 30,
                        "unexpected IDR at frame {i}"
                    );
                    assert_eq!(encoded.frame_id, i);
                    assert_eq!(encoded.capture_ts_us, input.capture_ts_us);
                    assert_eq!(encoded.codec, codec);
                    total_us += encoded.encode_us;
                    max_us = max_us.max(encoded.encode_us);
                    total_bytes += encoded.data.len();
                    if let Some(decoder) = &mut decoder {
                        let decoded = decoder
                            .decode(i, encoded.capture_ts_us, encoded.keyframe, &encoded.data)
                            .unwrap();
                        assert_eq!((decoded.width, decoded.height), (1920, 1080));
                    }
                }
                println!(
                    "{}: mean {:.2} ms, max {:.2} ms, {:.0} bytes/frame",
                    codec.name(),
                    total_us as f64 / 60_000.0,
                    max_us as f64 / 1000.0,
                    total_bytes as f64 / 60.0
                );
            })
            .join()
            .unwrap();
        }
    }
}
