/* Original test-only vector, using public Speex APIs, not codec internals. */
#include <speex/speex.h>
#include <stdio.h>
#include <stdint.h>
#include <math.h>
#include <string.h>

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    const SpeexMode *mode = speex_lib_get_mode(0);
    void *enc = speex_encoder_init(mode);
    void *dec = speex_decoder_init(mode);
    if (!enc || !dec) return 3;
    int size = 0, lookahead = 0, quality = 8;
    speex_encoder_ctl(enc, SPEEX_GET_FRAME_SIZE, &size);
    speex_encoder_ctl(enc, SPEEX_GET_LOOKAHEAD, &lookahead);
    speex_encoder_ctl(enc, SPEEX_SET_QUALITY, &quality);
    FILE *f = fopen(argv[1], "wb");
    if (!f || size != 160 || lookahead != 40 || mode->bitstream_version != 4) return 3;
    SpeexBits bits, decoded;
    speex_bits_init(&bits);
    speex_bits_init(&decoded);
    for (int packet = 0; packet < 2; packet++) {
        speex_bits_reset(&bits);
        for (int frame = 0; frame < (packet == 0 ? 2 : 1); frame++) {
            short input[160];
            for (int i = 0; i < size; i++)
                input[i] = packet == 1 && i >= 80 ? 0 : (short)(10000 * sin((packet * 320 + frame * 160 + i) * 0.1));
            if (speex_encode_int(enc, input, &bits) < 0) return 2;
        }
        if (packet == 1) speex_bits_pack(&bits, 15, 5);
        speex_bits_insert_terminator(&bits);
        char data[1024];
        uint32_t bytes = (uint32_t)speex_bits_write(&bits, data, sizeof data);
        unsigned char length[4] = {bytes, bytes >> 8, bytes >> 16, bytes >> 24};
        if (fwrite(length, 1, 4, f) != 4 || fwrite(data, 1, bytes, f) != bytes) return 4;
        speex_bits_read_from(&decoded, data, bytes);
        for (int slot = 0; slot < 2; slot++) {
            float output[160];
            int status = speex_decode(dec, &decoded, output);
            if (status != (packet == 1 && slot == 1 ? -1 : 0) ||
                speex_bits_remaining(&decoded) < 0) return 4;
            printf("packet=%d slot=%d official_status=%d remaining_bits=%d\n",
              packet, slot, status, speex_bits_remaining(&decoded));
        }
    }
    printf("size=%d encoder_lookahead=%d bitstream_version=%d\n", size, lookahead, mode->bitstream_version);
    fclose(f);
    speex_bits_destroy(&bits);
    speex_bits_destroy(&decoded);
    speex_encoder_destroy(enc);
    speex_decoder_destroy(dec);
}
