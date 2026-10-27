/* Isolated reference-reader arithmetic regression, never part of product code. */
#include <stdint.h>
#include <mpc/mpcdec.h>
#include "internal.h"
#include "huffman.h"
#include "mpc_bits_reader.h"
int main(void) {
    unsigned char bytes[8] = {0xda,0xbb,0xcc,0xee,0xd8,0x91,0,0};
    const uint64_t word = UINT64_C(0xdabbcceed891);
    for (unsigned start = 0; start < 8; ++start) {
        mpc_bits_reader reader = {bytes,8};
        if (start) mpc_bits_read(&reader,start);
        uint32_t expected = (uint32_t)(word >> (48-start-31)) & UINT32_C(0x7fffffff);
        if (mpc_bits_read(&reader,31) != expected) return 1;
    }
    return 0;
}
