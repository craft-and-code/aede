# Formats and dependencies

## Supported formats: Reading the Digital Grooves

A true CDthèque must be able to read every pressing on its shelves with absolute fidelity. The core of Aède is built on bespoke parsers, handcrafted to understand the exact anatomy of your audio files.

| Container   | Codecs                    | Tags                          | Duration from                  |
| ----------- | ------------------------- | ----------------------------- | ------------------------------ |
| FLAC        | FLAC                      | Vorbis Comment, leading ID3v2 | STREAMINFO                     |
| MP3         | MPEG 1/2/2.5 layers I–III | ID3v2.2/2.3/2.4, ID3v1        | Xing / VBRI / constant bitrate |
| MP4         | ALAC, AAC                 | iTunes atoms, freeform `----` | `mvhd`                         |
| Ogg         | Vorbis, Opus              | Vorbis Comment                | Granule position               |
| WAV         | PCM                       | `LIST/INFO`, `id3 ` chunk     | `fmt ` + `data`                |
| AIFF / AIFC | PCM                       | `NAME`/`AUTH`, `ID3 ` chunk   | `COMM`                         |

Extensions: `.flac` `.mp3` `.m4a` `.m4b` `.mp4` `.alac` `.ogg` `.oga` `.opus` `.wav` `.wave` `.aif` `.aiff` `.aifc`

For the rarer artifacts in your archive, the formats below are read through [`lofty`](https://crates.io/crates/lofty), an external crate which gracefully takes over whenever the file's signature matches none of the dedicated primary parsers above:

| Container      | Codecs           | Tags           | Duration from      |
| -------------- | ---------------- | -------------- | ------------------ |
| AAC            | AAC              | ID3v2, ID3v1   | ADTS frame headers |
| WavPack        | WavPack          | APEv2, ID3v1   | Block headers      |
| Monkey's Audio | APE              | APEv2, ID3v1   | Descriptor         |
| Musepack       | Musepack SV7/SV8 | APEv2, ID3v1   | Stream header      |
| Speex          | Speex            | Vorbis Comment | Granule position   |

Extensions: `.aac` `.ape` `.wv` `.mpc` `.mp+` `.mpp` `.spx`

This fallback is only ever reached last, ensuring it can never accidentally take a mainstream format away from one of Aède's primary, high-precision parsers.

### The Archivist's Parsers: Precision and Resilience

The primary parsers are written by hand directly from the official format specifications. They are rigorously validated against real audio files produced by ffmpeg and cross-checked with ffprobe (`crates/aede-core/tests/real_files.rs`).

Digital history is full of quirks, and a robust library must handle them all. Aède intentionally covers the most awkward archival cases: UTF-16 strings with a Byte Order Mark (BOM), the notorious ID3v2.3 unsynchronisation, legacy numeric genres like `(17)Rock`, the LAME encoder delay (crucial later for gapless playback), the ALAC magic cookie that reveals the true bit depth, and the Opus pre-skip.

**A damaged record should never crash the player.** There are strictly no `unwrap` calls and no direct memory indexing anywhere in the parsers. A violently truncated or corrupted file yields a polite error or a partial result, but never a catastrophic panic. An automated test explicitly enforces this guarantee by deliberately truncating a real file to a quarter, a third, and a half of its true size to ensure the parser always degrades gracefully.

## Dependencies: A Calculated Choice

Aède relies on exactly one dependency for audio reading: [`lofty`](https://crates.io/crates/lofty), which handles the secondary formats listed in the second table.

In this project, a dependency is a strict requirement, not a dogma. To be included, external code has to do something we could not reasonably do better ourselves, be actively maintained, widely used, and crucially, bring a dependency tree small enough to actually read and audit. `lofty` earns its place in the vault by seamlessly covering a long tail of niche formats that would each take days to parse by hand, yet are only exercised by a tiny fraction of most music collections.

### Why not use `lofty` for everything?

Because a generic tool cannot extract the microscopic details required for true high-fidelity playback.

The encoder delay and padding of the LAME tag, the ALAC magic cookie, and the exact Opus pre-skip are simply not exposed by any general-purpose library. Milestone M3 of this project relies heavily on these exact, sample-accurate metrics to achieve mathematically perfect gapless playback. That uncompromising standard is exactly why the main formats are kept firmly under the control of Aède's own custom parsers.
