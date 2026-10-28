#include "bridge.h"
#include <speex/speex.h>
#include <speex/speex_stereo.h>
#include <speex/speex_callbacks.h>
#include <stdlib.h>

typedef struct {
    void *state;
    SpeexBits bits;
    SpeexStereoState *stereo;
    int size, channels;
} Context;

void *aede_spx_open(int mode, int channels, int *size, int *lookahead, int *version) {
    const SpeexMode *m = speex_lib_get_mode(mode);
    if (!m) return NULL;
    Context *c = calloc(1, sizeof(*c));
    if (!c) return NULL;
    c->state = speex_decoder_init(m);
    if (!c->state) { free(c); return NULL; }
    c->channels = channels;
    speex_bits_init(&c->bits);
    /* Match libspeex's standard codec reconstruction, not application DSP. */
    int enhancement = 1;
    speex_decoder_ctl(c->state, SPEEX_SET_ENH, &enhancement);
    speex_decoder_ctl(c->state, SPEEX_GET_FRAME_SIZE, &c->size);
    speex_decoder_ctl(c->state, SPEEX_GET_LOOKAHEAD, lookahead);
    *size = c->size;
    *version = m->bitstream_version;
    if (channels == 2) {
        c->stereo = speex_stereo_state_init();
        if (!c->stereo) {
            speex_decoder_destroy(c->state);
            speex_bits_destroy(&c->bits);
            free(c);
            return NULL;
        }
        SpeexCallback cb = {0};
        cb.callback_id = SPEEX_INBAND_STEREO;
        cb.func = speex_std_stereo_request_handler;
        cb.data = c->stereo;
        speex_decoder_ctl(c->state, SPEEX_SET_HANDLER, &cb);
    }
    return c;
}

int aede_spx_decode(void *p, const unsigned char *packet, int bytes,
                    int frames, int terminal, float *out, int capacity) {
    Context *c = p;
    if (frames < 1 || frames > 16 || c->size * frames * c->channels > capacity)
        return -1;
    speex_bits_read_from(&c->bits, (char *)packet, bytes);
    for (int i = 0; i < frames; i++) {
        /* The official encoder fills unused final packet slots with the
           five-bit 0b01111 terminator. A decoder return of -1 alone is not
           enough: it also means fewer than five bits remain after truncation.
           Only an explicit marker after complete speech frames at EOS may
           shorten the nominal packet; every decode error stays fatal. */
        if (terminal && i > 0 && speex_bits_remaining(&c->bits) >= 5 &&
            speex_bits_peek_unsigned(&c->bits, 5) == 15)
            return c->size * i;
        float *block = out + i * c->size * c->channels;
        if (speex_decode(c->state, &c->bits, block) != 0 ||
            speex_bits_remaining(&c->bits) < 0)
            return -1;
        if (c->channels == 2)
            speex_decode_stereo(block, c->size, c->stereo);
    }
    return c->size * frames;
}

void aede_spx_close(void *p) {
    Context *c = p;
    speex_decoder_destroy(c->state);
    speex_bits_destroy(&c->bits);
    if (c->stereo) speex_stereo_state_destroy(c->stereo);
    free(c);
}
