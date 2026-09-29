// A small, stable C face on FFmpeg's decoder, compiled against whatever
// libavcodec the system has (4.4 through 9 so far): the struct layouts are
// the compiler's problem, not ours. One decoder, one frame out per packet,
// hardware first (NVIDIA through CUDA, then VA-API for Intel and AMD), the
// software decoder last.

#include <libavcodec/avcodec.h>
#include <libavutil/hwcontext.h>
#include <libavutil/pixdesc.h>
#include <libavutil/opt.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    int width, height;
    // 0: NV12 (Y plane, then interleaved UV), 1: I420 (Y, U, V).
    int layout;
    const uint8_t *planes[3];
    int strides[3];
    int full_range;
    // 0: BT.601, 1: BT.709, 2: BT.2020.
    int matrix;
} sunna_ff_frame;

typedef struct {
    AVCodecContext *ctx;
    AVPacket *packet;
    AVFrame *frame;
    AVFrame *soft;
    enum AVPixelFormat hw_format;
    const char *hw_name;
} sunna_ff;

static enum AVPixelFormat pick_format(AVCodecContext *ctx, const enum AVPixelFormat *offered) {
    sunna_ff *dec = ctx->opaque;
    for (const enum AVPixelFormat *p = offered; *p != AV_PIX_FMT_NONE; p++) {
        if (*p == dec->hw_format) return *p;
    }
    // The hardware can't take this stream (a profile it lacks): software.
    for (const enum AVPixelFormat *p = offered; *p != AV_PIX_FMT_NONE; p++) {
        const AVPixFmtDescriptor *desc = av_pix_fmt_desc_get(*p);
        if (desc && !(desc->flags & AV_PIX_FMT_FLAG_HWACCEL)) {
            dec->hw_name = "software";
            return *p;
        }
    }
    return AV_PIX_FMT_NONE;
}

static void say(char *error, int size, const char *what, int code) {
    if (!error || size <= 0) return;
    char text[128] = {0};
    if (code) av_strerror(code, text, sizeof text);
    snprintf(error, size, "%s%s%s", what, code ? ": " : "", text);
}

// Can this device decode `id`? Fills the pixel format it would hand back.
static int hw_config(const AVCodec *codec, enum AVHWDeviceType type, enum AVPixelFormat *format) {
    for (int i = 0;; i++) {
        const AVCodecHWConfig *config = avcodec_get_hw_config(codec, i);
        if (!config) return 0;
        if ((config->methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX) && config->device_type == type) {
            *format = config->pix_fmt;
            return 1;
        }
    }
}

// hevc: 0 for H.264, 1 for HEVC. hardware: 0 software only, 1 try hardware.
sunna_ff *sunna_ff_open(int hevc, int hardware, char *error, int error_size) {
    const AVCodec *codec = avcodec_find_decoder(hevc ? AV_CODEC_ID_HEVC : AV_CODEC_ID_H264);
    if (!codec) {
        say(error, error_size, hevc ? "this FFmpeg has no HEVC decoder" : "this FFmpeg has no H.264 decoder", 0);
        return NULL;
    }
    sunna_ff *dec = calloc(1, sizeof *dec);
    if (!dec) return NULL;
    dec->hw_format = AV_PIX_FMT_NONE;
    dec->hw_name = "software";
    dec->ctx = avcodec_alloc_context3(codec);
    dec->packet = av_packet_alloc();
    dec->frame = av_frame_alloc();
    dec->soft = av_frame_alloc();
    if (!dec->ctx || !dec->packet || !dec->frame || !dec->soft) {
        say(error, error_size, "out of memory", 0);
        goto fail;
    }
    dec->ctx->opaque = dec;
    // Every frame out as soon as it's in: no reordering delay, and slice
    // threads only (frame threads hold frames back).
    dec->ctx->flags |= AV_CODEC_FLAG_LOW_DELAY;
    dec->ctx->thread_type = FF_THREAD_SLICE;
    dec->ctx->thread_count = 0;

    if (hardware) {
        static const struct { enum AVHWDeviceType type; const char *name; } order[] = {
            { AV_HWDEVICE_TYPE_CUDA, "nvdec" },
            { AV_HWDEVICE_TYPE_VAAPI, "vaapi" },
        };
        for (size_t i = 0; i < sizeof order / sizeof order[0]; i++) {
            enum AVPixelFormat format;
            if (!hw_config(codec, order[i].type, &format)) continue;
            AVBufferRef *device = NULL;
            if (av_hwdevice_ctx_create(&device, order[i].type, NULL, NULL, 0) < 0) continue;
            dec->ctx->hw_device_ctx = device;
            dec->hw_format = format;
            dec->hw_name = order[i].name;
            dec->ctx->get_format = pick_format;
            break;
        }
    }
    int code = avcodec_open2(dec->ctx, codec, NULL);
    if (code < 0) {
        say(error, error_size, "couldn't open the decoder", code);
        goto fail;
    }
    return dec;
fail:
    if (dec->ctx) avcodec_free_context(&dec->ctx);
    av_packet_free(&dec->packet);
    av_frame_free(&dec->frame);
    av_frame_free(&dec->soft);
    free(dec);
    return NULL;
}

