# Original AAC-LC timing references

`generate.py` creates only original synthetic 16-bit PCM tones (997 Hz left,
2711 Hz right, amplitude 9000), encodes AAC-LC with FFmpeg 9.0.2 and decodes
independent f32 little-endian references. No third-party music is included.
FFmpeg is used to generate references, never by the native tests themselves.

| Source | Rate | Channels | Original/playable frames |
| --- | --- | --- | --- |
| mono441 | 44100 | 1 | 10001 |
| stereo48 | 48000 | 2 | 48013 |
| stereo96 | 96000 | 2 | 12037 |
| short48 | 48000 | 1 | 121 |
| stream48 | 48000 | 2 | 240013 |

MP4 movie clocks initially match PCM clocks, giving exact single-edit lengths.
`coarse48.m4a` remuxes `stereo48` with a 1000 Hz movie clock; its short final
STTS delta still supplies the exact end. It uses `stereo48.f32` as reference.
`stream48` exceeds the probe buffer and exercises descriptor-cursor restoration.
The generator explicitly crops FFmpeg's float output to the known source length:
its decoder drops start priming but may retain the last coded packet's padding.
Tests compare complete channel-ordered float PCM at better than 80 dB SNR;
AAC is lossy, so this is a decoder comparison, not lossless source recovery.

`mono441.aac` remuxes the same encoded packets to ADTS. Without the MP4 edit,
it retains 11 × 1024 coded frames; no guessed original length is asserted.
Truncated and unsupported-edit variants are constructed in temporary files.
Source files and tags stay unchanged during tests. The equivalent FlacCompagnon
fixture directory retains these same source/reference bytes.
