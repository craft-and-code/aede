# Progressive 32-bit FLAC playback fixtures

These tiny native FLAC files were assembled from known signed integer samples
with STREAMINFO, one verbatim audio frame, CRC-8/CRC-16 and the decoded-audio
MD5. Each contains 64 frames at 48,000 Hz and 32 bits per sample. The frame
header inherits its bit depth from STREAMINFO, rather than using the explicit
32-bit frame-header code unsupported by Symphonia 0.5.5.

All three files passed the reference decoder's `flac 1.5.0 --test` on
2026-10-04. The MD5 values below were independently computed over signed
little-endian, interleaved 32-bit PCM using Python's `hashlib.md5`.

| File | Channels | Channel assignment | Decoded PCM MD5 |
| --- | --- | --- | --- |
| `playback32-mono.flac` | 1 | Independent | `fccd7a0a1c8317a8be6566e141e44244` |
| `playback32-stereo.flac` | 2 | Independent | `d33c8c23048734b8ac8b03f0f157aa4f` |
| `playback32-mid-side.flac` | 2 | Mid/side | `d33c8c23048734b8ac8b03f0f157aa4f` |

The mono samples and stereo left channel repeat the following eight values
eight times:

```text
-2147483648, -2147483647, -16777217, -1, 0, 1, 16777217, 2147483647
```

The stereo right channel reverses the left channel's 64 samples. Both stereo
files therefore describe exactly the same PCM; only channel coding differs.
The mid/side file uses a 33-bit side subframe, which Symphonia 0.5.5 cannot
decode safely. It is a valid-source refusal regression, not a corrupt fixture.

The samples exercise signed extremes and low bits that can disappear when
converted to `f32`. Verification must use the original integer PCM before
floating-point conversion or DSP.
