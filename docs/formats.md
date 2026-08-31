# Formats and dependencies

## Supported formats

| Container   | Codecs                    | Tags                          | Duration from                  |
| ----------- | ------------------------- | ----------------------------- | ------------------------------ |
| FLAC        | FLAC                      | Vorbis Comment, leading ID3v2 | STREAMINFO                     |
| MP3         | MPEG 1/2/2.5 layers I–III | ID3v2.2/2.3/2.4, ID3v1        | Xing / VBRI / constant bitrate |
| MP4         | ALAC, AAC                 | iTunes atoms, freeform `----` | `mvhd`                         |
| Ogg         | Vorbis, Opus              | Vorbis Comment                | Granule position               |
| WAV         | PCM                       | `LIST/INFO`, `id3 ` chunk     | `fmt ` + `data`                |
| AIFF / AIFC | PCM                       | `NAME`/`AUTH`, `ID3 ` chunk   | `COMM`                         |

Extensions: `.flac` `.mp3` `.m4a` `.m4b` `.mp4` `.alac` `.ogg` `.oga` `.opus` `.wav` `.wave` `.aif` `.aiff` `.aifc`

The formats below are read through [`lofty`](https://crates.io/crates/lofty), which takes over whenever the signature matches none of the parsers above:

| Container      | Codecs           | Tags           | Duration from      |
| -------------- | ---------------- | -------------- | ------------------ |
| AAC            | AAC              | ID3v2, ID3v1   | ADTS frame headers |
| WavPack        | WavPack          | APEv2, ID3v1   | Block headers      |
| Monkey's Audio | APE              | APEv2, ID3v1   | Descriptor         |
| Musepack       | Musepack SV7/SV8 | APEv2, ID3v1   | Stream header      |
| Speex          | Speex            | Vorbis Comment | Granule position   |

Extensions: `.aac` `.ape` `.wv` `.mpc` `.mp+` `.mpp` `.spx`

The fallback is only ever reached last, so it can never take a format away from a parser above.

The parsers are written by hand from the specifications and validated against real files produced by ffmpeg and cross-checked with ffprobe (`crates/aede-core/tests/real_files.rs`). The awkward cases are covered: UTF-16 with a BOM, ID3v2.3 unsynchronisation, numeric genres like `(17)Rock`, the LAME encoder delay (needed later for gapless playback), the ALAC magic cookie for the real bit depth, the Opus pre-skip.

No `unwrap` and no direct indexing anywhere in the parsers: a truncated file yields an error or a partial result, never a panic. A test checks this by truncating a real file to a quarter, a third and a half of its size.

## Dependencies

One: [`lofty`](https://crates.io/crates/lofty), which reads the formats listed above as its own. A dependency is a requirement here, not a dogma — it has to do something we could not do as well ourselves, be maintained and widely used, and bring a dependency tree small enough to read. `lofty` earns its place by covering a long tail of formats that would each take days to parse and would be exercised by a handful of files.

What it does not replace: the encoder delay and padding of the LAME tag, the ALAC magic cookie and the Opus pre-skip are not exposed by any general-purpose library, and milestone M3 needs them for gapless playback. That is why the main formats keep their own parsers.
