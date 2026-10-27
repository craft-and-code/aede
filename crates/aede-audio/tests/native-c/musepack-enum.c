/* Independent one-of-32 combinatorial masks, including the sign-bit position. */
#include <stdint.h>
#include <mpc/mpcdec.h>
#include "internal.h"
#include "huffman.h"
#include "mpc_bits_reader.h"
int main(void) {
    for (unsigned position = 0; position < 32; ++position) {
        /* All 32 single-bit masks have a five-bit rank, read MSB first. */
        unsigned char bytes[8] = {(unsigned char)(position << 3),0,0,0,0,0,0,0};
        mpc_bits_reader reader = {bytes,8};
        if (mpc_bits_enum_dec(&reader,1,32) != (UINT32_C(1) << position)) return 1;
    }
    return 0;
}
