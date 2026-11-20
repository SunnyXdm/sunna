#include "../modes.h"
#include <stdio.h>
#include <stdlib.h>

#define CHECK(condition) do { if (!(condition)) { \
    fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #condition); return 1; } } while (0)

int main() {
    using namespace sunna;
    static_assert(sizeof(SUNNA_DISPLAY_INFO) == 8 && sizeof(SUNNA_DISPLAY_ADD) == 16 && sizeof(SUNNA_DISPLAY_ID) == 4);
    static_assert(IOCTL_SUNNA_DISPLAY_INFO == 0x22a000 && IOCTL_SUNNA_DISPLAY_ADD == 0x22a004 && IOCTL_SUNNA_DISPLAY_REMOVE == 0x22a008);
    CHECK(Valid({640,480,24}));
    CHECK(Valid({7680,4320,500}));
    CHECK(!Valid({638,480,60})); CHECK(!Valid({640,478,60}));
    CHECK(!Valid({7682,4320,60})); CHECK(!Valid({7680,4322,60}));
    CHECK(!Valid({641,480,60})); CHECK(!Valid({640,481,60}));
    CHECK(!Valid({1920,1080,23})); CHECK(!Valid({1920,1080,501}));
    CHECK(!Valid({UINT32_MAX,UINT32_MAX,UINT32_MAX}));
    const uint32_t widths[] = {640,1280,1366,1920,2560,3440,3840,4094,4096,7680};
    const uint32_t heights[] = {480,720,768,1080,1440,2160,4094,4096,4320};
    const uint32_t rates[] = {24,30,59,60,120,144,165,240,255,256,360,500};
    uint32_t cases = 0;
    for (auto w : widths) for (auto h : heights) for (auto hz : rates) {
        Mode requested{w,h,hz};
        auto modes = MakeModes(requested);
        CHECK(modes.count >= 12 && modes.count <= 25);
        CHECK(modes.items[0].width == w && modes.items[0].height == h && modes.items[0].hz == hz);
        for (uint32_t i = 0; i < modes.count; ++i) {
            CHECK(Valid(modes.items[i]));
            CHECK(modes.items[i].hz == hz || modes.items[i].hz == 60);
            for (uint32_t j = i+1; j < modes.count; ++j)
                CHECK(modes.items[i].width != modes.items[j].width || modes.items[i].height != modes.items[j].height || modes.items[i].hz != modes.items[j].hz);
        }
        const uint32_t id = 0x12340001 + cases;
        uint8_t edid[128];
        MakeEdid(edid, id, requested);
        uint32_t sum = 0;
        for (auto b : edid) sum += b;
        CHECK((sum & 255) == 0);
        CHECK(edid[0] == 0 && edid[7] == 0 && edid[126] == 0);
        for (unsigned i = 1; i < 7; ++i) CHECK(edid[i] == 255);
        CHECK(edid[8] == 0x4d && edid[9] == 0xc1);
        CHECK((uint32_t(edid[12]) | (uint32_t(edid[13])<<8) | (uint32_t(edid[14])<<16) | (uint32_t(edid[15])<<24)) == id);
        CHECK(memcmp(edid+77, "Sunna Display", 13) == 0);
        CHECK(edid[93] == 0xfd);
        const uint32_t maxV = edid[96] + ((edid[94] & 2) ? 255 : 0);
        CHECK(maxV >= hz && maxV >= 60);
        const bool hasDtd = edid[54] || edid[55];
        CHECK(hasDtd == (PixelRate(requested) <= 655350000 && w <= 4095 && h <= 4095));
        CHECK(bool(edid[24] & 2) == hasDtd);
        if (hasDtd) {
            CHECK((uint32_t(edid[56]) | (uint32_t(edid[58] & 0xf0)<<4)) == w);
            CHECK((uint32_t(edid[59]) | (uint32_t(edid[61] & 0xf0)<<4)) == h);
            const uint64_t clock = (uint32_t(edid[54]) | (uint32_t(edid[55])<<8))*10000ull;
            const uint64_t expected = PixelRate(requested);
            CHECK(clock + 5000 >= expected && clock <= expected + 5000);
        }
        ++cases;
    }
    CHECK(PixelRate({7680,4320,500}) > UINT32_MAX);
    printf("Mode/EDID/ABI checks: PASS (%u mode combinations)\n", cases);
    return 0;
}
