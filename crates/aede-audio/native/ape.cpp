// Original read-only adapter over the approved SDK public interfaces. No path
// reaches the SDK. Its CAPEInfo borrows IAPEIO; this owner destroys the decoder
// before the I/O object, while Rust retains its stable callback boxes.
#include "bridge.h"
#include "All.h"
#include "MACLib.h"
#include "IAPEIO.h"
#include <memory>
#include <limits>

struct AedeApeInfo {
    uint32_t rate, channels, bits, version, blocks, last, count;
    int64_t frames;
};
class AedeApeIO final : public APE::IAPEIO {
    AedeReader *reader;
public:
    explicit AedeApeIO(AedeReader *value) : reader(value) {}
    int Open(const APE::str_utfn *, bool) override { return APE_ERROR_IO_READ; }
    int Close() override { return APE_ERROR_SUCCESS; }
    int Read(void *out, APE::int64 count, APE::int64 *read) override {
        if (read) *read = 0;
        if (count < 0 || count > 16 * 1024 * 1024 || (count && !out))
            return APE_ERROR_IO_READ;
        int32_t got = reader->read(reader->user, out, static_cast<int32_t>(count));
        if (got < 0 || got > count) return APE_ERROR_IO_READ;
        if (read) *read = got;
        return APE_ERROR_SUCCESS;
    }
    int Write(const void *, APE::int64, APE::int64 *) override { return APE_ERROR_IO_WRITE; }
    int Seek(APE::int64 offset, APE::SeekMethod method) override {
        int64_t base = method == APE::SeekFileBegin ? 0 :
            method == APE::SeekFileCurrent ? reader->tell(reader->user) :
            method == APE::SeekFileEnd ? reader->length : -1;
        if (base < 0 || base > reader->length ||
            (offset < 0 && offset < -base) ||
            (offset >= 0 && offset > reader->length - base)) return APE_ERROR_IO_READ;
        return reader->seek(reader->user, base + offset) ? APE_ERROR_SUCCESS : APE_ERROR_IO_READ;
    }
    int Create(const APE::str_utfn *) override { return APE_ERROR_IO_WRITE; }
    int Delete() override { return APE_ERROR_IO_WRITE; }
    int SetEOF() override { return APE_ERROR_IO_WRITE; }
    unsigned char *GetBuffer(int *bytes) override { if (bytes) *bytes = 0; return nullptr; }
    APE::int64 GetPosition() override { return reader->tell(reader->user); }
    APE::int64 GetSize() override { return reader->length; }
};
struct AedeApe {
    std::unique_ptr<AedeApeIO> io;
    std::unique_ptr<APE::IAPEDecompress> decoder;
};
extern "C" void *aede_ape_open(AedeReader *reader, AedeApeInfo *info) noexcept {
    try {
        if (!reader || !info || reader->length < 32) return nullptr;
        std::unique_ptr<AedeApe> owner(new AedeApe);
        owner->io.reset(new AedeApeIO(reader));
        int error = APE_ERROR_SUCCESS;
        owner->decoder.reset(CreateIAPEDecompressEx(owner->io.get(), &error));
        if (!owner->decoder || error != APE_ERROR_SUCCESS ||
            owner->decoder->SetNumberOfThreads(1) != 1) return nullptr;
        auto &d = *owner->decoder;
        using I = APE::IAPEInfo;
        if (d.GetInfo(I::APE_INFO_APL) != 0) return nullptr;
        info->rate = static_cast<uint32_t>(d.GetInfo(I::APE_INFO_SAMPLE_RATE));
        info->channels = static_cast<uint32_t>(d.GetInfo(I::APE_INFO_CHANNELS));
        info->bits = static_cast<uint32_t>(d.GetInfo(I::APE_INFO_BITS_PER_SAMPLE));
        info->version = static_cast<uint32_t>(d.GetInfo(I::APE_INFO_FILE_VERSION));
        info->blocks = static_cast<uint32_t>(d.GetInfo(I::APE_INFO_BLOCKS_PER_FRAME));
        info->last = static_cast<uint32_t>(d.GetInfo(I::APE_INFO_FINAL_FRAME_BLOCKS));
        info->count = static_cast<uint32_t>(d.GetInfo(I::APE_INFO_TOTAL_FRAMES));
        info->frames = d.GetInfo(I::APE_INFO_TOTAL_BLOCKS);
        if (info->channels < 1 || info->channels > 2 ||
            (info->bits != 8 && info->bits != 16 && info->bits != 24 && info->bits != 32) ||
            d.GetInfo(I::APE_INFO_BLOCK_ALIGN) != info->channels * (info->bits / 8)) return nullptr;
        return owner.release();
    } catch (...) { return nullptr; }
}
extern "C" int32_t aede_ape_frame(void *handle, uint64_t start, uint32_t frames,
                                  uint8_t *out, uint32_t capacity) noexcept {
    try {
        if (!handle || !out || frames == 0 || frames > 1048576 || start > uint64_t(INT64_MAX) - frames)
            return -1;
        auto &d = *static_cast<AedeApe *>(handle)->decoder;
        int64_t align = d.GetInfo(APE::IAPEInfo::APE_INFO_BLOCK_ALIGN);
        if (align < 1 || align > 8 || uint64_t(frames) * align != capacity) return -1;
        int status = d.Seek(static_cast<int64_t>(start));
        if (status != APE_ERROR_SUCCESS ||
            d.GetInfo(APE::IAPEInfo::APE_DECOMPRESS_CURRENT_BLOCK) != static_cast<int64_t>(start)) return -1;
        APE::int64 got = 0;
        // A complete codec frame is staged before publication, so a CRC error
        // cannot expose an otherwise playable prefix of that damaged frame.
        status = d.GetData(out, frames, &got);
        if (status != APE_ERROR_SUCCESS || got != frames ||
            d.GetInfo(APE::IAPEInfo::APE_DECOMPRESS_CURRENT_BLOCK) != static_cast<int64_t>(start + frames)) return -1;
        return static_cast<int32_t>(capacity);
    } catch (...) { return -1; }
}
extern "C" void aede_ape_close(void *handle) noexcept {
    try { delete static_cast<AedeApe *>(handle); } catch (...) { }
}
