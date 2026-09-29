# DSP product proposal and research notes

Status: research and product proposal, not an approved product contract. Researched on 2026-09-29. Scope: local music playback and optional playback processing, not recording, mastering, DAW effects, or network transport. Prices and entitlements remain to be decided.

## Present implementation

`aede-dsp` currently has a decoded interleaved `f32` PCM contract, a continuous gain stage, optional broad bass/treble shelves, integrated LUFS and true-peak measurement, sample-peak and over-full-scale reporting, a conservative headroom cap, an explicit final sample ceiling, and a 24-band spectrum visualizer. `aede-core` can select ReplayGain and Opus R128 gains from metadata. Its `PcmTrack` layer supplies processed `f32le` blocks and format metadata to either a local sink or a future server transport. `aede play` sends PCM with optional ReplayGain/Opus R128 metadata gain to a CPAL device stream when a matching floating-point format is available, or to the ffplay fallback. Both retain their output across consecutive files of the same decoded format and reopen it for format changes. MP3 delay/padding and Opus pre-skip/end trimming are tested against real fixtures; matching-format FLAC-to-MP3 output has no inserted PCM frames. The native callback preserves sample order across track boundaries, but an underrun is still possible if file opening or decoding exhausts its bounded queue; physical gapless output has not been measured on hardware. Native Vorbis end trimming remains uncertain without ffmpeg. F5 now reuses current FlacCompagnon track LUFS and true peak or measures missing track and album programme loudness in `aede-dsp` with the pure-Rust `ebur128` crate. `aede-core` decodes audio and caches results by file identity in the independent conclusions store. F3/F4 apply the selected gain and output headroom policy. F6 adds flat-bypassable 120 Hz/4 kHz tone shelves with a conservative boost preamp. F7 preserves source speaker masks and downmixes known multichannel layouts to stereo for the local player, omitting LFE and refusing unknown layouts. The visualizer is an approximation of data entering the selected output, not device output. The server does not yet transmit audio. See [current state](../coding/current-state.md), [playback design](playback.md), and [`aede-dsp` README](../../crates/aede-dsp/README.md).

## DSP library choices

