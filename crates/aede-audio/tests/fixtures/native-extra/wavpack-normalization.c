/* Original test-only vector, using public WavPack APIs, not codec internals. */
#include <wavpack.h>
#include <stdio.h>
#include <string.h>
#include <stdint.h>

static int write_block(void *id, void *data, int32_t bytes) {
    return fwrite(data, 1, (size_t)bytes, id) == (size_t)bytes;
}

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    FILE *file = fopen(argv[1], "wb");
    if (!file) return 3;
    WavpackContext *enc = WavpackOpenFileOutput(write_block, file, NULL);
    WavpackConfig config = {0};
    config.bits_per_sample = 32;
    config.bytes_per_sample = 4;
    config.num_channels = 1;
    config.channel_mask = 4;
    config.sample_rate = 44100;
    config.float_norm_exp = 142;
    float source[4] = {16384.f, -16384.f, 8192.f, -8192.f};
    int32_t encoded[4];
    memcpy(encoded, source, sizeof source);
    if (!enc || !WavpackSetConfiguration(enc, &config, 4) ||
        !WavpackPackInit(enc) || !WavpackPackSamples(enc, encoded, 4) ||
        !WavpackFlushSamples(enc)) {
        fprintf(stderr, "encode error: %s\n", enc ? WavpackGetErrorMessage(enc) : "open");
        return 4;
    }
    WavpackCloseFile(enc);
    fclose(file);
    for (int i = 0; i < 2; ++i) {
        char error[80] = {0};
        WavpackContext *dec = WavpackOpenFileInput(argv[1], error, i ? OPEN_NORMALIZE : 0, 0);
        if (!dec) { fprintf(stderr, "decode open: %s\n", error); return 5; }
        int32_t decoded[4];
        float pcm[4];
        unsigned count = WavpackUnpackSamples(dec, decoded, 4);
        memcpy(pcm, decoded, sizeof pcm);
        printf("flags=%s count=%u float_norm_exp=%d mode=%x errors=%d PCM=[%.9g,%.9g,%.9g,%.9g]\n",
          i ? "OPEN_NORMALIZE" : "0", count, WavpackGetFloatNormExp(dec),
          WavpackGetMode(dec), WavpackGetNumErrors(dec), pcm[0], pcm[1], pcm[2], pcm[3]);
        if (count != 4 || WavpackGetNumErrors(dec) != 0 ||
            WavpackGetFloatNormExp(dec) != (i ? 127 : 142)) return 6;
        for (int sample = 0; sample < 4; sample++)
            if (pcm[sample] != source[sample] / (i ? 32768.f : 1.f)) return 7;
        WavpackCloseFile(dec);
    }
    return 0;
}
