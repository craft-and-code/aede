# Audio decoding and external-tool removal

## Scope and source of truth

This is the step-1 inventory for the shared audio work across Aède, FlacCompagnon and Phémios. It separates catalogue recognition, PCM decoding, spectral rendering and device output. A filename extension or successful tag read is not evidence that its audio is decodable. The [DSD design](native-dsd.md) defines the first native DSD profile; [shared audio](shared-audio.md) defines the ownership boundary and [spectrograms](../spectrograms.md) defines image publication.

The tables describe the **local source candidates**, not the currently installed Apps. The DSD APIs and Rust spectrogram integration are committed locally but not yet published together. Stable-source build preparation still selects published Aède 0.7.5; the existing dependency pins have not been replaced by sibling-checkout paths. Publication must follow the order described in the DSD design.

Removing the experimental CLI player's `ffplay` output is explicitly outside this work. Conversion/encoding, acoustic fingerprinting, downloading, enrichment and operating-system media tools are also outside the decoder-removal criterion. They must not be mistaken for playback/spectrogram dependencies.

## File formats and codecs

**Native** means in-process decoding, not native DSD output, strict bit-perfect output or physical gapless acceptance. **Compatibility** means an optional FFmpeg subprocess. **Unsupported** means an explicit error; it does not mean a new compatibility path should be invented. FlacCompagnon's preview uses the shared reader, whereas its original-integer analysis remains separate.

| Container / codec | Catalogue in Aède | Aède / Phémios catalogue PCM playback | FlacCompagnon minimal preview | Spectrograms: Aède / FlacCompagnon | External dependency still relevant |
| --- | --- | --- | --- | --- | --- |
| Native FLAC, ordinary integer precision | Native tags, STREAMINFO duration | Verified native reader, original-integer MD5 at complete EOF | Shared native reader | Native / native | None for qualified profiles |
| Ogg FLAC | Outside the dedicated Vorbis/Opus metadata contract; fallback admission must be qualified | Shared reader can decode admitted inputs, but catalogue admission is separate | Shared native reader where admitted | Aède catalogue admission still required / FC container and extension admission must be qualified | Not a promise of complete catalogue or Ogg-FLAC support |
| FLAC 32-bit independent channels | Native tags | Native verified reader | Shared Symphonia admission is narrower | Existing Aède reader / FC decoder admission is narrower | Aède spectral compatibility on native refusal |
| FLAC 32-bit correlated stereo (33-bit side channel) | Native tags | Explicit native refusal | Unsupported by the pinned native codec | Compatibility / unsupported by native FC readers | FFmpeg for Aède spectrograms; no new playback promise |
| WAV PCM, AIFF/AIFC PCM | Native container/tag readers | Native Symphonia for admitted representations/layouts | Shared native reader | Native / native | Unsupported representations remain explicit; compressed WAV/AIFC may reach Aède spectral compatibility |
| MPEG layers I/II/III | Native tags; codec follows actual layer | Native for admitted streams | Shared native reader | Native / native | No decoder subprocess for qualified profiles |
| Ogg Vorbis | Native tags | Aède's native Vorbis adapter | Shared native reader | Native / native | None for qualified profiles |
| Ogg Opus | Native tags and pre-skip | Shared native mono/stereo profile first; compatibility outside it | Shared native mono/stereo profile | Native first / native for the admitted profile | FFmpeg remains optional in Aède for unsupported profiles, including multistream |
| AAC-LC in ADTS or MP4/M4A | Native MP4 or Lofty ADTS metadata; known LC config supplies layout | Native for qualified MP4 edits; compatibility on other open failures | Shared native reader for the admitted profile | Qualified native MP4/ADTS first / shared native analysis | MP4 mono/44.1 kHz and stereo/48–96 kHz references include explicit delay/padding, counted duration and native seek; unknown/complex edits retain Aède compatibility |
| Other AAC profiles, including HE-AAC extensions | Recognized metadata is not decoder validation | Profile-dependent; compatibility may decode it | No blanket support | Profile-dependent compatibility / explicit native failure | Requires independently encoded profile fixtures before removing compatibility |
| ALAC in MP4/M4A | Native tags/configuration | Shared codec configuration for qualified mono/stereo M4A; compatibility on other open failures | Shared native reader | Qualified native mono/stereo first / shared configuration for native analysis | ALAC16 mono/44.1 kHz and ALAC24 stereo/96 kHz references are qualified in the candidate; other profiles retain their existing boundaries |
| Monkey's Audio / APE | Lofty metadata | Shared reader candidate; catalogue selector unchanged | Shared native reader | Native admitted bounded mono/stereo / native spectral visitor | Shared SDK covers bounded historical 3.80–3.98; Phémios CUE uses exact shared PCM16, including bounded prefixes |
| WavPack, integer/float/hybrid | Lofty metadata; losslessness/unknown float precision kept distinct | Shared reader candidate; catalogue selector unchanged | Shared native reader | Native admitted mono/stereo PCM / native spectral visitor | No DSD or automatic correction-file support; compatibility outside admission |
| Musepack SV7/SV8 | Lofty metadata | Shared reader candidate; catalogue selector unchanged | Shared native reader | Native admitted mono/stereo / native spectral visitor | Independent PCM qualification currently SV8; reference reader below 2 GiB |
| Ogg Speex | Lofty metadata | Shared reader candidate; catalogue selector unchanged | Shared native reader | Native single-stream NB/WB/UWB mono/stereo / native spectral visitor | Coded PCM length; no invented original gapless window |
| DSF or uncompressed DFF, DSD64/128, 44.1-kHz family, mono/stereo | Native candidate: genuine DSD clock, one bit, exact source duration | Native shared listening profile → 88.2-kHz PCM | Same native listening profile | Native measurement profile / native measurement profile | None; measurement preserves a wider band than listening |
| Other uncompressed DSD clocks/layouts | Container admission and metadata are separate from native capability | Explicit native refusal | Explicit native refusal | No general Aède spectral promise / existing optional compatibility profile | FFmpeg remains optional for applicable FC analysis/spectrogram profiles |
| DST-compressed DFF, SACD ISO | No native PCM claim | Unsupported | Unsupported | Unsupported; applicable header-only DSD analysis is not a spectrogram | Separate future scope, not a silent fallback |

