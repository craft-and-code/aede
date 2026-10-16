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

WavPack losslessness follows the selected stream's declared properties, including hybrid lossy streams. Lossy streams have no reported lossless PCM depth; this does not add `.wvc` correction-file handling. For fallback formats, the primary tag (such as APEv2 in WavPack) precedes legacy alternatives such as ID3v1, while unique secondary values remain available. Rereading metadata with `scan --full` applies these rules to already cached entries.

Floating-point WavPack retains its known codec and losslessness, but its PCM precision is left unknown because the shared properties cannot describe that representation. The PCM representation check examines at most 64 WavPack block headers to identify the first audio block; an unresolved representation also leaves precision unknown. Conversion to FLAC/WAV then refuses before encoding. Unchanged copying and explicit lossy conversion remain available; floating-point WavPack conversion support is not added.

### Parser precision and resilience

The dedicated readers retain details needed by later operations, including LAME encoder delay/padding, Opus pre-skip, channel layout and ALAC stream information. Regression fixtures cover malformed/truncated inputs and real files produced by media tools. A successful metadata read still does not verify every audio sample or establish file integrity; use [integrity checks](integrity.md) or attributed [acoustic analysis](imported-analyses.md) for their respective questions.

For MPEG audio, the reported codec follows the parsed layer (`mp1`, `mp2` or `mp3`), rather than assuming Layer III from the `.mp3` filename. This does not add file extensions or guarantee a new playback decoder.

ID3v2.3 unsynchronisation is undone at tag scope; ID3v2.4 is undone at frame scope after consuming the encoded frame size, so following frames keep their boundaries. Group identity and optional data-length headers are removed before decoding text. Legacy ID3 `TDAT`/`TDA` day/month values are retained as `recordingdaymonth`, separately from year-bearing `date` values.

ID3 custom and MP4 free-form MusicBrainz keys are normalized once. The release-track identifier stays separate from the recording identifier; it cannot create or split canonical recording identities. The compatibility `Track.mbid` field still names the recording. Use `scan --full` to reread these tags in an existing metadata cache.

For legacy FLAC files with a leading ID3v2 tag, native Vorbis comments take precedence and distinct ID3 fields/values remain available. Both path and opened-file metadata reads apply this rule.

Path-based metadata, embedded-artwork extraction and native integrity/resolution readers refuse nonregular audio sources before opening them, including static FIFOs; links to ordinary files remain supported. They recheck the opened descriptor, but this metadata/open sequence does not prevent concurrent path replacement.

MP4 stream properties are read from audio tracks, so a video track cannot replace them. Extensible PCM WAV distinguishes valid sample bits from storage width; unsupported compressed AIFC has no inferred PCM precision or duration.

For AAC in MP4, rates above 65535 Hz use the bounded `esds` decoder configuration because they cannot fit the sample entry's 16.16 field. Ordinary-rate entries retain their existing interpretation; this is not a complete HE-AAC extension validator or decoder. Truncated or unsupported configurations do not replace the entry's rate. Reread existing cached metadata with `scan --full` to apply the corrected high rate.

Ogg duration lookup follows complete page boundaries. Its end window uses the existing page CRC to recognise a page while resynchronising at an unknown byte boundary; this bounded lookup is not a whole-file integrity check and does not decode the audio.

## Dependencies and playback

`lofty` handles the secondary tag-reading formats. FlacCompagnon's Rust library provides in-process acoustic analysis. These are library dependencies; the user does not install a standalone FlacCompagnon executable to use `aede analyze`.

The current progressive player has native decoding paths for tested FLAC, PCM WAV, LAME MP3 and native Vorbis fixtures. Opus, AAC and ALAC in M4A use an optional FFmpeg decoding path. Other catalogued formats are not thereby promised a supported player path. The [play command](cli/play.md) documents current supported selections and errors.

CPAL provides native output when the build/platform/device supports it; ffplay is the fallback. Linux musl release archives use ffplay, while GNU Linux source builds can use ALSA output. FFmpeg is also an external tool for `copy --compress` and applicable spectrogram/decoder paths. Fingerprinting needs `fpcalc` or an FFmpeg build containing Chromaprint; ordinary FFmpeg installation does not promise that feature.

### Why preserve decoder details?

Trimming and frame bounds support continuous software processing and prevent encoder padding from becoming unintended playback frames. They do not alone prove seamless physical playback on every audio device. Hardware join/drain timing and target NAS performance remain validation work. Do not describe the current player as universally bit-perfect or physically gapless.

See [installation](manual/install.md), [play](cli/play.md), [copy](cli/copy.md) and [fingerprint](cli/fingerprint.md) for practical setup and command options.
