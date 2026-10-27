/* A corrupt admitted block must reach CRC failure with defined arithmetic. */
#define _POSIX_C_SOURCE 200809L
#include "bridge.h"
#include <stdio.h>
#include <stdint.h>

void *aede_wv_open(AedeReader *, AedeInfo *);
int aede_wv_decode(void *, int32_t *, uint32_t);
void aede_wv_close(void *);

static int32_t read_bytes(void *file, void *out, int32_t count) {
    if (count < 0 || count > 16777216) return 0;
    return (int32_t)fread(out, 1, (size_t)count, file);
}
static int64_t position(void *file) { return ftello(file); }
static int32_t seek_to(void *file, int64_t offset) {
    return offset >= 0 && fseeko(file, offset, SEEK_SET) == 0;
}
static int check_corruption(FILE *file) {
    if (fseeko(file, 0, SEEK_END) != 0) return 1;
    int64_t length = ftello(file);
    if (length < 0 || fseeko(file, 0, SEEK_SET) != 0) return 1;
    AedeReader reader = {file, length, read_bytes, position, seek_to};
    AedeInfo info = {0};
    void *codec = aede_wv_open(&reader, &info);
    if (!codec) return 1;
    int failed = 1;
    if (info.channels == 2 && info.rate == 44100 && info.bits == 16) {
        int32_t output[4096 * 2];
        for (unsigned i = 0; i < 10000; ++i) {
            int frames = aede_wv_decode(codec, output, 4096);
            if (frames < 0) { failed = 0; break; }
            if (frames == 0) break;
        }
    }
    aede_wv_close(codec);
    return failed;
}
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    FILE *file = fopen(argv[1], "rb");
    if (!file) return 3;
    int failed = check_corruption(file);
    fclose(file);
    return failed;
}