Ordinary PCM readers validate the actual sample rate, channel count/layout and finite complete frames. They do not promise all profiles a codec specification could encode. Native FLAC's MD5 verdict is about original integer samples; f32 spectral/preview PCM must not replace that integrity path. DSD duration is based on source bits, while transport/seeking uses the derived PCM frame rate.

### Analysis is a separate contract

FlacCompagnon's original-integer FLAC path and its admitted Symphonia PCM paths provide full analysis without FFmpeg. Codecs outside native admission do not gain FC analysis support merely because Aède can decode them externally for playback or images. The candidate shared Opus factory now supplies the admitted complete mono/stereo profile to analysis as well as preview and spectral paths. AAC-LC MP4 analysis uses the shared admitted edit window and counts its playable frames; ADTS without original-length metadata retains coded PCM length. See [AAC admission](shared-audio.md#candidate-aac-lc-mp4-input) for its explicit profile and compatibility boundaries. Native DSD analysis uses the shared **measurement** profile; other applicable uncompressed DSD profiles retain the existing optional FFmpeg visitor. Header-only reports and codec recognition are not successful complete decodes. Checks such as original FLAC MD5, effective precision, stereo and lossy/transcode detection each retain their existing applicability rules; no table entry makes every check applicable to every format.

Explicit Opus/MP3 exports in FlacCompagnon reuse the shared `RateConverter` when their codec rate differs from the source. This filters before decimation, preserves the ceiling of the resampled frame count and admits the existing 8–384 kHz converter profile; outside it the export fails explicitly. The file conversion adapter is in FC services; its filter remains in Aède's DSP crate. Encoding and whole-file conversion memory remain outside the progressive playback/spectrogram contract.

