// Link-only fixture: retain all three C ABI entry points and SDK dependencies.
// Cross-compiled artifacts are never described as executed platform tests.
#include <stdint.h>
struct AedeReader;
struct AedeApeInfo {
    uint32_t rate, channels, bits, version, blocks, last, count;
    int64_t frames;
};
extern "C" void *aede_ape_open(AedeReader *, AedeApeInfo *) noexcept;
extern "C" int32_t aede_ape_frame(void *, uint64_t, uint32_t, uint8_t *, uint32_t) noexcept;
extern "C" void aede_ape_close(void *) noexcept;
int main() {
    AedeApeInfo info = {};
    void *handle = aede_ape_open(nullptr, &info);
    uint8_t pcm[4] = {};
    int32_t result = aede_ape_frame(handle, 0, 1, pcm, sizeof(pcm));
    aede_ape_close(handle);
    return result == -1 ? 0 : 1;
}
