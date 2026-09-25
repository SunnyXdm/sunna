//! VideoToolbox low-latency H.264 encoder/decoder (macOS).
//!
//! Settings per research/03 §2: low-latency rate control, real-time, no frame
//! reordering (no B-frames), so the session is one-in-one-out. Encode forces
//! completion per frame (`VTCompressionSessionCompleteFrames`) and decode is
//! synchronous by default, which keeps both trait impls blocking and simple.
//!
//! Encode input is zero-copy for captured frames (the capture's
//! IOSurface-backed CVPixelBuffer goes straight in); CPU frames from the
//! synthetic source are copied into a pixel buffer. Decode output is also
//! zero-copy: IOSurface-backed BGRA pixel buffers the viewer displays
//! directly.

mod ffi;

use std::ffi::c_void;
use std::ptr;
use std::sync::Mutex;

use anyhow::{bail, Context};
use bytes::Bytes;
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use sunna_capture::macos::SurfaceFrame;
use sunna_capture::{FrameData, PixelFormat, VideoFrame};

use crate::h264;
use crate::{Codec, DecodedFrame, EncodedFrame, Decoder, Encoder};
use ffi::*;

fn cf_key(raw: core_foundation_sys::string::CFStringRef) -> CFString {
    unsafe { CFString::wrap_under_get_rule(raw) }
}

struct EncodedSample {
    avcc: Vec<u8>,
    keyframe: bool,
    sps: Vec<Vec<u8>>,
    pps: Vec<Vec<u8>>,
}

/// The encoder legitimately drops frames under load/rate-control pressure
/// (null sample buffer or the FrameDropped info flag) — that is not a failure.
enum EncodeOutcome {
    Sample(EncodedSample),
    Dropped,
    Failed(OSStatusCode),
}

#[derive(Default)]
struct EncoderShared {
    result: Mutex<Option<EncodeOutcome>>,
}

type OSStatusCode = i32;

const VT_ENCODE_INFO_FRAME_DROPPED: u32 = 1 << 1;

extern "C" fn compression_callback(
    refcon: *mut c_void,
    _source_frame_refcon: *mut c_void,
    status: i32,
    info_flags: u32,
    sample_buffer: CMSampleBufferRef,
) {
    let shared = unsafe { &*(refcon as *const EncoderShared) };
    let outcome = if status != 0 {
        EncodeOutcome::Failed(status)
    } else if info_flags & VT_ENCODE_INFO_FRAME_DROPPED != 0 || sample_buffer.is_null() {
        EncodeOutcome::Dropped
    } else {
        match extract_sample(sample_buffer) {
            Ok(sample) => EncodeOutcome::Sample(sample),
            Err(code) => EncodeOutcome::Failed(code),
        }
    };
    *shared.result.lock().unwrap() = Some(outcome);
}

fn extract_sample(sample_buffer: CMSampleBufferRef) -> Result<EncodedSample, OSStatusCode> {
    unsafe {
        // Keyframe: absence of the NotSync attachment (or NotSync == false).
        let attachments = CMSampleBufferGetSampleAttachmentsArray(sample_buffer, 0);
        let keyframe = if attachments.is_null() || CFArrayGetCount(attachments) == 0 {
            true
        } else {
            let dict = CFArrayGetValueAtIndex(attachments, 0)
                as core_foundation_sys::dictionary::CFDictionaryRef;
            let not_sync =
                CFDictionaryGetValue(dict, kCMSampleAttachmentKey_NotSync as *const c_void);
            not_sync.is_null() || CFBooleanGetValue(not_sync as _) == 0
        };

        let mut sps = Vec::new();
        let mut pps = Vec::new();
        if keyframe {
            let desc = CMSampleBufferGetFormatDescription(sample_buffer);
            if desc.is_null() {
                return Err(-2);
            }
            let mut count = 0usize;
            let mut nal_header_len = 0i32;
            let status = CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                desc,
                0,
                &mut ptr::null(),
                &mut 0,
                &mut count,
                &mut nal_header_len,
            );
            if status != 0 {
                return Err(status);
            }
            for index in 0..count {
                let mut set_ptr: *const u8 = ptr::null();
                let mut set_len = 0usize;
                let status = CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                    desc,
                    index,
                    &mut set_ptr,
                    &mut set_len,
                    &mut 0,
                    &mut 0,
                );
                if status != 0 {
                    return Err(status);
                }
                let set = std::slice::from_raw_parts(set_ptr, set_len).to_vec();
                match h264::nal_type(&set) {
                    Some(h264::NAL_SPS) => sps.push(set),
                    Some(h264::NAL_PPS) => pps.push(set),
                    _ => {}
                }
            }
        }

        let block = CMSampleBufferGetDataBuffer(sample_buffer);
        if block.is_null() {
            return Err(-3);
        }
        let len = CMBlockBufferGetDataLength(block);
        let mut avcc = vec![0u8; len];
        let status = CMBlockBufferCopyDataBytes(block, 0, len, avcc.as_mut_ptr() as *mut c_void);
        if status != 0 {
            return Err(status);
        }
        Ok(EncodedSample {
            avcc,
            keyframe,
            sps,
            pps,
        })
    }
}

