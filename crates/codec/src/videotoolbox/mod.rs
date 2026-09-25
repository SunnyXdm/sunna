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

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
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
use crate::{Codec, DecodedFrame, EncodedFrame, Decoder, Encoder, EncoderOutput, EncoderSink};
use ffi::*;

fn cf_key(raw: core_foundation_sys::string::CFStringRef) -> CFString {
    unsafe { CFString::wrap_under_get_rule(raw) }
}


/// A submitted frame, waiting for its compression callback.
struct PendingFrame {
    frame_id: u64,
    capture_ts_us: u64,
    format: PixelFormat,
    submitted: std::time::Instant,
    keyframe_forced: bool,
    sink: EncoderSink,
}

/// State the compression callback (on a VideoToolbox thread) shares with the
/// encoder. Pending frames are looked up by token rather than handed over as
/// raw pointers, so a failed submit can't double-free.
#[derive(Default)]
struct EncoderShared {
    pending: Mutex<HashMap<usize, PendingFrame>>,
    in_flight: AtomicUsize,
    /// A forced keyframe was dropped by the encoder: force the next one.
    rearm_keyframe: AtomicBool,
    width: AtomicU32,
    height: AtomicU32,
    is_hevc: AtomicBool,
}

type OSStatusCode = i32;

const VT_ENCODE_INFO_FRAME_DROPPED: u32 = 1 << 1;

extern "C" fn compression_callback(
    refcon: *mut c_void,
    source_frame_refcon: *mut c_void,
    status: i32,
    info_flags: u32,
    sample_buffer: CMSampleBufferRef,
) {
    let shared = unsafe { &*(refcon as *const EncoderShared) };
    let token = source_frame_refcon as usize;
    let Some(pending) = shared.pending.lock().unwrap().remove(&token) else {
        return;
    };
    shared.in_flight.fetch_sub(1, Ordering::AcqRel);
    let output = if status != 0 {
        EncoderOutput::Failed {
            frame_id: pending.frame_id,
            error: format!("encode callback failed: OSStatus {status}"),
        }
    } else if info_flags & VT_ENCODE_INFO_FRAME_DROPPED != 0 || sample_buffer.is_null() {
        EncoderOutput::Dropped { frame_id: pending.frame_id }
    } else {
        match extract_sample(sample_buffer, shared.is_hevc.load(Ordering::Relaxed)) {
            Ok(sample) => EncoderOutput::Frame(to_encoded_frame(&pending, sample, shared)),
            Err(code) => EncoderOutput::Failed {
                frame_id: pending.frame_id,
                error: format!("reading encoded sample failed: OSStatus {code}"),
            },
        }
    };
    if pending.keyframe_forced && !matches!(output, EncoderOutput::Frame(_)) {
        // Don't lose a pending keyframe request to a dropped/failed frame.
        shared.rearm_keyframe.store(true, Ordering::Release);
    }
    let _ = pending.sink.send(output);
}

/// Wire format: Annex B, parameter sets prepended on keyframes.
fn to_encoded_frame(
    pending: &PendingFrame,
    sample: EncodedSample,
    shared: &EncoderShared,
) -> EncodedFrame {
    let mut annexb = Vec::with_capacity(sample.avcc.len() + 128);
    if sample.keyframe {
        for set in &sample.parameter_sets {
            h264::push_annexb_nal(&mut annexb, set);
        }
    }
    h264::avcc_to_annexb(&sample.avcc, &mut annexb);
    EncodedFrame {
        frame_id: pending.frame_id,
        codec: if shared.is_hevc.load(Ordering::Relaxed) { Codec::Hevc } else { Codec::H264 },
        keyframe: sample.keyframe,
        data: Bytes::from(annexb),
        capture_ts_us: pending.capture_ts_us,
        encode_done_ts_us: sunna_proto::now_us(),
        encode_us: pending.submitted.elapsed().as_micros() as u64,
        width: shared.width.load(Ordering::Relaxed),
        height: shared.height.load(Ordering::Relaxed),
        format: pending.format,
    }
}

struct EncodedSample {
    avcc: Vec<u8>,
    keyframe: bool,
    /// On keyframes: SPS/PPS (H.264) or VPS/SPS/PPS (HEVC), in decoder order.
    parameter_sets: Vec<Vec<u8>>,
}

