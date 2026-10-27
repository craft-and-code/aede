# Original additional-codec fixtures

All signals are generated test audio, not third-party recordings. The original bytes/tags are never modified by production decoding.

- `track.wv`, `float32.wv`, `hybrid-lossy.wv`, `track.spx` are copied unchanged from Aède's existing generated metadata fixtures. They retain integer/float/hybrid and standard Speex coverage.
- `ramp.ape` is the existing Phémios CUE fixture, copied unchanged. Monkey's Audio 3.99 encoded original stereo PCM16 at 44.1 kHz; 132,300 frames (225 CD sectors), left sample `frame % 30001 - 15000`, right its negative. The test checks every original sample, independently of a reference decoder. Legacy/prefixed or other APE precisions are not qualified by this fixture.
- `tone-sv8.mpc` is produced by the official Musepack r475 encoder (mpcenc 1.30.1), from 44,113 stereo PCM16 frames at 44.1 kHz: left 997 Hz, right 2,711 Hz, amplitude 10,000. The reference decoder returns exactly that playable count. FFmpeg's independent PCM reference retains 481 synthesis frames; fixture generation explicitly crops that prefix and keeps the next 44,113. This is not a production trim or a blanket SV7 claim.
- `speex-{nb,wb,uwb}.spx`: original PCM16 sine signals at 8/16/32 kHz, lengths 8,013/16,013/32,013, channels 1/2/2 and 1/2/1 codec frames per packet. `speex-encode.c` uses only public libspeex encoder APIs, quality 8; `generate.py` supplies checked RFC 3533 Ogg pages. Last packets are padded with zero input. Native coded PCM lengths are 8,160/16,640/32,640. Header granules do not claim an exact original gapless window.

`*.f32` are little-endian normalized interleaved reference samples. WavPack and APE come from independent FFmpeg decoding; Musepack uses the above explicit reference alignment. Speex references come from the official libspeex 1.2.1 `speexdec` default (`--enh`), converted from its PCM16 output to float. The independent FFmpeg Speex reconstruction differs in wideband modes and is deliberately not treated as an equivalent PCM reference. Native float vs official quantized Speex permits at most three PCM16 LSBs; integer lossless sources and Musepack have their separately bounded comparisons in the tests.

Recreate the newly encoded SV8/Speex inputs and all PCM references:

```sh
python3 generate.py /path/to/mpcenc /path/to/fixture-speex-encode /path/to/speexdec
```

Build the test-only `speex-encode.c` against libspeex 1.2.1 public headers and library. Official `mpcenc`/`speexdec` are reference tools used only during fixture generation; they are not product dependencies and tests run from checked-in bytes without them. Archive provenance/hashes are in `../../../vendor/README.md`. `sha256.json` records the fixture bytes; regeneration must be reviewed rather than accepted merely because the current decoder agrees.

## Historical APE vectors

`silence-3950.ape` and `silence-3970.ape` are original 882,000-frame stereo PCM16/44.1-kHz silence, with three historical codec frames (294,912 + 294,912 + 292,176). `generate-legacy-ape.py` uses the approved SDK 13.43 encoder's special-silence packet, reconstructs the historical 32-byte header/seek table and independently computes each PCM CRC. Both SDK decoding and FFmpeg with errors made fatal must reproduce exactly 3,528,000 zero bytes before generation succeeds. This qualifies the old layout, CRC, complete-frame staging and exact seek boundaries; silence does not qualify historical prediction filters. No historical encoder is falsely attributed to these generated containers. Existing nonzero 3.99 references remain unchanged.

Separate local acceptance used the real `sh3.ape` (version 3.97) from <https://samples.ffmpeg.org/monkeyaudio/sh3.ape>: source SHA-256 `9b8e89b81a87001648d58dc9ef440a5b9b8c214a4df07bd22776da1ff6e32004`, 690,843 stereo PCM16 frames. Every normalized sample and original PCM16 byte, including forward/backward/frame-edge/EOF seeks, matched independent FFmpeg PCM SHA-256 `57cb3908bb26e91f372c1f0e8f10e68a699f90107d486546ceef9ff695fc2a7b`. The unmodified Rust-only dependency failed frame zero with a range-coder error before SDK integration.

The [FFmpeg FATE Monkey's Audio suite](https://github.com/FFmpeg/FFmpeg/blob/master/tests/fate/monkeysaudio.mak) provides truncated `luckynight-mac{380,388,389b1,391b1,392b2,394b1}-c2000.ape` samples under <https://fate-suite.ffmpeg.org/lossless-audio/>. Acceptance retained only complete compressed frames and rewrote count/last/wrapper fields while preserving historical seek/bit offsets, producing exactly 73,728 PCM frames per sample. Every sample and exact seek matched independent FFmpeg PCM. The original truncated programmes remain correctly refused as complete sources. These third-party recordings are not redistributed as original project fixtures. Their retained extent/hash record is `historical-reference-provenance.json`; `cargo run -p aede-audio --example verify_legacy_ape --features ape-pcm16 -- INPUT.ape REFERENCE.s16le` repeats the public-API comparison on a separately acquired complete source.