pub struct VtEncoder {
    session: VTCompressionSessionRef,
    shared: Box<EncoderShared>,
    width: u32,
    height: u32,
    fps: u32,
    force_keyframe: bool,
}

// The session is owned and driven from exactly one thread at a time; the
// shared slot is Mutex-guarded for the callback thread.
unsafe impl Send for VtEncoder {}

impl VtEncoder {
    pub fn new(width: u32, height: u32, fps: u32, bitrate_bps: u32) -> anyhow::Result<Self> {
        let shared = Box::<EncoderShared>::default();
        let refcon = &*shared as *const EncoderShared as *mut c_void;

        let low_latency_spec = CFDictionary::from_CFType_pairs(&[(
            cf_key(unsafe { kVTVideoEncoderSpecification_EnableLowLatencyRateControl })
                .as_CFType(),
            CFBoolean::true_value().as_CFType(),
        )]);

        let mut session: VTCompressionSessionRef = ptr::null_mut();
        let mut status = unsafe {
            VTCompressionSessionCreate(
                ptr::null(),
                width as i32,
                height as i32,
                kCMVideoCodecType_H264,
                low_latency_spec.as_concrete_TypeRef() as _,
                ptr::null(),
                ptr::null(),
                compression_callback,
                refcon,
                &mut session,
            )
        };
        if status != 0 {
            tracing::warn!(status, "low-latency encoder unavailable, using standard session");
            status = unsafe {
                VTCompressionSessionCreate(
                    ptr::null(),
                    width as i32,
                    height as i32,
                    kCMVideoCodecType_H264,
                    ptr::null(),
                    ptr::null(),
                    ptr::null(),
                    compression_callback,
                    refcon,
                    &mut session,
                )
            };
        }
        if status != 0 || session.is_null() {
            bail!("VTCompressionSessionCreate failed: {status}");
        }

        let set = |key: core_foundation_sys::string::CFStringRef,
                   value: core_foundation_sys::base::CFTypeRef,
                   label: &str| {
            let status = unsafe { VTSessionSetProperty(session, key, value) };
            if status != 0 {
                tracing::debug!(status, label, "encoder property not applied");
            }
        };
        unsafe {
            set(
                kVTCompressionPropertyKey_RealTime,
                CFBoolean::true_value().as_CFTypeRef(),
                "RealTime",
            );
            set(
                kVTCompressionPropertyKey_AllowFrameReordering,
                CFBoolean::false_value().as_CFTypeRef(),
                "AllowFrameReordering",
            );
            set(
                kVTCompressionPropertyKey_ProfileLevel,
                // High: 8x8 transforms compress text and UI noticeably better
                // than Main at the same bitrate; every Apple decoder has it.
                kVTProfileLevel_H264_High_AutoLevel as _,
                "ProfileLevel",
            );
            set(
                kVTCompressionPropertyKey_AverageBitRate,
                CFNumber::from(bitrate_bps as i64).as_CFTypeRef(),
                "AverageBitRate",
            );
            set(
                kVTCompressionPropertyKey_MaxKeyFrameInterval,
                CFNumber::from(i32::MAX).as_CFTypeRef(),
                "MaxKeyFrameInterval",
            );
            set(
                kVTCompressionPropertyKey_ExpectedFrameRate,
                CFNumber::from(fps as i32).as_CFTypeRef(),
                "ExpectedFrameRate",
            );
            let status = VTCompressionSessionPrepareToEncodeFrames(session);
            if status != 0 {
                tracing::debug!(status, "PrepareToEncodeFrames failed (non-fatal)");
            }
        }

        Ok(Self {
            session,
            shared,
            width,
            height,
            fps,
            force_keyframe: true,
        })
    }

