//! NVENC on Windows: the session lives on the capture's D3D11 device and
//! encodes its NV12 textures in place (registered once, mapped per frame),
//! so a frame goes from the desktop to the bitstream without leaving the GPU.

use std::ffi::c_void;
use std::ptr;
use std::sync::OnceLock;

use anyhow::{bail, ensure, Context};
use libloading::Library;
use sunna_capture::{FrameData, PixelFormat, VideoFrame};
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};

use super::*;
use crate::{Codec, EncodedFrame, Encoder};

/// NVIDIA's PCI vendor id.
const NVIDIA: u32 = 0x10de;
/// Registrations kept before starting over: a capture pool is a handful of
/// textures, so more means the pool was replaced.
const MAX_REGISTERED: usize = 16;

/// Whether this PC's NVIDIA GPU and driver can encode `codec`: opens a small
/// session once per codec on a device of its own and caches the answer.
pub fn supports(codec: Codec) -> bool {
    static H264: OnceLock<bool> = OnceLock::new();
    static HEVC: OnceLock<bool> = OnceLock::new();
    let cell = match codec {
        Codec::H264 => &H264,
        Codec::Hevc => &HEVC,
        _ => return false,
    };
    *cell.get_or_init(|| {
        let probe = || -> anyhow::Result<()> {
            let device = sunna_capture::windows::device_for_vendor(NVIDIA)?;
            let mut encoder = NvencEncoder::new(codec, 640, 360, 30, 2_000_000)?;
            encoder.open(&device)
        };
        match probe() {
            Ok(()) => true,
            Err(error) => {
                tracing::info!(?codec, reason = %format!("{error:#}"), "NVENC can't encode this here");
                false
            }
        }
    })
}

pub struct NvencEncoder {
    api: NV_ENCODE_API_FUNCTION_LIST,
    /// Null until the first frame brings the device to open it on.
    session: *mut c_void,
    device: Option<ID3D11Device>,
    output: NV_ENC_OUTPUT_PTR,
    /// Textures registered with the session, kept alive so their addresses
    /// can't be reused by other textures while registered.
    registered: Vec<(ID3D11Texture2D, NV_ENC_REGISTERED_PTR)>,
    config: Box<NV_ENC_CONFIG>,
    init: NV_ENC_INITIALIZE_PARAMS,
    codec: Codec,
    width: u32,
    height: u32,
    fps: u32,
    bitrate: u32,
    force_keyframe: bool,
    _library: Library,
}

// SAFETY: exclusive &mut access serializes encoding; the D3D11 device is
// multithread-protected. The boxed config pointer stays stable.
unsafe impl Send for NvencEncoder {}

impl NvencEncoder {
    pub fn new(codec: Codec, width: u32, height: u32, fps: u32, bitrate: u32) -> anyhow::Result<Self> {
        ensure!(matches!(codec, Codec::H264 | Codec::Hevc), "NVENC supports h264 and hevc");
        ensure!(
            width > 0 && height > 0 && width.is_multiple_of(2) && height.is_multiple_of(2),
            "NVENC NV12 requires nonzero even dimensions, got {width}x{height}"
        );
        ensure!(fps > 0 && bitrate >= fps, "NVENC requires positive fps and at least one bit per frame");
        let (library, api) = load_api()?;
        Ok(Self {
            api,
            session: ptr::null_mut(),
            device: None,
            output: ptr::null_mut(),
            registered: Vec::new(),
            config: Box::new(params!(NV_ENC_CONFIG, NV_ENC_CONFIG_VER)),
            init: params!(NV_ENC_INITIALIZE_PARAMS, NV_ENC_INITIALIZE_PARAMS_VER),
            codec,
            width,
            height,
            fps,
            bitrate,
            force_keyframe: true,
            _library: library,
        })
    }

    fn check(&self, status: NVENCSTATUS, operation: &str) -> anyhow::Result<()> {
        check_status(status, operation, &self.api, self.session)
    }

    /// Open and initialize the session on `device`, closing any old one.
    fn open(&mut self, device: &ID3D11Device) -> anyhow::Result<()> {
        self.close();
        self.session = open_session(
            &self.api,
            device.as_raw(),
            _NV_ENC_DEVICE_TYPE_NV_ENC_DEVICE_TYPE_DIRECTX,
        )?;
        self.device = Some(device.clone());
        configure(
            &self.api,
            self.session,
            self.codec,
            self.width,
            self.height,
            self.fps,
            self.bitrate,
            &mut self.config,
            &mut self.init,
        )?;
        let mut output = params!(NV_ENC_CREATE_BITSTREAM_BUFFER, NV_ENC_CREATE_BITSTREAM_BUFFER_VER);
        // SAFETY: the initialized session allocates an owned bitstream buffer.
        let status = unsafe { self.api.nvEncCreateBitstreamBuffer.unwrap()(self.session, &mut output) };
        self.output = output.bitstreamBuffer;
        self.check(status, "nvEncCreateBitstreamBuffer")?;
        self.force_keyframe = true;
        Ok(())
    }

