//! Synchronous NVIDIA encoding, loaded at runtime so hosts without the
//! driver can still use a software encoder. P1 + one-frame CBR keeps latency
//! down. Linux uploads the captured pixels and converts them on the GPU with
//! CUDA (`cuda`); Windows hands it the capture's D3D11 textures directly
//! (`d3d11`). The session settings are shared (`configure`).

#[allow(unused_imports)]
mod sys;

use std::ffi::{c_void, CStr};
use std::ptr;

use anyhow::{bail, ensure, Context};
use libloading::Library;

use crate::Codec;
use sys::*;

#[cfg(target_os = "linux")]
const LIBRARY: &str = "libnvidia-encode.so.1";
#[cfg(target_os = "windows")]
const LIBRARY: &str = "nvEncodeAPI64.dll";

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
#[cfg(target_os = "linux")]
const NV_ENC_CREATE_INPUT_BUFFER_VER: u32 = struct_version(1);
const NV_ENC_CREATE_BITSTREAM_BUFFER_VER: u32 = struct_version(1);
#[cfg(target_os = "linux")]
const NV_ENC_LOCK_INPUT_BUFFER_VER: u32 = struct_version(1);
const NV_ENC_PIC_PARAMS_VER: u32 = struct_version(6) | (1 << 31);
const NV_ENC_LOCK_BITSTREAM_VER: u32 = struct_version(1) | (1 << 31);
const NV_ENC_REGISTER_RESOURCE_VER: u32 = struct_version(4);
const NV_ENC_MAP_INPUT_RESOURCE_VER: u32 = struct_version(4);
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

macro_rules! params {
    ($ty:ty, $version:expr) => {{
        // SAFETY: used only with NVENC C structs; zero is valid for every field.
        let mut value: $ty = unsafe { std::mem::zeroed() };
        value.version = $version;
        value
    }};
}

// After `params!`, which they use.
#[cfg(target_os = "linux")]
mod cuda;
#[cfg(target_os = "windows")]
mod d3d11;

#[cfg(target_os = "linux")]
pub use cuda::{supports, NvencEncoder};
#[cfg(target_os = "windows")]
pub use d3d11::{capabilities, supports, NvencEncoder};

type GetMaxVersion = unsafe extern "C" fn(*mut u32) -> NVENCSTATUS;
type CreateInstance = unsafe extern "C" fn(*mut NV_ENCODE_API_FUNCTION_LIST) -> NVENCSTATUS;

/// Load the driver's NVENC library and its function table (API 12.1+).
fn load_api() -> anyhow::Result<(Library, NV_ENCODE_API_FUNCTION_LIST)> {
    let mut api = params!(NV_ENCODE_API_FUNCTION_LIST, NV_ENCODE_API_FUNCTION_LIST_VER);
    // SAFETY: symbols use the NVENC C ABI; the caller keeps the library alive
    // for as long as it uses the function table.
    let library = unsafe {
        let library = Library::new(LIBRARY).with_context(|| format!("loading {LIBRARY}"))?;
        let max_version = library.get::<GetMaxVersion>(b"NvEncodeAPIGetMaxSupportedVersion\0")?;
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
        check_status(create(&mut api), "NvEncodeAPICreateInstance", &api, ptr::null_mut())?;
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
        nvEncRegisterResource,
        nvEncUnregisterResource,
        nvEncMapInputResource,
        nvEncUnmapInputResource,
        nvEncEncodePicture,
        nvEncLockBitstream,
        nvEncUnlockBitstream,
        nvEncReconfigureEncoder,
        nvEncDestroyEncoder
    );
    Ok((library, api))
}