    /// A +1 retained pixel buffer for `frame`; the caller releases it.
    fn pixel_buffer_for(&self, frame: &VideoFrame) -> anyhow::Result<CVPixelBufferRef> {
        anyhow::ensure!(
            frame.format == PixelFormat::Bgra8,
            "VtEncoder expects BGRA input"
        );
        let bytes = match &frame.data {
            FrameData::Surface(surface) => {
                let pixel_buffer = surface.pixel_buffer();
                unsafe { CFRetain(pixel_buffer as _) };
                return Ok(pixel_buffer);
            }
            FrameData::Cpu(bytes) => bytes,
        };
        let (width, height) = (frame.width as usize, frame.height as usize);
        anyhow::ensure!(
            bytes.len() >= width * height * 4,
            "frame data smaller than {width}x{height} BGRA"
        );
        let mut pixel_buffer: CVPixelBufferRef = ptr::null_mut();
        let status = unsafe {
            CVPixelBufferCreate(
                ptr::null(),
                width,
                height,
                kCVPixelFormatType_32BGRA,
                ptr::null(),
                &mut pixel_buffer,
            )
        };
        if status != 0 || pixel_buffer.is_null() {
            bail!("CVPixelBufferCreate failed: {status}");
        }
        unsafe {
            CVPixelBufferLockBaseAddress(pixel_buffer, 0);
            let base = CVPixelBufferGetBaseAddress(pixel_buffer) as *mut u8;
            let stride = CVPixelBufferGetBytesPerRow(pixel_buffer);
            let src_stride = width * 4;
            for row in 0..height {
                ptr::copy_nonoverlapping(
                    bytes.as_ptr().add(row * src_stride),
                    base.add(row * stride),
                    src_stride,
                );
            }
            CVPixelBufferUnlockBaseAddress(pixel_buffer, 0);
        }
        Ok(pixel_buffer)
    }
}