| Need | Library | Decision |
| --- | --- | --- |
| Integrated LUFS and true peak | [`ebur128`](https://github.com/sdroege/ebur128) | Used inside `aede-dsp::loudness`. It is a Rust port of C `libebur128` and provides gated measurements across multiple meter histories, including programmes with format changes. |
| Broad bass/treble shelves (F6) | [`biquad`](https://docs.rs/biquad/0.6.0/biquad/) | Used in `aede-dsp` for W3C Audio EQ Cookbook shelf coefficients and per-channel filter state. Flat playback bypasses the filters. |
| Device-rate conversion (F8) | [`Rubato`](https://github.com/HEnquist/rubato/) | Candidate for a stateful bandlimited resampler; evaluate its API, latency and measured response when F8 begins. |
| PCM building blocks | [`dasp`](https://github.com/RustAudio/dasp) | Available if a specific later stage benefits from its sample/frame primitives; no dependency is needed for the current gain and meter. |

Keep decoding, metadata precedence and persisted measurements in `aede-core`. Put processing and measurements that need only PCM in `aede-dsp`, so CLI and future network playback use the same signal code.

## Meaning of confidence

The percentages below are *planning estimates*, not measured probabilities. They indicate confidence that Aède can implement and validate an excellent production-quality version using the present architecture and public references, without a specialist or licensed technology. They combine algorithm clarity, cross-platform integration, testability, and perceptual validation. They are not a claim that the feature is already implemented, or that every listener will prefer its sound. Anything below about 75% merits specialist review, measured reference material, or both.

The order within each table is the proposed development order. An item marked "conditional" only applies when the corresponding output format or device is supported. Numbers are independent within each tier; Premium begins after the free foundation, although some Premium controls can be developed in parallel.

## Free foundation

| Priority | Capability | User meaning and completion criterion | Confidence | References |
| --- | --- | --- | ---: | --- |
| F1 | Persistent output and format negotiation | One output stream across tracks; choose a supported device format and explicitly handle sample rate and channel layout changes. Required foundation for all processing. | 85% | [1], [7] |
| F2 | Exact gapless playback | Remove encoder delay and end padding when declared, preserve sample-continuous album transitions, and test with known PCM joins. This is playback engineering adjacent to DSP. | 82% | [2], [3] |
| F3 | Metadata loudness normalization | Actually apply selected ReplayGain/Opus R128 gain; choose album or track scope, with an off switch. Preserve Opus header gain semantics. | 93% | [3], [4], [5] |
| F4 | Output headroom and clipping policy | Track gain throughout the chain, provide safe attenuation when needed, and make clipping/limiting decisions explicit at output. Do not silently clip floating PCM. | 90% | [4], [5], [6] |
| F5 (implemented) | Measured loudness fallback | Reuse current FlacCompagnon integrated track LUFS and true-peak analyses when available; measure missing tracks and album programmes separately, caching derived results without modifying files. Do not average track LUFS to estimate an album. | 83% | [4], [5] |
| F6 (implemented) | Basic sound controls | Optional broad bass/treble EQ with reset and safe preamp; keep a clean bypass path. A small usable EQ is a fair free capability. | 94% | [8] |
| F7 (implemented) | Robust channel handling | Mono/stereo handling and standards-based multichannel downmix with documented LFE choices; preserve channel labels. | 84% | [9] |
| F8 | Device-rate conversion, conditional | Use a stateful bandlimited resampler only when the output device cannot accept the source rate; validate bandwidth, aliasing, latency, and transitions. | 78% | [7] |
| F9 | Final integer conversion and dither, conditional | When an integer PCM sink is used, apply controlled quantization and TPDF dither; bypass this for a floating-point sink or an untouched compatible path. | 91% | [10] |
| F10 | Honest metering and diagnostics | Show sample peak, true peak, gain reduction/headroom and exact stage status. Preserve the spectrum visualizer as an optional display rather than an audio effect. | 90% | [4], [11] |

## Premium processing

| Priority | Capability | User meaning and completion criterion | Confidence | References |
| --- | --- | --- | ---: | --- |
| P1 | Parametric equalizer | Multiple adjustable bell, shelf, high-pass and low-pass filters with frequency, gain, Q, bypass, presets, and click-free parameter changes. | 91% | [8] |
| P2 | Per-device profiles and preset import/export | Separate headphone/speaker/output settings and portable profiles. Validate values and make the active profile obvious. | 95% | [8] |
| P3 | Convolution engine and user impulse responses | Apply user-supplied FIR room/headphone filters with bounded latency, resampling policy and safe transitions. This is the engine, not automatic room correction. | 82% | [12] |
| P4 | Headphone model EQ | Provide opt-in correction profiles based on identified headphone models; quality depends on measurements, fit, and licensing of profile data. | 72% | [13] |
| P5 | Crossfeed | Blend a delayed/filtered portion of each stereo side into the opposite headphone side to soften hard-panned stereo; include strength and bypass. Listening validation matters. | 77% | [14] |
| P6 | True-peak limiter | Optional look-ahead ceiling after EQ/convolution, with oversampled true-peak check, latency disclosure and gain-reduction meter. It must not serve as a hidden fix for unsafe gain. | 78% | [4], [15] |
| P7 | Gentle dynamic-range compression / night mode | Reduce loud/quiet spread for noisy rooms or late listening; expose a simple preset first and avoid claiming fidelity improvement. | 79% | [15] |
| P8 | Crossfade / transition curves | Blend unrelated tracks with configurable duration and curve; automatically disable for gapless album sequences unless requested. This is a playback/processing hybrid. | 89% | [1] |
| P9 | Channel trims, balance, delay and polarity | Calibrate individual channels, correct left/right imbalance, and align speakers within bounded delay and level ranges. | 87% | [9] |
| P10 | Crossover and bass management | Route bass to a subwoofer with configurable cutoff, slope, polarity and timing. Requires suitable multichannel outputs and listening measurements. | 72% | [9], [16] |
| P11 | Measurement-assisted room correction | Calibrated microphone sweeps at several listening positions, target curve, conservative low-frequency correction, and before/after validation. A specialist should review the acoustics and user workflow. | 55% | [16] |
| P12 | Dynamic EQ / loudness compensation | Change tonal balance with level or content; requires calibrated listening level and careful validation, otherwise results are arbitrary. | 65% | [17] |
| P13 | Mid/side and stereo width | Adjust centre-versus-side content, with mono-compatibility and gain safeguards; useful as a creative control, not an objective quality upgrade. | 84% | [12] |
| P14 | Time stretch and pitch shift | Change playback speed without changing pitch (or change pitch separately), preserving transients and acceptable CPU/latency. Useful for practice/listening, not central to hi-fi. | 68% | [18] |
| P15 | Binaural/spatial rendering | Render supported multichannel material over headphones with HRTFs, optional head tracking, and SOFA data handling. High research and listening-test burden. | 52% | [19] |

## Boundaries and release gates

1. The free tier must retain transparent playback, format compatibility, normalization, and clipping protection. A bypass that performs no EQ/effects should be available. "Bit-perfect" may only be claimed for a proven untouched compatible signal path; enabling gain or EQ changes samples.
2. Keep file decoding, DSP, and device output separate. Avoid allocations in the real-time processing call, and make sample rate, channel layout, processing latency, and tail handling explicit. Preserve Aède's rule of never modifying files or tags.
3. At minimum, test a filter's frequency/phase response and stability, resampler aliasing and passband, sample and true-peak handling, album/track loudness, gapless joins, block-size invariance, and preset serialization. Listening claims need blinded tests; ITU-R BS.1116 covers small impairments and BS.1534 covers intermediate differences [20].
4. Room correction needs acoustic measurement expertise and real rooms. Headphone profiles need rights and measurement provenance. Spatial rendering and time stretch need perceptual review. These are partnership candidates rather than promises for the first Premium release.
5. Keep optional creative effects (reverb, exciter, saturation, multiband compression, noise reduction, source separation, upmix) outside the committed roadmap until a concrete user case and listening protocol exist. They are technically possible but do not define a high-quality music player.

## Source library

The links below are standards, original research, maintainers' technical documentation, and identified technical community references. A paywall is noted where applicable.

1. Aède [playback design](playback.md) and [`aede-dsp` README](../../crates/aede-dsp/README.md).
2. Hydrogenaudio, [Gapless playback](https://wiki.hydrogenaudio.org/index.php?title=Gapless) (technical community reference for encoder delay/padding).
3. IETF, [RFC 7845: Ogg Encapsulation for the Opus Audio Codec](https://www.rfc-editor.org/rfc/rfc7845.html) (pre-skip, end trim, output gain, R128 gain).
4. ITU-R, [BS.1770-5: Algorithms to measure audio programme loudness and true-peak audio level](https://www.itu.int/rec/R-REC-BS.1770-5-202311-I).
5. AES, [TD1008: Recommendations for Loudness of Internet Audio Streaming and On-demand Distribution](https://aes.org/wp-content/uploads/2024/01/20210924_TD1008_v3.13.pdf); EBU, [R 128 S2: Loudness in Streaming](https://tech.ebu.ch/publications/r128s2); Hydrogenaudio, [ReplayGain 2.0 specification](https://wiki.hydrogenaudio.org/index.php?title=ReplayGain_2.0_specification).
6. AES, [Loudness Normalization](https://aes.org/resources/audio-topics/loudness-project/loudness-normalization/).
7. libsamplerate, [FAQ](https://libsndfile.github.io/libsamplerate/faq.html), [full streaming API](https://libsndfile.github.io/libsamplerate/api_full.html), and [quality discussion](https://libsndfile.github.io/libsamplerate/quality.html).
8. W3C, Robert Bristow-Johnson, [Audio EQ Cookbook](https://www.w3.org/TR/audio-eq-cookbook/).
9. ITU-R, [BS.775-4: Multichannel stereophonic sound system](https://www.itu.int/rec/R-REC-BS.775-4-202212-I).
10. Lipshitz, Wannamaker & Vanderkooy, [Quantization and Dither: A Theoretical Survey](https://aes.org/publications/elibrary-page/?id=7047), *JAES* 40(5), 1992 (full paper paywalled).
11. ITU-R, [BS.1771-1: Requirements for loudness and true-peak indicating meters](https://www.itu.int/dms_pubrec/itu-r/rec/bs/r-rec-bs.1771-1-201201-i%21%21pdf-e.pdf).
12. FFmpeg, [Audio filter documentation](https://ffmpeg.org/ffmpeg-filters.html), notably `afir`, `equalizer`, `crossfeed`, `alimiter`, `acompressor` (implementation reference, not a recommendation to add FFmpeg as a new DSP dependency).
13. Olive, Welti & McMullin, [Listener Preferences for In-Room Loudspeaker and Headphone Target Responses](https://aes.org/publications/elibrary-page/?id=17042), AES Convention Paper 8994, 2013 (full paper paywalled).
14. FFmpeg, [crossfeed filter documentation](https://ffmpeg.org/ffmpeg-filters.html#crossfeed).
15. Giannoulis, Massberg & Reiss, [Digital Dynamic Range Compressor Design—A Tutorial and Analysis](https://eecs.qmul.ac.uk/~josh/documents/2012/GiannoulisMassbergReiss-dynamicrangecompression-JAES2012.pdf), *JAES* 60(6), 2012.
16. miniDSP, [Room Correction 101](https://www.minidsp.com/applications/digital-room-correction/drc-basics) and [measurement guidance](https://docs.minidsp.com/product-manuals/shd-studio/dirac-live/measure.html) (vendor documentation; independent acoustic validation remains necessary).
17. FFmpeg, [dynamic equalizer](https://ffmpeg.org/ffmpeg-filters.html#adynamicequalizer) (implementation reference).
18. Signalsmith Audio, [Signalsmith Stretch](https://git.signalsmith-audio.co.uk/Signalsmith-Audio/signalsmith-stretch/src/branch/main/README.md) (original implementer documentation).
19. AES69 / SOFA, [Spatially Oriented Format for Acoustics](https://www.sofaconventions.org/) (HRTF and spatial impulse-response interchange).
20. ITU-R, [BS.1116-3: Small impairments](https://www.itu.int/rec/R-REC-BS.1116-3-201502-I) and [BS.1534-3: Intermediate audio quality (MUSHRA)](https://www.itu.int/rec/R-REC-BS.1534-3-201510-I).
