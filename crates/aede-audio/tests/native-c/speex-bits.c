/* Upstream issue #42 regression using only public libspeex bit-buffer APIs.
 * Non-owned capacity is 8 bytes; sentinel storage must never be written.
 * Keep out of the product source list. */
#include <speex/speex_bits.h>
#include <string.h>
#include <stdio.h>
int main(void) {
    unsigned char storage[32];
    memset(storage, 0xa5, sizeof(storage));
    SpeexBits bits;
    speex_bits_init_buffer(&bits, storage, 8);
    speex_bits_read_from(&bits, "ab", 2);
    speex_bits_read_whole_bytes(&bits, "0123456789abcdef", 16);
    int okay = bits.nbBits <= 64;
    for (int i = 8; i < 32; ++i) okay &= storage[i] == 0xa5;
    speex_bits_destroy(&bits);
    if (!okay) { fputs("non-owned buffer overwritten\n", stderr); return 1; }
    return 0;
}
