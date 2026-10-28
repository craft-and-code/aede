#ifndef AEDE_SPEEX_CONFIG_TYPES_H
#define AEDE_SPEEX_CONFIG_TYPES_H
// Fixed-width public ABI types normally produced by Speex configure. Keep this
// target-independent: Linux GNU and musl must not inherit host-sized typedefs.
#include <stdint.h>
typedef int16_t spx_int16_t;
typedef uint16_t spx_uint16_t;
typedef int32_t spx_int32_t;
typedef uint32_t spx_uint32_t;
#endif