/// Open a session on `device` (a CUDA context or a D3D11 device).
fn open_session(
    api: &NV_ENCODE_API_FUNCTION_LIST,
    device: *mut c_void,
    device_type: NV_ENC_DEVICE_TYPE,
) -> anyhow::Result<*mut c_void> {
    let mut open = params!(
        NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS,
        NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER
    );
    open.device = device;
    open.deviceType = device_type;
    open.apiVersion = NVENCAPI_VERSION;
    let mut session = ptr::null_mut();
    // SAFETY: validated function pointer, live device, writable session handle.
    let status = unsafe { api.nvEncOpenEncodeSessionEx.unwrap()(&mut open, &mut session) };
    if let Err(error) = check_status(status, "nvEncOpenEncodeSessionEx", api, session) {
        if !session.is_null() {
            // SAFETY: the driver may hand back a session even on failure.
            unsafe { api.nvEncDestroyEncoder.unwrap()(session) };
        }
        return Err(error);
    }
    Ok(session)
}

/// Sunna's low-latency settings on an open session, then initialize it:
/// P1, ultra-low-latency tuning, CBR with a one-frame VBV, no lookahead, no
/// B-frames, an endless GOP (keyframes only on request), BT.709 video range.
#[allow(clippy::too_many_arguments)]
fn configure(
    api: &NV_ENCODE_API_FUNCTION_LIST,
    session: *mut c_void,
    codec: Codec,
    width: u32,
    height: u32,
    fps: u32,
    bitrate: u32,
    config: &mut NV_ENC_CONFIG,
    init: &mut NV_ENC_INITIALIZE_PARAMS,
) -> anyhow::Result<()> {
    let guid = if codec == Codec::H264 {
        NV_ENC_CODEC_H264_GUID
    } else {
        NV_ENC_CODEC_HEVC_GUID
    };
    let mut preset = params!(NV_ENC_PRESET_CONFIG, NV_ENC_PRESET_CONFIG_VER);
    preset.presetCfg.version = NV_ENC_CONFIG_VER;
    // SAFETY: session is live and preset has both required version fields set.
    check_status(
        unsafe {
            api.nvEncGetEncodePresetConfigEx.unwrap()(
                session,
                guid,
                NV_ENC_PRESET_P1_GUID,
                TUNING,
                &mut preset,
            )
        },
        "nvEncGetEncodePresetConfigEx",
        api,
        session,
    )?;
    *config = preset.presetCfg;
    config.version = NV_ENC_CONFIG_VER;
    config.gopLength = NVENC_INFINITE_GOPLENGTH;
    config.frameIntervalP = 1;
    config.frameFieldMode = _NV_ENC_PARAMS_FRAME_FIELD_MODE_NV_ENC_PARAMS_FRAME_FIELD_MODE_FRAME;
    let rc = &mut config.rcParams;
    rc.version = NV_ENC_RC_PARAMS_VER;
    rc.rateControlMode = _NV_ENC_PARAMS_RC_MODE_NV_ENC_PARAMS_RC_CBR;
    rc.set_enableLookahead(0);
    rc.lookaheadDepth = 0;
    rc.set_zeroReorderDelay(1);
    set_bitrate(rc, bitrate, fps);
    // SAFETY: access only the union member for the codec queried above.
    unsafe {
        if codec == Codec::H264 {
            config.profileGUID = NV_ENC_H264_PROFILE_HIGH_GUID;
            let h264 = &mut config.encodeCodecConfig.h264Config;
            h264.idrPeriod = NVENC_INFINITE_GOPLENGTH;
            h264.set_repeatSPSPPS(1);
            h264.set_disableSPSPPS(0);
            h264.chromaFormatIDC = 1;
            set_vui(&mut h264.h264VUIParameters);
        } else {
            config.profileGUID = NV_ENC_HEVC_PROFILE_MAIN_GUID;
            let hevc = &mut config.encodeCodecConfig.hevcConfig;
            hevc.idrPeriod = NVENC_INFINITE_GOPLENGTH;
            hevc.set_repeatSPSPPS(1);
            hevc.set_disableSPSPPS(0);
            hevc.set_chromaFormatIDC(1);
            hevc.set_pixelBitDepthMinus8(0);
            set_vui(&mut hevc.hevcVUIParameters);
        }
    }
    init.encodeGUID = guid;
    init.presetGUID = NV_ENC_PRESET_P1_GUID;
    init.tuningInfo = TUNING;
    init.encodeWidth = width;
    init.encodeHeight = height;
    init.darWidth = width;
    init.darHeight = height;
    init.frameRateNum = fps;
    init.frameRateDen = 1;
    init.enableEncodeAsync = 0;
    init.enablePTD = 1;
    init.encodeConfig = config;
    // SAFETY: init and the caller's (stable) config outlive the session.
    let status = unsafe { api.nvEncInitializeEncoder.unwrap()(session, init) };
    check_status(status, "nvEncInitializeEncoder", api, session)
}