fn extract_sample(
    sample_buffer: CMSampleBufferRef,
    is_hevc: bool,
) -> Result<EncodedSample, OSStatusCode> {
    let get_parameter_set = if is_hevc {
        CMVideoFormatDescriptionGetHEVCParameterSetAtIndex
    } else {
        CMVideoFormatDescriptionGetH264ParameterSetAtIndex
    };
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

        let mut parameter_sets = Vec::new();
        if keyframe {
            let desc = CMSampleBufferGetFormatDescription(sample_buffer);
            if desc.is_null() {
                return Err(-2);
            }
            let mut count = 0usize;
            let mut nal_header_len = 0i32;
            let status = get_parameter_set(
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
                let status =
                    get_parameter_set(desc, index, &mut set_ptr, &mut set_len, &mut 0, &mut 0);
                if status != 0 {
                    return Err(status);
                }
                let set = std::slice::from_raw_parts(set_ptr, set_len).to_vec();
                if h264::parameter_set_rank(&set, is_hevc).is_some() {
                    parameter_sets.push(set);
                }
            }
            parameter_sets.sort_by_key(|set| h264::parameter_set_rank(set, is_hevc));
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
            parameter_sets,
        })
    }
}

pub struct VtEncoder {
    session: VTCompressionSessionRef,
    shared: Box<EncoderShared>,
    fps: u32,
    force_keyframe: bool,
    next_token: usize,
    /// Standard (not low-latency) session: rate spikes are capped with
    /// DataRateLimits, which low-latency mode doesn't take.
    standard_session: bool,
}

// The session is owned and driven from exactly one thread at a time; the
// shared state is synchronized for the callback thread.
unsafe impl Send for VtEncoder {}