## Other Phémios sources

| Source | Decoder / transport | FFmpeg or FFprobe | Boundary |
| --- | --- | --- | --- |
| Physical CD-DA; CUE/BIN | `phemios-cd`, exact stereo PCM16 at 44.1 kHz | Neither | Temporary local sources, bounded sector reads, existing transport/output |
| CUE/WAV PCM16 or CUE/FLAC | Symphonia in `phemios-cd` | Neither | Stereo 16-bit 44.1 kHz, integral 588-frame sectors, total ≤120 minutes |
| CUE/APE | Pinned read-only descriptor, shared native PCM16 reader | None for admitted APE 3.80–4.10, including prefixes up to 1 MiB; pre-3.80/out-of-budget profiles are explicitly unavailable | Same CD-quality contract; conversion is not implicit |
| Radio and podcast streams | Existing Phémios media-element/Web Audio transport; stream formats depend on the browser/webview | Neither launched by this transport | Platform codec availability is separate from catalogue/native-file capability |
| Web demo / WebAssembly | Browser decoding plus the existing Rust DSP adapter | Neither | WASM DSP is not a native file decoder; it grants no CD/device or codec parity |
| UPnP/OpenHome/Cast/SlimProto originals | Aède selected-original transfer | No host decode implied | Renderer codec support, account leases and device acceptance remain separate |
| HQPlayer output | Existing explicit external output route | Not an FFmpeg replacement | Receiver capabilities and source/owner cancellation contracts remain authoritative |

## Executable call sites

| Owner and source | Purpose | Removal decision |
| --- | --- | --- |
| Aède `playback/decoder.rs`, `decoder/ffmpeg.rs` | Optional Opus/AAC/ALAC PCM | Replace codec by codec, preserving delay, padding, channels and progressive cancellation |
| Aède `spectrum.rs`, `spectrum/legacy.rs` | Unsupported-codec PCM; Rust performs FFT, labels and PNG | Reuse new shared/native decoders before dropping each compatibility selector |
| FC `decode/dsd.rs`, `pipeline/dsd.rs` | Existing non-native DSD measurement profiles | Extend shared DSD admission only after filter/layout qualification; never substitute the listening low-pass for measurements |
| FC `spectrogram.rs`, native `commands/analysis.rs` and `spectrogram.rs` adapters | Lazily resolve FFmpeg only for applicable DSD compatibility | Ordinary PCM and admitted native DSD already avoid discovery/spawn |
| Phémios `phemios-cd/src/ape.rs` and `ape_header.rs` | APE characteristics and exact CUE PCM | Native descriptor gate plus shared original PCM16; no external probe/decoder. Historical 3.80–3.98 uses the approved shared SDK; out-of-profile sources remain explicit refusals |
| Aède `playback/output.rs` | Experimental CLI `ffplay` output | Excluded by request |
| Aède copy/fingerprint tools and explicit conversion workflows | Encoding or acoustic fingerprint extraction | Excluded; no claim that every use of FFmpeg in the repositories is removed |

No FFprobe is needed by Aède's spectral fallback: the external decoder emits a bounded self-describing float WAV header. It establishes actual rate/channel count rather than trusting catalogue hints. Phémios CUE/APE no longer invokes FFprobe or FFmpeg; profiles outside the shared native gate are explicit refusals. Decoder executables are never automatically downloaded.

## Corpus and acceptance checklist

Keep existing original fixtures and generators; extend them rather than adding redundant container copies. A listed fixture is software coverage, not acceptance of every stream using that codec. The matrix is complete as an inventory; the global qualification corpus is **not yet complete**.