/// Apply a new bitrate to an initialized session, keeping everything else.
fn reconfigure_bitrate(
    api: &NV_ENCODE_API_FUNCTION_LIST,
    session: *mut c_void,
    config: &mut NV_ENC_CONFIG,
    init: &NV_ENC_INITIALIZE_PARAMS,
    bitrate: u32,
) -> anyhow::Result<()> {
    let old = config.rcParams.averageBitRate;
    if (bitrate as f64 - old as f64).abs() / (old as f64).max(1.0) < 0.05 {
        return Ok(());
    }
    ensure!(
        bitrate >= init.frameRateNum,
        "NVENC bitrate must provide at least one bit per frame"
    );
    let mut candidate = *config;
    set_bitrate(&mut candidate.rcParams, bitrate, init.frameRateNum);
    let mut params = params!(NV_ENC_RECONFIGURE_PARAMS, NV_ENC_RECONFIGURE_PARAMS_VER);
    params.reInitEncodeParams = *init;
    params.reInitEncodeParams.encodeConfig = &mut candidate;
    params.set_resetEncoder(0);
    params.set_forceIDR(0);
    // SAFETY: reconfiguration is synchronous; the candidate config lives through the call.
    check_status(
        unsafe { api.nvEncReconfigureEncoder.unwrap()(session, &mut params) },
        "nvEncReconfigureEncoder",
        api,
        session,
    )?;
    *config = candidate;
    Ok(())
}

/// Copy out the finished frame of a blocking encode, then unlock it.
fn read_bitstream(
    api: &NV_ENCODE_API_FUNCTION_LIST,
    session: *mut c_void,
    output_buffer: NV_ENC_OUTPUT_PTR,
) -> anyhow::Result<(bytes::Bytes, bool)> {
    let mut output = params!(NV_ENC_LOCK_BITSTREAM, NV_ENC_LOCK_BITSTREAM_VER);
    output.outputBitstream = output_buffer;
    output.set_doNotWait(0);
    // SAFETY: blocking lock waits for the submitted frame to finish.
    check_status(
        unsafe { api.nvEncLockBitstream.unwrap()(session, &mut output) },
        "nvEncLockBitstream",
        api,
        session,
    )?;
    let copied = if output.bitstreamBufferPtr.is_null() || output.bitstreamSizeInBytes == 0 {
        None
    } else {
        // SAFETY: NVENC provides this many readable bytes until the unlock.
        Some(bytes::Bytes::copy_from_slice(unsafe {
            std::slice::from_raw_parts(
                output.bitstreamBufferPtr.cast::<u8>(),
                output.bitstreamSizeInBytes as usize,
            )
        }))
    };
    // SAFETY: matches the successful lock above.
    check_status(
        unsafe { api.nvEncUnlockBitstream.unwrap()(session, output_buffer) },
        "nvEncUnlockBitstream",
        api,
        session,
    )?;
    let data = copied.context("NVENC returned an empty bitstream")?;
    Ok((data, output.pictureType == _NV_ENC_PIC_TYPE_NV_ENC_PIC_TYPE_IDR))
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