impl VtEncoder {
    pub fn new(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        bitrate_bps: u32,
    ) -> anyhow::Result<Self> {
        let (codec_type, profile) = match codec {
            Codec::H264 => (kCMVideoCodecType_H264, unsafe { kVTProfileLevel_H264_High_AutoLevel }),
            Codec::Hevc => (kCMVideoCodecType_HEVC, unsafe { kVTProfileLevel_HEVC_Main_AutoLevel }),
            other => bail!("VideoToolbox encoder: unsupported codec {}", other.name()),
        };
        let shared = Box::<EncoderShared>::default();
        shared.is_hevc.store(codec == Codec::Hevc, Ordering::Relaxed);
        let refcon = &*shared as *const EncoderShared as *mut c_void;

        let low_latency_spec = CFDictionary::from_CFType_pairs(&[(
            cf_key(unsafe { kVTVideoEncoderSpecification_EnableLowLatencyRateControl })
                .as_CFType(),
            CFBoolean::true_value().as_CFType(),
        )]);

        // Low-latency rate control by default. Full-res (2846x1778) encode on
        // an M1, dogfood builds 5-8: low-latency ~23.6 ms (good quality);
        // standard + PrioritizeSpeed ~19 ms (visibly softer); standard
        // quality-first ~28.5 ms. SUNNA_VT_LOW_LATENCY=0 for the standard
        // session.
        let low_latency = std::env::var("SUNNA_VT_LOW_LATENCY").map_or(true, |value| value != "0");
        let spec: *const c_void = if low_latency {
            low_latency_spec.as_concrete_TypeRef() as _
        } else {
            ptr::null()
        };
        tracing::info!(codec = codec.name(), low_latency, width, height, "creating encoder");
        let mut session: VTCompressionSessionRef = ptr::null_mut();
        let mut status = unsafe {
            VTCompressionSessionCreate(
                ptr::null(),
                width as i32,
                height as i32,
                codec_type,
                spec as _,
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
                    codec_type,
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
            if status == 0 {
                tracing::debug!(label, "encoder property applied");
            } else {
                tracing::info!(status, label, "encoder property NOT applied");
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
                // H.264 High: 8x8 transforms compress text and UI noticeably
                // better than Main; every Apple decoder has it. HEVC Main.
                profile as _,
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
            // Tag the stream Rec. 709 (matching the capture's matrix) so the
            // decoder converts back to RGB with the same matrix.
            set(
                kVTCompressionPropertyKey_ColorPrimaries,
                kCVImageBufferColorPrimaries_ITU_R_709_2 as _,
                "ColorPrimaries",
            );
            set(
                kVTCompressionPropertyKey_TransferFunction,
                kCVImageBufferTransferFunction_ITU_R_709_2 as _,
                "TransferFunction",
            );
            set(
                kVTCompressionPropertyKey_YCbCrMatrix,
                kCVImageBufferYCbCrMatrix_ITU_R_709_2 as _,
                "YCbCrMatrix",
            );
            // MaxFrameDelayCount and PrioritizeEncodingSpeedOverQuality both
            // return kVTPropertyNotSupportedErr (-12900) in the low-latency
            // session on an M1 (dogfood, macOS 15.6); try them only in the
            // standard session.
            if !low_latency {
                set(
                    kVTCompressionPropertyKey_MaxFrameDelayCount,
                    CFNumber::from(0).as_CFTypeRef(),
                    "MaxFrameDelayCount",
                );
                // Accepted in the standard session, but it visibly softened
                // the picture (dogfood session A); opt-in only.
                if std::env::var("SUNNA_VT_PRIORITIZE_SPEED").is_ok_and(|value| value == "1") {
                    set(
                        kVTCompressionPropertyKey_PrioritizeEncodingSpeedOverQuality,
                        CFBoolean::true_value().as_CFTypeRef(),
                        "PrioritizeEncodingSpeedOverQuality",
                    );
                }
                // AverageBitRate alone let peaks reach ~46 Mbps against a
                // 40 Mbps target; bursts like that clog Wi-Fi queues.
                let status = VTSessionSetProperty(
                    session,
                    kVTCompressionPropertyKey_DataRateLimits,
                    data_rate_limits(bitrate_bps).as_CFTypeRef(),
                );
                if status != 0 {
                    tracing::info!(status, "encoder property NOT applied: DataRateLimits");
                }
            }
            let status = VTCompressionSessionPrepareToEncodeFrames(session);
            if status != 0 {
                tracing::debug!(status, "PrepareToEncodeFrames failed (non-fatal)");
            }
        }

        shared.width.store(width, Ordering::Relaxed);
        shared.height.store(height, Ordering::Relaxed);
        Ok(Self {
            session,
            shared,
            fps,
            force_keyframe: true,
            next_token: 1,
            standard_session: !low_latency,
        })
    }

    /// A +1 retained pixel buffer for `frame`; the caller releases it.
    fn pixel_buffer_for(&self, frame: &VideoFrame) -> anyhow::Result<CVPixelBufferRef> {
        let bytes = match &frame.data {
            // Any surface format VideoToolbox accepts (BGRA or NV12) goes in
            // as-is.
            FrameData::Surface(surface) => {
                let pixel_buffer = surface.pixel_buffer();
                unsafe { CFRetain(pixel_buffer as _) };
                return Ok(pixel_buffer);
            }
            FrameData::Cpu(bytes) => bytes,
        };
        anyhow::ensure!(
            frame.format == PixelFormat::Bgra8,
            "CPU frames must be BGRA"
        );
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

/// At most 1.5x the target over any one second.
fn data_rate_limits(
    bitrate_bps: u32,
) -> core_foundation::array::CFArray<core_foundation::base::CFType> {
    let bytes_per_second = (bitrate_bps as f64 * 1.5 / 8.0) as i64;
    core_foundation::array::CFArray::from_CFTypes(&[
        CFNumber::from(bytes_per_second).as_CFType(),
        CFNumber::from(1.0).as_CFType(),
    ])
}

impl Encoder for VtEncoder {
    fn encode(&mut self, frame: &VideoFrame) -> anyhow::Result<Option<EncodedFrame>> {
        let (sink, outputs) = std::sync::mpsc::channel();
        self.submit(frame, &sink)?;
        let status = unsafe { VTCompressionSessionCompleteFrames(self.session, CMTime::invalid()) };
        if status != 0 {
            bail!("VTCompressionSessionCompleteFrames failed: {status}");
        }
        match outputs.try_recv().context("encoder produced no output")? {
            EncoderOutput::Frame(encoded) => Ok(Some(encoded)),
            EncoderOutput::Dropped { .. } => Ok(None),
            EncoderOutput::Failed { error, .. } => bail!(error),
        }
    }

    fn submit(&mut self, frame: &VideoFrame, sink: &EncoderSink) -> anyhow::Result<()> {
        let pixel_buffer = self.pixel_buffer_for(frame)?;
        let keyframe_wanted =
            self.force_keyframe || self.shared.rearm_keyframe.swap(false, Ordering::AcqRel);
        self.force_keyframe = false;
        let frame_properties = keyframe_wanted.then(|| {
            CFDictionary::from_CFType_pairs(&[(
                cf_key(unsafe { kVTEncodeFrameOptionKey_ForceKeyFrame }).as_CFType(),
                CFBoolean::true_value().as_CFType(),
            )])
        });

        let token = self.next_token;
        self.next_token = self.next_token.wrapping_add(1).max(1);
        self.shared.pending.lock().unwrap().insert(
            token,
            PendingFrame {
                frame_id: frame.frame_id,
                capture_ts_us: frame.capture_ts_us,
                format: frame.format,
                submitted: std::time::Instant::now(),
                keyframe_forced: keyframe_wanted,
                sink: sink.clone(),
            },
        );
        self.shared.in_flight.fetch_add(1, Ordering::AcqRel);

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
                token as *mut c_void,
                ptr::null_mut(),
            )
        };
        unsafe { CFRelease(pixel_buffer as _) };
        if status != 0 {
            // If the callback didn't already take it, this submit is ours to undo.
            if self.shared.pending.lock().unwrap().remove(&token).is_some() {
                self.shared.in_flight.fetch_sub(1, Ordering::AcqRel);
            }
            self.force_keyframe |= keyframe_wanted;
            bail!("VTCompressionSessionEncodeFrame failed: {status}");
        }
        Ok(())
    }

    fn in_flight(&self) -> usize {
        self.shared.in_flight.load(Ordering::Acquire)
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
        if self.standard_session {
            unsafe {
                VTSessionSetProperty(
                    self.session,
                    kVTCompressionPropertyKey_DataRateLimits,
                    data_rate_limits(bits_per_second).as_CFTypeRef(),
                );
            }
        }
    }

    fn request_keyframe(&mut self) {
        self.force_keyframe = true;
    }
}

impl Drop for VtEncoder {
    fn drop(&mut self) {
        unsafe {
            // Flush so every pending frame's callback runs (and its sink
            // clone is dropped) before the shared state goes away.
            VTCompressionSessionCompleteFrames(self.session, CMTime::invalid());
            VTCompressionSessionInvalidate(self.session);
            CFRelease(self.session as _);
        }
        self.shared.pending.lock().unwrap().clear();
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
    is_hevc: bool,
    session: VTDecompressionSessionRef,
    format_desc: CMFormatDescriptionRef,
    shared: Box<DecoderShared>,
}

unsafe impl Send for VtDecoder {}

impl VtDecoder {
    /// Created lazily: the session needs parameter sets, which arrive with
    /// the first keyframe.
    pub fn new(codec: Codec) -> Self {
        Self {
            is_hevc: codec == Codec::Hevc,
            session: ptr::null_mut(),
            format_desc: ptr::null_mut(),
            shared: Box::default(),
        }
    }

    fn ensure_session(&mut self, sets: &[&[u8]]) -> anyhow::Result<()> {
        if !self.session.is_null() {
            return Ok(());
        }
        let needed = if self.is_hevc { 3 } else { 2 };
        anyhow::ensure!(
            sets.len() >= needed,
            "waiting for a keyframe with parameter sets"
        );

        let pointers: Vec<*const u8> = sets.iter().map(|set| set.as_ptr()).collect();
        let sizes: Vec<usize> = sets.iter().map(|set| set.len()).collect();
        let mut format_desc: CMFormatDescriptionRef = ptr::null_mut();
        let status = unsafe {
            if self.is_hevc {
                CMVideoFormatDescriptionCreateFromHEVCParameterSets(
                    ptr::null(),
                    sets.len(),
                    pointers.as_ptr(),
                    sizes.as_ptr(),
                    4,
                    ptr::null(),
                    &mut format_desc,
                )
            } else {
                CMVideoFormatDescriptionCreateFromH264ParameterSets(
                    ptr::null(),
                    sets.len(),
                    pointers.as_ptr(),
                    sizes.as_ptr(),
                    4,
                    &mut format_desc,
                )
            }
        };
        if status != 0 || format_desc.is_null() {
            bail!("creating the decoder format description failed: {status}");
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
        Self::new(Codec::H264)
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
        let parsed = h264::annexb_to_access_unit(data, self.is_hevc);
        self.ensure_session(&parsed.parameter_sets)?;
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
