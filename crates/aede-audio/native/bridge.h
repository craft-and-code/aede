#ifndef AEDE_CODEC_BRIDGE_H
#define AEDE_CODEC_BRIDGE_H
#include <stdint.h>

typedef struct {
    void *user;
    int64_t length;
    int32_t (*read)(void *, void *, int32_t);
    int64_t (*tell)(void *);
    int32_t (*seek)(void *, int64_t);
} AedeReader;

typedef struct {
    uint32_t rate, channels, bits, floating, lossless;
    int64_t frames;
} AedeInfo;
#endif
