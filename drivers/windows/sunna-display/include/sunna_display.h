#ifndef SUNNA_DISPLAY_H
#define SUNNA_DISPLAY_H

#include <stdint.h>

/* All wire fields are little-endian uint32_t; no pointers or implicit padding. */
#define SUNNA_DISPLAY_INTERFACE_GUID_STRING "{27e046a1-9ba8-4479-963f-cd0f642788c1}"
#define SUNNA_DISPLAY_INTERFACE_GUID_INIT \
    {0x27e046a1, 0x9ba8, 0x4479, {0x96, 0x3f, 0xcd, 0x0f, 0x64, 0x27, 0x88, 0xc1}}
#define SUNNA_DISPLAY_VERSION 1u
#define SUNNA_DISPLAY_MAX_MONITORS 4u

/* CTL_CODE(FILE_DEVICE_UNKNOWN, function, METHOD_BUFFERED, FILE_WRITE_ACCESS). */
#define SUNNA_DISPLAY_IOCTL(function) ((0x22u << 16) | (2u << 14) | ((function) << 2))
#define IOCTL_SUNNA_DISPLAY_INFO   SUNNA_DISPLAY_IOCTL(0x800u)
#define IOCTL_SUNNA_DISPLAY_ADD    SUNNA_DISPLAY_IOCTL(0x801u)
#define IOCTL_SUNNA_DISPLAY_REMOVE SUNNA_DISPLAY_IOCTL(0x802u)

typedef struct SUNNA_DISPLAY_INFO {
    uint32_t version;
    uint32_t max_monitors;
} SUNNA_DISPLAY_INFO;

typedef struct SUNNA_DISPLAY_ADD {
    uint32_t version;
    uint32_t width;
    uint32_t height;
    uint32_t refresh_hz;
} SUNNA_DISPLAY_ADD;

typedef struct SUNNA_DISPLAY_ID {
    uint32_t id;
} SUNNA_DISPLAY_ID;

/* INFO: no input, INFO output. ADD: ADD input, ID output. REMOVE: ID input,
   no output. Open the interface with GENERIC_WRITE. IDs are opaque; retain
   the ADD result. DEVICE_NOT_READY is retryable; never retry a successful ADD. */
#endif
