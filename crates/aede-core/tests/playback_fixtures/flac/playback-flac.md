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
