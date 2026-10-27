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
