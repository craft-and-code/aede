/* Exercise the retained demux arithmetic with independent byte/seek values. */
#include "mpc_demux.c"

static mpc_int32_t read_bytes(mpc_reader *reader, void *out, mpc_int32_t count) {
    (void)reader;
    memset(out, 0, count);
    return count;
}
static mpc_bool_t seek_bytes(mpc_reader *reader, mpc_int32_t position) {
    (void)reader;
    return position >= 0;
}
static mpc_int32_t tell_bytes(mpc_reader *reader) {
    (void)reader;
    return 0;
}
static void bits(unsigned char *out, unsigned *at, mpc_uint32_t value, unsigned count) {
    while (count--) {
        out[*at / 8] |= ((value >> count) & 1u) << (7u - *at % 8);
        ++*at;
    }
}
static int table(mpc_demux *demux, mpc_uint32_t residual) {
    const unsigned quotient = residual >> 12;
    const size_t size = (28u + quotient + 13u + 7u) / 8u + 8u;
    unsigned char *allocation = calloc(size + 4u, 1);
    if (!allocation) return 1;
    /* The word reader also inspects the preceding bytes of its demux buffer. */
    unsigned char *data = allocation + 4u;
    unsigned at = 0;
    bits(data, &at, 3, 8);  /* Three seek entries. */
    bits(data, &at, 0, 4);  /* No extra seek power. */
    bits(data, &at, 10, 8); /* First two byte offsets relative to header. */
    bits(data, &at, 20, 8);
    at += quotient;       /* Unary quotient, followed by a one and 12 bits. */
    bits(data, &at, 1, 1);
    bits(data, &at, residual & 4095u, 12);
    demux->bits_reader = (mpc_bits_reader){data, 8};
    demux->si.samples = 100000;
    demux->si.header_position = 4;
    const mpc_int64_t delta = residual & 1u ? -(mpc_int64_t)(residual & ~1u) : residual;
    const mpc_int64_t expected = 2 * 192 - 112 + delta * 4;
    int failed = mpc_demux_ST(demux) != MPC_STATUS_OK ||
        demux->seek_table_size != 3 || demux->seek_table[0] != 112 ||
        demux->seek_table[1] != 192 || demux->seek_table[2] != expected;
    free(demux->seek_table);
    demux->seek_table = NULL;
    free(allocation);
    return failed;
}
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    mpc_demux *demux = calloc(1, sizeof(*demux));
    if (!demux) return 3;
    mpc_reader reader = {read_bytes, seek_bytes, tell_bytes, NULL, NULL, NULL};
    demux->r = &reader;
    demux->bits_reader = (mpc_bits_reader){demux->buffer, 8};
    int failed;
    if (!strcmp(argv[1], "fill")) {
        failed = mpc_demux_fill(demux, 4, MPC_BUFFER_SWAP) != 4;
    } else if (!strcmp(argv[1], "seek")) {
        demux->si.stream_version = 7;
        failed = mpc_demux_seek(demux, 40, 4) != MPC_STATUS_OK ||
            demux->bits_reader.buff != demux->buffer + 1;
    } else {
        failed = table(demux, !strcmp(argv[1], "negative") ? 3u :
            !strcmp(argv[1], "positive") ? 4u : 0x7ffffffeu);
    }
    free(demux);
    return failed;
}