    fn unregister_all(&mut self) {
        for (_, resource) in self.registered.drain(..) {
            // SAFETY: registered with this live session and not mapped.
            unsafe { self.api.nvEncUnregisterResource.unwrap()(self.session, resource) };
        }
    }

    fn close(&mut self) {
        if self.session.is_null() {
            return;
        }
        self.unregister_all();
        // SAFETY: owned handles of this session; no encode is in progress.
        unsafe {
            if !self.output.is_null() {
                self.api.nvEncDestroyBitstreamBuffer.unwrap()(self.session, self.output);
            }
            self.api.nvEncDestroyEncoder.unwrap()(self.session);
        }
        self.output = ptr::null_mut();
        self.session = ptr::null_mut();
        self.device = None;
    }

    fn registration(&mut self, texture: &ID3D11Texture2D) -> anyhow::Result<NV_ENC_REGISTERED_PTR> {
        if let Some((_, resource)) = self.registered.iter().find(|(t, _)| t == texture) {
            return Ok(*resource);
        }
        if self.registered.len() >= MAX_REGISTERED {
            self.unregister_all();
        }
        let mut register = params!(NV_ENC_REGISTER_RESOURCE, NV_ENC_REGISTER_RESOURCE_VER);
        register.resourceType = _NV_ENC_INPUT_RESOURCE_TYPE_NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX;
        register.width = self.width;
        register.height = self.height;
        register.pitch = 0;
        register.subResourceIndex = 0;
        register.resourceToRegister = texture.as_raw();
        register.bufferFormat = _NV_ENC_BUFFER_FORMAT_NV_ENC_BUFFER_FORMAT_NV12;
        register.bufferUsage = _NV_ENC_BUFFER_USAGE_NV_ENC_INPUT_IMAGE;
        // SAFETY: live session; the texture is on the session's device and
        // stays alive (held in `registered`) until unregistered.
        let status = unsafe { self.api.nvEncRegisterResource.unwrap()(self.session, &mut register) };
        self.check(status, "nvEncRegisterResource")?;
        self.registered.push((texture.clone(), register.registeredResource));
        Ok(register.registeredResource)
    }
}

impl Encoder for NvencEncoder {
    fn encode(&mut self, frame: &VideoFrame) -> anyhow::Result<Option<EncodedFrame>> {
        let FrameData::D3d11(input) = &frame.data else {
            bail!("NVENC on Windows encodes GPU frames; this one is in system memory");
        };
        ensure!(frame.format == PixelFormat::Nv12, "NVENC path expects NV12 textures");
        ensure!(
            frame.width == self.width && frame.height == self.height,
            "NVENC expects {}x{}, got {}x{}",
            self.width,
            self.height,
            frame.width,
            frame.height
        );
        let started = std::time::Instant::now();
        if self.device.as_ref() != Some(&input.device) {
            // First frame, or capture made a new device (GPU reset, display
            // moved to another adapter).
            self.open(&input.device).context("opening NVENC on the capture's device")?;
        }
        let resource = self.registration(&input.texture)?;
        let mut map = params!(NV_ENC_MAP_INPUT_RESOURCE, NV_ENC_MAP_INPUT_RESOURCE_VER);
        map.registeredResource = resource;
        // SAFETY: registered resource of this session, not currently mapped.
        self.check(
            unsafe { self.api.nvEncMapInputResource.unwrap()(self.session, &mut map) },
            "nvEncMapInputResource",
        )?;
        let mut pic = params!(NV_ENC_PIC_PARAMS, NV_ENC_PIC_PARAMS_VER);
        pic.inputWidth = self.width;
        pic.inputHeight = self.height;
        pic.inputPitch = self.width;
        pic.inputBuffer = map.mappedResource;
        pic.outputBitstream = self.output;
        pic.bufferFmt = map.mappedBufferFmt;
        pic.pictureStruct = _NV_ENC_PIC_STRUCT_NV_ENC_PIC_STRUCT_FRAME;
        pic.frameIdx = frame.frame_id as u32;
        pic.inputTimeStamp = frame.capture_ts_us;
        if self.force_keyframe {
            pic.encodePicFlags = _NV_ENC_PIC_FLAGS_NV_ENC_PIC_FLAG_FORCEIDR
                | _NV_ENC_PIC_FLAGS_NV_ENC_PIC_FLAG_OUTPUT_SPSPPS;
        }
        // SAFETY: mapped input and owned output of this session.
        let encoded = self
            .check(
                unsafe { self.api.nvEncEncodePicture.unwrap()(self.session, &mut pic) },
                "nvEncEncodePicture",
            )
            .and_then(|()| read_bitstream(&self.api, self.session, self.output));
        // SAFETY: matches the successful map above; the encode has finished.
        unsafe { self.api.nvEncUnmapInputResource.unwrap()(self.session, map.mappedResource) };
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
        self.bitrate = bits_per_second.max(self.fps);
        if self.session.is_null() {
            return; // applied when the session opens
        }
        // The trait cannot return errors; leave the old rate intact if rejected.
        if let Err(error) = reconfigure_bitrate(&self.api, self.session, &mut self.config, &self.init, bits_per_second) {
            tracing::warn!(%error, "couldn't apply a new NVENC bitrate");
        }
    }

