#ifndef AEDE_MPC_COUNT_MINIMUM_H
#define AEDE_MPC_COUNT_MINIMUM_H
#include <stdint.h>
#include <stdlib.h>

/* Original Aede count helper, not the upstream LGPL generic math header.
 * libmpcdec uses this name only for non-negative byte/enumeration counts.
 * Admission caps file sizes below INT32_MAX and frames well below UINT32_MAX.
 * A uint64_t intermediate preserves either counter without macro re-evaluation. */
static inline uint64_t mini(uint64_t first, uint64_t second) {
    if (first > second) return second;
    return first;
}
static inline int absi(int value) {
    return abs(value);
}
#endif
