# Complete FLAC playback fixtures

These playback fixtures preserve the encoded audio and metadata of the matching
`audit-*.flac` [general fixtures](../../fixtures). They differ only where necessary in STREAMINFO's total
frame count and decoded-audio MD5, so that progressive playback can verify the
complete audio actually present. The audit fixtures remain unchanged.

The integer PCM was decoded with reference `flac 1.5.0` using signed,
little-endian raw output and `--decode-through-errors`. Each digest was computed
independently with Python's `hashlib.md5`. The resulting playback files all
passed `flac 1.5.0 --test` on 2026-10-04. Bytes before STREAMINFO's packed format
field and after its MD5 were compared with their audit sources and are identical;
there was no re-encoding or change to audio samples.

| File | Frames | Rate | Channels | Bits | Decoded PCM MD5 |
| --- | --- | --- | --- | --- | --- |
| `playback-stereo.flac` | 18432 | 44100 Hz | 2 | 16 | `7e2d2157e4982d903552e068d16a097d` |
| `playback-real24.flac` | 18432 | 48000 Hz | 2 | 24 | `094731fdef7e6d68c1ee38926308ddba` |
| `playback-padded24.flac` | 18432 | 48000 Hz | 2 | 24 | `50da7e50ca791bc6be7e1ca80ef96570` |
| `playback-dualmono.flac` | 18432 | 44100 Hz | 2 | 16 | `80055b149f8a74aef381a1164895c5e2` |

The existing [`audit-silence.flac`](../../fixtures/audit-silence.flac) already has
the correct complete frame count and MD5 and passed reference `--test` unchanged;
the silence playback regression therefore reuses it without a duplicate fixture.

`playback-ogg-flac.oga` was generated with reference `flac 1.5.0 --ogg` from
[`track.flac`](../../fixtures/track.flac), without padding or seek tables. It also passed reference `--test`
and exercises the Ogg FLAC mapping separately from native FLAC framing.

## Six-channel reference

`playback-5_1.flac` contains the complete 120 frames of the existing
[`surround-5_1.wav`](../../channel_fixtures/surround-5_1.wav), at 48,000 Hz with
six signed 16-bit channels ordered FL, FR, FC, LFE, BL, BR (mask `0x3f`). It was
generated once with FFmpeg 9.0.2 on 2026-10-22, from the repository root:

```sh
ffmpeg -hide_banner -loglevel error -nostdin \
  -i crates/aede-core/tests/channel_fixtures/surround-5_1.wav \
  -map 0:a:0 -map_metadata -1 -c:a flac -compression_level 8 \
  -metadata_header_padding 0 -f flac \
  crates/aede-core/tests/playback_fixtures/flac/playback-5_1.flac
```

Independent FFmpeg signed `s16le` decodes of the WAV and FLAC matched all 1,440
PCM bytes, with MD5 `c0fd1ad6121fdfbbe1f3cc99e372283e`; both encoded sources
remained unchanged. The retained decoder tests use the checked-in FLAC and
existing WAV directly and require no encoder at runtime.

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `surround-5_1.wav` | 1508 | `abc9ee2421a0940e07d7e5661157595c11c554aa65402148c7d09a16b4185d5c` |
| `playback-5_1.flac` | 311 | `1b64ea2142e49bb567d74cc62729cf9571075fb4eed0d55b2e741b6cf33764e3` |