    fn request_keyframe(&mut self) {
        self.force_keyframe = true;
    }
}

impl Drop for NvencEncoder {
    fn drop(&mut self) {
        self.close();
    }
}

const NV_ENC_CODEC_AV1_GUID: GUID = GUID {
    Data1: 0x0a352289,
    Data2: 0x0aa7,
    Data3: 0x4759,
    Data4: [0x86, 0x2d, 0x5d, 0x15, 0xcd, 0x16, 0xd2, 0x54],
};

/// (capability, value) pairs for one codec.
pub type Capabilities = Vec<(&'static str, i32)>;

/// What this PC's NVIDIA encoder can do, per codec, as (capability, value)
/// pairs: what Sunna can offer beyond the basics (4:4:4 for sharp text,
/// 10-bit for HDR, reference invalidation to repair loss without a keyframe).
pub fn capabilities() -> anyhow::Result<Vec<(&'static str, Capabilities)>> {
    let device = sunna_capture::windows::device_for_vendor(NVIDIA)?;
    let (_library, api) = load_api()?;
    let session = open_session(&api, device.as_raw(), _NV_ENC_DEVICE_TYPE_NV_ENC_DEVICE_TYPE_DIRECTX)?;
    let caps = [
        ("yuv444", _NV_ENC_CAPS_NV_ENC_CAPS_SUPPORT_YUV444_ENCODE),
        ("10bit", _NV_ENC_CAPS_NV_ENC_CAPS_SUPPORT_10BIT_ENCODE),
        ("lossless", _NV_ENC_CAPS_NV_ENC_CAPS_SUPPORT_LOSSLESS_ENCODE),
        ("ref_pic_invalidation", _NV_ENC_CAPS_NV_ENC_CAPS_SUPPORT_REF_PIC_INVALIDATION),
        ("max_ltr_frames", _NV_ENC_CAPS_NV_ENC_CAPS_NUM_MAX_LTR_FRAMES),
        ("intra_refresh", _NV_ENC_CAPS_NV_ENC_CAPS_SUPPORT_INTRA_REFRESH),
        ("async_encode", _NV_ENC_CAPS_NV_ENC_CAPS_ASYNC_ENCODE_SUPPORT),
        ("subframe_readback", _NV_ENC_CAPS_NV_ENC_CAPS_SUPPORT_SUBFRAME_READBACK),
        ("dynamic_slice_mode", _NV_ENC_CAPS_NV_ENC_CAPS_SUPPORT_DYNAMIC_SLICE_MODE),
        ("dyn_res_change", _NV_ENC_CAPS_NV_ENC_CAPS_SUPPORT_DYN_RES_CHANGE),
        ("encoder_engines", _NV_ENC_CAPS_NV_ENC_CAPS_NUM_ENCODER_ENGINES),
        ("width_max", _NV_ENC_CAPS_NV_ENC_CAPS_WIDTH_MAX),
        ("height_max", _NV_ENC_CAPS_NV_ENC_CAPS_HEIGHT_MAX),
        ("mb_per_sec_max", _NV_ENC_CAPS_NV_ENC_CAPS_MB_PER_SEC_MAX),
    ];
    let mut result = Vec::new();
    for (name, guid) in [
        ("h264", NV_ENC_CODEC_H264_GUID),
        ("hevc", NV_ENC_CODEC_HEVC_GUID),
        ("av1", NV_ENC_CODEC_AV1_GUID),
    ] {
        let mut values = Vec::new();
        for (cap, id) in caps {
            let mut param = params!(NV_ENC_CAPS_PARAM, struct_version(1));
            param.capsToQuery = id;
            let mut value = 0;
            // SAFETY: live session; out-value is a local.
            let status = unsafe { api.nvEncGetEncodeCaps.unwrap()(session, guid, &mut param, &mut value) };
            if status != _NVENCSTATUS_NV_ENC_SUCCESS {
                values.push(("unsupported", status as i32));
                break;
            }
            values.push((cap, value));
        }
        result.push((name, values));
    }
    // SAFETY: the session is ours and unused from here.
    unsafe { api.nvEncDestroyEncoder.unwrap()(session) };
    Ok(result)
}