impl Encoder for VtEncoder {
    fn encode(&mut self, frame: &VideoFrame) -> anyhow::Result<Option<EncodedFrame>> {
        let pixel_buffer = self.pixel_buffer_for(frame)?;
        self.shared.result.lock().unwrap().take();

        let keyframe_wanted = self.force_keyframe;
        let frame_properties = if keyframe_wanted {
            Some(CFDictionary::from_CFType_pairs(&[(
                cf_key(unsafe { kVTEncodeFrameOptionKey_ForceKeyFrame }).as_CFType(),
                CFBoolean::true_value().as_CFType(),
            )]))
        } else {
            None
        };
        self.force_keyframe = false;

        let pts = CMTime::new(frame.frame_id as i64, self.fps.max(1) as i32);
        let duration = CMTime::new(1, self.fps.max(1) as i32);
        let status = unsafe {
            VTCompressionSessionEncodeFrame(
                self.session,
                pixel_buffer,
                pts,
                duration,
                frame_properties
                    .as_ref()
                    .map(|dict| dict.as_concrete_TypeRef() as _)
                    .unwrap_or(ptr::null()),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if status != 0 {
            unsafe { CFRelease(pixel_buffer as _) };
            bail!("VTCompressionSessionEncodeFrame failed: {status}");
        }
        let status = unsafe { VTCompressionSessionCompleteFrames(self.session, CMTime::invalid()) };
        unsafe { CFRelease(pixel_buffer as _) };
        if status != 0 {
            bail!("VTCompressionSessionCompleteFrames failed: {status}");
        }

        let outcome = self
            .shared
            .result
            .lock()
            .unwrap()
            .take()
            .context("encoder produced no output")?;
        let sample = match outcome {
            EncodeOutcome::Sample(sample) => sample,
            EncodeOutcome::Dropped => {
                // Don't lose a pending keyframe request to a dropped frame.
                self.force_keyframe |= keyframe_wanted;
                return Ok(None);
            }
            EncodeOutcome::Failed(status) => {
                self.force_keyframe |= keyframe_wanted;
                anyhow::bail!("encode callback failed: OSStatus {status}");
            }
        };

        // Wire format: Annex B, parameter sets prepended on keyframes.
        let mut annexb = Vec::with_capacity(sample.avcc.len() + 128);
        if sample.keyframe {
            for set in sample.sps.iter().chain(sample.pps.iter()) {
                h264::push_annexb_nal(&mut annexb, set);
            }
        }
        h264::avcc_to_annexb(&sample.avcc, &mut annexb);

        Ok(Some(EncodedFrame {
            frame_id: frame.frame_id,
            codec: Codec::H264,
            keyframe: sample.keyframe,
            data: Bytes::from(annexb),
            capture_ts_us: frame.capture_ts_us,
            encode_done_ts_us: sunna_proto::now_us(),
            width: self.width,
            height: self.height,
            format: frame.format,
        }))
    }

    fn set_target_bitrate(&mut self, bits_per_second: u32) {
        let status = unsafe {
            VTSessionSetProperty(
                self.session,
                kVTCompressionPropertyKey_AverageBitRate,
                CFNumber::from(bits_per_second as i64).as_CFTypeRef(),
            )
        };
        if status != 0 {
            tracing::debug!(status, "bitrate update not applied");
        }
    }

    fn request_keyframe(&mut self) {
        self.force_keyframe = true;
    }
}

impl Drop for VtEncoder {
    fn drop(&mut self) {
        unsafe {
            VTCompressionSessionInvalidate(self.session);
            CFRelease(self.session as _);
        }
    }
}

#[derive(Default)]
struct DecoderShared {
    result: Mutex<Option<Result<SurfaceFrame, OSStatusCode>>>,
}

extern "C" fn decompression_callback(
    refcon: *mut c_void,
    _source_frame_refcon: *mut c_void,
    status: i32,
    _info_flags: u32,
    image_buffer: CVImageBufferRef,
    _pts: CMTime,
    _duration: CMTime,
) {
    let shared = unsafe { &*(refcon as *const DecoderShared) };
    let outcome = if status != 0 {
        Err(status)
    } else if image_buffer.is_null() {
        Err(-1)
    } else {
        // Keep the decoder's IOSurface-backed buffer; no pixel copy.
        unsafe { SurfaceFrame::from_pixel_buffer(image_buffer) }.ok_or(-4)
    };
    *shared.result.lock().unwrap() = Some(outcome);
}

pub struct VtDecoder {
    session: VTDecompressionSessionRef,
    format_desc: CMFormatDescriptionRef,
    shared: Box<DecoderShared>,
}

unsafe impl Send for VtDecoder {}

impl VtDecoder {
    /// Created lazily: the session needs SPS/PPS, which arrive with the first keyframe.
    pub fn new() -> Self {
        Self {
            session: ptr::null_mut(),
            format_desc: ptr::null_mut(),
            shared: Box::default(),
        }
    }

    fn ensure_session(&mut self, sps: &[&[u8]], pps: &[&[u8]]) -> anyhow::Result<()> {
        if !self.session.is_null() {
            return Ok(());
        }
        anyhow::ensure!(
            !sps.is_empty() && !pps.is_empty(),
            "waiting for keyframe with SPS/PPS"
        );

        let sets: Vec<&[u8]> = sps.iter().chain(pps.iter()).copied().collect();
        let pointers: Vec<*const u8> = sets.iter().map(|set| set.as_ptr()).collect();
        let sizes: Vec<usize> = sets.iter().map(|set| set.len()).collect();
        let mut format_desc: CMFormatDescriptionRef = ptr::null_mut();
        let status = unsafe {
            CMVideoFormatDescriptionCreateFromH264ParameterSets(
                ptr::null(),
                sets.len(),
                pointers.as_ptr(),
                sizes.as_ptr(),
                4,
                &mut format_desc,
            )
        };
        if status != 0 || format_desc.is_null() {
            bail!("CMVideoFormatDescriptionCreateFromH264ParameterSets failed: {status}");
        }

        // BGRA in IOSurfaces: Core Animation can show these directly, and
        // the YUV→RGB conversion happens on the GPU inside VideoToolbox.
        let empty = CFDictionary::<CFString, CFNumber>::from_CFType_pairs(&[]);
        let dest_attrs = CFDictionary::from_CFType_pairs(&[
            (
                cf_key(unsafe { kCVPixelBufferPixelFormatTypeKey }).as_CFType(),
                CFNumber::from(kCVPixelFormatType_32BGRA as i64).as_CFType(),
            ),
            (
                cf_key(unsafe { kCVPixelBufferIOSurfacePropertiesKey }).as_CFType(),
                empty.as_CFType(),
            ),
        ]);
        let record = VTDecompressionOutputCallbackRecord {
            decompressionOutputCallback: decompression_callback,
            decompressionOutputRefCon: &*self.shared as *const DecoderShared as *mut c_void,
        };
        let mut session: VTDecompressionSessionRef = ptr::null_mut();
        let status = unsafe {
            VTDecompressionSessionCreate(
                ptr::null(),
                format_desc,
                ptr::null(),
                dest_attrs.as_concrete_TypeRef() as _,
                &record,
                &mut session,
            )
        };
        if status != 0 || session.is_null() {
            unsafe { CFRelease(format_desc as _) };
            bail!("VTDecompressionSessionCreate failed: {status}");
        }
        self.session = session;
        self.format_desc = format_desc;
        Ok(())
    }
}

impl Default for VtDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder for VtDecoder {
    fn decode(
        &mut self,
        frame_id: u64,
        capture_ts_us: u64,
        _keyframe: bool,
        data: &[u8],
    ) -> anyhow::Result<DecodedFrame> {
        let parsed = h264::annexb_to_avcc(data);
        self.ensure_session(&parsed.sps, &parsed.pps)?;
        anyhow::ensure!(!parsed.avcc.is_empty(), "frame contained no VCL NAL units");

        unsafe {
            let mut block: CMBlockBufferRef = ptr::null_mut();
            let status = CMBlockBufferCreateWithMemoryBlock(
                ptr::null(),
                ptr::null_mut(),
                parsed.avcc.len(),
                ptr::null(), // default allocator
                ptr::null(),
                0,
                parsed.avcc.len(),
                0,
                &mut block,
            );
            if status != 0 || block.is_null() {
                bail!("CMBlockBufferCreateWithMemoryBlock failed: {status}");
            }
            let status = CMBlockBufferReplaceDataBytes(
                parsed.avcc.as_ptr() as *const c_void,
                block,
                0,
                parsed.avcc.len(),
            );
            if status != 0 {
                CFRelease(block as _);
                bail!("CMBlockBufferReplaceDataBytes failed: {status}");
            }

            let timing = CMSampleTimingInfo {
                duration: CMTime::invalid(),
                presentationTimeStamp: CMTime::new(frame_id as i64, 1_000_000),
                decodeTimeStamp: CMTime::invalid(),
            };
            let sizes = [parsed.avcc.len()];
            let mut sample: CMSampleBufferRef = ptr::null_mut();
            let status = CMSampleBufferCreate(
                ptr::null(),
                block,
                1,
                ptr::null(),
                ptr::null_mut(),
                self.format_desc,
                1,
                1,
                &timing,
                1,
                sizes.as_ptr(),
                &mut sample,
            );
            if status != 0 || sample.is_null() {
                CFRelease(block as _);
                bail!("CMSampleBufferCreate failed: {status}");
            }

            self.shared.result.lock().unwrap().take();
            // Flag 0 → synchronous: the output callback runs before this returns.
            let status =
                VTDecompressionSessionDecodeFrame(self.session, sample, 0, ptr::null_mut(), ptr::null_mut());
            CFRelease(sample as _);
            CFRelease(block as _);
            if status != 0 {
                bail!("VTDecompressionSessionDecodeFrame failed: {status}");
            }
        }

        let surface = self
            .shared
            .result
            .lock()
            .unwrap()
            .take()
            .context("decoder produced no output")?
            .map_err(|status| anyhow::anyhow!("decode callback failed: OSStatus {status}"))?;

        Ok(DecodedFrame {
            frame_id,
            width: surface.width(),
            height: surface.height(),
            format: PixelFormat::Bgra8,
            data: FrameData::Surface(surface),
            capture_ts_us,
        })
    }
}

impl Drop for VtDecoder {
    fn drop(&mut self) {
        unsafe {
            if !self.session.is_null() {
                VTDecompressionSessionInvalidate(self.session);
                CFRelease(self.session as _);
            }
            if !self.format_desc.is_null() {
                CFRelease(self.format_desc as _);
            }
        }
    }
}