const char *sunna_ff_backend(sunna_ff *dec) { return dec->hw_name; }

// Is hardware decoding of this codec possible here? (Opens a device.)
int sunna_ff_hardware_available(int hevc) {
    const AVCodec *codec = avcodec_find_decoder(hevc ? AV_CODEC_ID_HEVC : AV_CODEC_ID_H264);
    if (!codec) return 0;
    const enum AVHWDeviceType types[] = { AV_HWDEVICE_TYPE_CUDA, AV_HWDEVICE_TYPE_VAAPI };
    for (size_t i = 0; i < sizeof types / sizeof types[0]; i++) {
        enum AVPixelFormat format;
        if (!hw_config(codec, types[i], &format)) continue;
        AVBufferRef *device = NULL;
        if (av_hwdevice_ctx_create(&device, types[i], NULL, NULL, 0) >= 0) {
            av_buffer_unref(&device);
            return 1;
        }
    }
    return 0;
}

int sunna_ff_has_decoder(int hevc) {
    return avcodec_find_decoder(hevc ? AV_CODEC_ID_HEVC : AV_CODEC_ID_H264) != NULL;
}

static int matrix_of(enum AVColorSpace space) {
    switch (space) {
    case AVCOL_SPC_BT709: return 1;
    case AVCOL_SPC_BT2020_NCL:
    case AVCOL_SPC_BT2020_CL: return 2;
    case AVCOL_SPC_UNSPECIFIED: return -1;
    default: return 0;
    }
}

// Decode one access unit. Returns 1 with `out` filled (valid until the next
// call), 0 if no picture came out yet, negative on error (text in `error`).
int sunna_ff_decode(sunna_ff *dec, const uint8_t *data, int size, sunna_ff_frame *out, char *error, int error_size) {
    dec->packet->data = (uint8_t *)data;
    dec->packet->size = size;
    int code = avcodec_send_packet(dec->ctx, dec->packet);
    dec->packet->data = NULL;
    dec->packet->size = 0;
    if (code < 0 && code != AVERROR(EAGAIN)) {
        say(error, error_size, "decode", code);
        return code;
    }
    av_frame_unref(dec->frame);
    code = avcodec_receive_frame(dec->ctx, dec->frame);
    if (code == AVERROR(EAGAIN)) return 0;
    if (code < 0) {
        say(error, error_size, "decode", code);
        return code;
    }
    AVFrame *picture = dec->frame;
    if (dec->hw_format != AV_PIX_FMT_NONE && picture->format == dec->hw_format) {
        // Off the GPU, as NV12.
        av_frame_unref(dec->soft);
        code = av_hwframe_transfer_data(dec->soft, picture, 0);
        if (code < 0) {
            say(error, error_size, "copy from the GPU", code);
            return code;
        }
        dec->soft->color_range = picture->color_range;
        dec->soft->colorspace = picture->colorspace;
        picture = dec->soft;
    }
    int layout;
    switch (picture->format) {
    case AV_PIX_FMT_NV12: layout = 0; break;
    case AV_PIX_FMT_YUV420P:
    case AV_PIX_FMT_YUVJ420P: layout = 1; break;
    default:
        say(error, error_size, av_get_pix_fmt_name(picture->format) ? av_get_pix_fmt_name(picture->format) : "an unknown pixel format", 0);
        return -1;
    }
    out->width = picture->width;
    out->height = picture->height;
    out->layout = layout;
    for (int i = 0; i < 3; i++) {
        out->planes[i] = picture->data[i];
        out->strides[i] = picture->linesize[i];
    }
    out->full_range = picture->color_range == AVCOL_RANGE_JPEG || picture->format == AV_PIX_FMT_YUVJ420P;
    out->matrix = matrix_of(picture->colorspace);
    return 1;
}

void sunna_ff_close(sunna_ff *dec) {
    if (!dec) return;
    avcodec_free_context(&dec->ctx);
    av_packet_free(&dec->packet);
    av_frame_free(&dec->frame);
    av_frame_free(&dec->soft);
    free(dec);
}
