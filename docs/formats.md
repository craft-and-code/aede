# Formats and optional tools

Aède distinguishes reading a file's tags/container from decoding its audio for playback or analysis. A format in the catalog table is not automatically a native playback format. Original files and tags remain read-only.

## Supported formats: reading the library

The primary readers handle these containers:

| Container | Codecs | Tags | Duration from |
| --- | --- | --- | --- |
| FLAC | FLAC | Vorbis Comment, leading ID3v2 | STREAMINFO |
| MP3 | MPEG 1/2/2.5 layers I–III | ID3v2.2/2.3/2.4, ID3v1 | Xing / VBRI / constant bitrate |
| MP4 | ALAC, AAC | iTunes atoms, freeform `----` | `mvhd` |
| Ogg | Vorbis, Opus | Vorbis Comment | Granule position |
| WAV | PCM | `LIST/INFO`, `id3 ` chunk | `fmt ` + `data` |
| AIFF / AIFC | PCM | `NAME`/`AUTH`, `ID3 ` chunk | `COMM` |

Extensions: `.flac`, `.mp3`, `.m4a`, `.m4b`, `.mp4`, `.alac`, `.ogg`, `.oga`, `.opus`, `.wav`, `.wave`, `.aif`, `.aiff`, `.aifc`.

Other recognized formats are read through the `lofty` fallback when a primary reader does not match:

| Container | Codecs | Tags | Duration from |
| --- | --- | --- | --- |
| AAC | AAC | ID3v2, ID3v1 | ADTS frame headers |
| WavPack | WavPack | APEv2, ID3v1 | Block headers |
| Monkey's Audio | APE | APEv2, ID3v1 | Descriptor |
| Musepack | Musepack SV7/SV8 | APEv2, ID3v1 | Stream header |
| Speex | Speex | Vorbis Comment | Granule position |

Extensions: `.aac`, `.ape`, `.wv`, `.mpc`, `.mp+`, `.mpp`, `.spx`.

### Parser precision and resilience

The dedicated readers retain details needed by later operations, including LAME encoder delay/padding, Opus pre-skip, channel layout and ALAC stream information. Regression fixtures cover malformed/truncated inputs and real files produced by media tools. A successful metadata read still does not verify every audio sample or establish file integrity; use [integrity checks](integrity.md) or attributed [acoustic analysis](imported-analyses.md) for their respective questions.

## Dependencies and playback

`lofty` handles the secondary tag-reading formats. FlacCompagnon's Rust library provides in-process acoustic analysis. These are library dependencies; the user does not install a standalone FlacCompagnon executable to use `aede analyze`.

The current progressive player has native decoding paths for tested FLAC, PCM WAV, LAME MP3 and native Vorbis fixtures. Opus, AAC and ALAC in M4A use an optional FFmpeg decoding path. Other catalogued formats are not thereby promised a supported player path. The [play command](cli/play.md) documents current supported selections and errors.

CPAL provides native output when the build/platform/device supports it; ffplay is the fallback. Linux musl release archives use ffplay, while GNU Linux source builds can use ALSA output. FFmpeg is also an external tool for `copy --compress` and applicable spectrogram/decoder paths. Fingerprinting needs `fpcalc` or an FFmpeg build containing Chromaprint; ordinary FFmpeg installation does not promise that feature.

### Why preserve decoder details?

Trimming and frame bounds support continuous software processing and prevent encoder padding from becoming unintended playback frames. They do not alone prove seamless physical playback on every audio device. Hardware join/drain timing and target NAS performance remain validation work. Do not describe the current player as universally bit-perfect or physically gapless.

See [installation](manual/install.md), [play](cli/play.md), [copy](cli/copy.md) and [fingerprint](cli/fingerprint.md) for practical setup and command options.