| Family | Existing evidence | Required before deleting compatibility |
| --- | --- | --- |
| FLAC/WAV/Vorbis/MPEG | Existing real playback, metadata and analysis fixtures; original-integer integrity and gapless cases | Add each missing rate/depth/channel/representation explicitly, especially correlated FLAC32 and compressed WAV/AIFC |
| ALAC | Candidate standard-cookie M4A mono16/44.1 kHz and stereo24/96 kHz references; every original sample, codec clock/layout/precision, exact duration, native seek and spectral resolver checks | Additional depths/rates/layouts, legacy cookies, CAF and platform/listening acceptance; existing explicitly declared layouts retain prior behavior |
| AAC | Candidate AAC-LC MP4 mono/44.1 kHz, stereo/48 and 96 kHz, short, rounded-movie-clock and longer-than-buffer sources; independent float references, exact playable length, seeks, cancellation, native spectral resolver checks and missing-tail refusal; ordinary ADTS LC retains coded duration | HE/SBR/PS, PCE/multichannel/960-frame profiles, other container edits/metadata conventions and platform/listening acceptance; no blanket AAC qualification |
| Opus | Real mono/stereo/very-short fixtures, multi-packet pre-skip, final granule, gain, corrupt/truncated/reset/cancel cases, native seek, FC encoder round trips and spectral resolver spy | Multistream, nonzero-origin, chained/live streams and other platform/listening acceptance remain separate profiles |
| APE | Original CD-quality `ramp.ape`, exact every-sample CUE seeking/track boundaries | Native metadata and every-sample PCM16 seek/EOF tests, prefixed variants and a child with empty PATH; independent historical 3.80/3.88/3.89/3.91/3.92/3.94/3.97 every-sample and seek evidence is recorded in the shared fixture README |
| WavPack/Musepack/Speex | Candidate native integer/float/hybrid WavPack, SV8, Speex NB/WB/UWB references plus corruption/cancellation/source replacement | SV7 independent reference, larger corpus, remaining channel/profile and target-platform qualification |
| DSD | Shared DSD64 mono and DSD128 stereo DSF/DFF pairs, exact tails, filtering, cancellation, native transport and spectrogram tests | Real libraries, listening/ultrasonic references and physical/platform throughput; additional rates/layouts remain unqualified |

For every replacement, test ordinary and mislabeled inputs, truncation, huge/inconsistent metadata, exact EOF/duration, channel ordering, cancellation after read/before publication, pause/Stop/seek, source replacement, account/logout retirement and changes of rate/layout between tracks. Keep source-stamped caches and atomic spectral publication. The original file must remain unchanged. A failed or cancelled decoder must not publish a completed analysis/image or start another source.

Require tests that prove the affected native path never calls an external resolver, including with no FFmpeg/FFprobe installed; a resolver spy or isolated child environment avoids mutating global PATH in parallel tests. Keep separate compatibility-reference tests where executables are intentionally present. Verify bounded memory and playback priority before broad performance claims. Native macOS/Windows/Linux execution, Linux musl/ARM64 toolchain acceptance and hardware audio remain separate gates. No App rebuild or release is performed by completing this inventory.

## Next replacement order

1. Native CUE/APE descriptor inspection and shared exact PCM16 are connected for the admitted 3.80–4.10 profile; versions outside that profile are explicitly refused.
2. Shared Opus mono/stereo decoding is implemented in the source candidate after approval to reuse `audiopus`/libopus. Aède playback and both spectral/analysis paths reuse it. Qualify additional Opus profiles separately; retain compatibility meanwhile.
3. Shared AAC-LC MP4 edit handling and ALAC cookie/timeline admission are implemented in the candidate for the qualified profiles above. Expand the independent corpus before removing compatibility for additional encoders, edits or layouts.
4. Qualify remaining native APE/WavPack/Musepack/Speex profiles/platforms, including unqualified historical compression levels/precisions. Shared PCM16 CUE integration now removes its FFmpeg/FFprobe calls; publication and native-platform acceptance remain separate.
5. Expand DSD profiles separately, then run the common lifecycle/cross-platform matrix and publish in dependency order.
