# Mono, stereo and multichannel audio

A **channel** is one signal intended for a speaker position. A channel count alone does not tell which is front-left, center or rear-right. Aède preserves a known speaker mask/layout when the decoder can supply it and refuses to invent positions for unsupported multichannel audio.

## Local playback mapping

Mono and stereo retain their ordinary channel meaning. Known multichannel sources are downmixed to stereo before shared gain/tone/output protection. Supported conventional masks cover 2.1, 3.0, 3.1, quad, 4.0, 5.0, 5.1, 7.0 and 7.1. A recognized count with an unrecognized mask is still refused.

**Downmix** means combining channels, not simply discarding every channel except left/right. Center contributes to both sides; surrounding channels contribute to their relevant side. Ordinary center/surround coefficients are −3 dB (about 0.707), inspired by ITU-R BS.775. In 7.1 the side/rear pairs each use −6 dB before final scaling. Each stereo row is conservatively scaled so coherent full-scale contributors cannot overload the resulting sum.

For a 5.1 illustration, left receives front-left + 0.707 center + 0.707 left-surround, then the conservative row scale; right is analogous. Actual matrices depend on the known speaker mask. This can lower the overall level while retaining content from those channels. It is not a universal theatrical mastering recipe.

## LFE is omitted

The `.1` LFE channel is **low-frequency effects**, not all bass from the music. Aède omits it in the current stereo downmix. Bass already in the main channels remains. There is no configurable subwoofer crossover, bass routing or per-channel delay/trim in this version. Those require a separate output and processing design.

```sh
aede play /path/to/known-5.1-file.flac --normalize off
```

No downmix CLI flag is required: the local player chooses the supported stereo path. Turning normalization off does not turn mapping off. If layout is unknown/unsupported, expect a clear error instead of guessed audio placement.

## Decoder and output limits

FFmpeg fallback decoding currently cannot supply a trustworthy speaker mask above stereo, so such multichannel fallback cannot be downmixed by Aède; mono/stereo remain supported. Source format/layout is retained for diagnostics even when playback format becomes stereo. The [native PCM route](../server/playback.md) uses the same mono/stereo path and known stereo downmix; original multichannel PCM passthrough is not implemented. [Subsonic/OpenSubsonic](../server/subsonic.md) transfers the original encoded file, leaving decoding and channel mapping to its client.

The downmix is a zero-processing-latency matrix with no per-block allocation; it does not account for host/device buffering. The interactive model illustrates known channel paths and omitted LFE. It does not auto-detect your speakers or send multichannel sound.

See [Output selection](output.md) and [future channel/bass-management ideas](premium.md).
