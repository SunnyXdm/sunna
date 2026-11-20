#pragma once
#include <stdint.h>
#include <string.h>
#include "include/sunna_display.h"

namespace sunna {
struct Mode { uint32_t width, height, hz; };
struct Modes {
    Mode items[25];
    uint32_t count;
    void Add(Mode m) noexcept {
        for (uint32_t i = 0; i < count; ++i)
            if (items[i].width == m.width && items[i].height == m.height && items[i].hz == m.hz)
                return;
        if (count < 25) items[count++] = m;
    }
};
inline bool Valid(Mode m) noexcept {
    return m.width >= 640 && m.width <= 7680 && m.height >= 480 && m.height <= 4320 &&
        !(m.width & 1) && !(m.height & 1) && m.hz >= 24 && m.hz <= 500;
}
inline Modes MakeModes(Mode requested) noexcept {
    Modes result{};
    result.Add(requested);
    constexpr uint32_t sizes[][2] = {
        {3840,2160}, {3440,1440}, {2560,1600}, {2560,1440},
        {1920,1200}, {1920,1080}, {1680,1050}, {1600,900},
        {1440,900}, {1366,768}, {1280,800}, {1280,720}
    };
    for (const auto& size : sizes) {
        result.Add({size[0], size[1], requested.hz});
        result.Add({size[0], size[1], 60});
    }
    return result;
}
// Small, fixed blanking intervals suit a virtual link; no physical link bandwidth limit.
constexpr uint32_t HBlank = 160;
constexpr uint32_t VBlank = 45;
inline uint64_t PixelRate(Mode m) noexcept {
    return uint64_t(m.width + HBlank) * (m.height + VBlank) * m.hz;
}
inline void MakeEdid(uint8_t (&edid)[128], uint32_t id, Mode m) noexcept {
    memset(edid, 0, sizeof(edid));
    for (unsigned i = 1; i < 7; ++i) edid[i] = 0xff;
    // EISA manufacturer SNA (19,14,1), little-endian product and serial.
    edid[8] = 0x4d; edid[9] = 0xc1;
    edid[10] = 1;
    for (unsigned i = 0; i < 4; ++i) edid[12+i] = uint8_t(id >> (8*i));
    edid[16] = 1; edid[17] = 36;
    edid[18] = 1; edid[19] = 4;
    edid[20] = 0xa5; // Digital, 8-bit DisplayPort-like endpoint.
    edid[21] = 60; edid[22] = 34; edid[23] = 120;
    edid[24] = 0x04; // sRGB; preferred timing flag set only when a DTD is present.
    // sRGB chromaticities, encoded in EDID's 10-bit coordinates.
    constexpr uint16_t xy[] = {655,338,307,614,154,61,320,337};
    edid[25] = uint8_t(((xy[0]&3)<<6)|((xy[1]&3)<<4)|((xy[2]&3)<<2)|(xy[3]&3));
    edid[26] = uint8_t(((xy[4]&3)<<6)|((xy[5]&3)<<4)|((xy[6]&3)<<2)|(xy[7]&3));
    for (unsigned i = 0; i < 8; ++i) edid[27+i] = uint8_t(xy[i] >> 2);
    for (unsigned i = 38; i < 54; ++i) edid[i] = 1;
    for (unsigned i = 54; i < 126; i += 18) edid[i+3] = 0x10;

    const uint64_t clock = (PixelRate(m) + 5000) / 10000;
    // DTD also has 12-bit active dimensions. IddCx modes remain authoritative.
    if (PixelRate(m) <= 655350000 && m.width <= 4095 && m.height <= 4095) {
        uint8_t* d = edid + 54;
        memset(d, 0, 18);
        d[0] = uint8_t(clock); d[1] = uint8_t(clock >> 8);
        d[2] = uint8_t(m.width); d[3] = uint8_t(HBlank);
        d[4] = uint8_t((m.width >> 8) << 4);
        d[5] = uint8_t(m.height); d[6] = uint8_t(VBlank);
        d[7] = uint8_t((m.height >> 8) << 4);
        d[8] = 48; d[9] = 32; d[10] = 0x35;
        d[12] = 88; d[13] = 84; d[14] = 0x21; // 600 x 340 mm.
        d[17] = 0x1a;
        edid[24] |= 2;
    }
    uint8_t* name = edid + 72;
    name[3] = 0xfc;
    memcpy(name+5, "Sunna Display", 13);
    uint8_t* range = edid + 90;
    range[3] = 0xfd;
    // EDID 1.4 supports +255 range offsets; clamp horizontal limits that cannot fit.
    const uint32_t maxV = m.hz > 60 ? m.hz : 60;
    const uint32_t maxHeight = m.height > 2160 ? m.height : 2160;
    uint32_t maxH = ((maxHeight + VBlank) * maxV + 999) / 1000;
    if (maxH > 510) maxH = 510;
    range[4] = uint8_t((maxV > 255 ? 2 : 0) | (maxH > 255 ? 8 : 0));
    range[5] = uint8_t(m.hz < 60 ? m.hz : 60);
    range[6] = uint8_t(maxV > 255 ? maxV - 255 : maxV);
    range[7] = 12;
    range[8] = uint8_t(maxH > 255 ? maxH - 255 : maxH);
    uint64_t maxClock = PixelRate(m);
    const uint64_t commonClock = PixelRate({3840,2160,maxV});
    if (commonClock > maxClock) maxClock = commonClock;
    const uint64_t clock10MHz = (maxClock + 9999999) / 10000000;
    range[9] = uint8_t(clock10MHz > 255 ? 255 : clock10MHz);
    range[10] = 1; range[11] = 0x0a;
    memset(range+12, 0x20, 6);
    uint8_t sum = 0;
    for (unsigned i = 0; i < 127; ++i) sum = uint8_t(sum + edid[i]);
    edid[127] = uint8_t(0u - sum);
}
}
