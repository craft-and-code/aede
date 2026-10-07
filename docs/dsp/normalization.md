# Loudness normalization: track or album

Normalization reduces level jumps between recordings by applying a constant gain during playback. **Gain** is amplification/attenuation, measured in dB: positive is louder, negative quieter. It does not change the system volume or compress quiet/loud passages within a song.

## Choose a scope

```sh
aede play "An Album" --normalize album
aede play /path/to/playlist.m3u --normalize track
aede play /path/to/song.flac --normalize off
```

Album mode preserves the programme's relative quiet/loud passages and tracks; track mode aims to align separate tracks' average loudness. Local playback defaults to without-effects and normalization off. With `--playback=dsp`, a catalogued album defaults to album normalization and other selections to track, including a folder that happens to contain an album. Explicit `--normalize album` selects album semantics rather than guessing from the path and implies DSP when no playback policy was supplied. Active track/album normalization conflicts with explicit without-effects or bit-perfect. Off skips normalization gain; it does not disable selected EQ, device conversion or ordinary output protection.

The current target is **−18 LUFS**, a loudness reference, not a peak ceiling. If a measured track is −23 LUFS, the requested gain is +5 dB; a −13 LUFS track requests −5 dB. [Headroom](headroom.md) can reduce a positive request, so achieving exactly −18 LUFS is not guaranteed. LUFS uses a perceptually weighted programme measurement; it cannot predict your room's sound-pressure level.

## Tags and reference levels

ReplayGain stores track/album gain and sometimes corresponding peaks. Its nominal reference is −18 LUFS. Opus R128 gains use a −23 LUFS reference and fixed-point values; Aède adjusts that reference toward its −18 target. The Opus decoder's mandatory header output gain is separate and applied before R128 normalization. On Opus, R128 is preferred over ReplayGain when both exist.

The metadata selector first looks for the requested scope, then the other scope only if absent. The playback policy can use a fresh measured value before that fallback in track mode; see [Measurements](measurements.md). A malformed/conflicting selected tag is an error, not an excuse to silently pick a different value. Original tags are never rewritten.

## When information is missing

The player starts without decoding the whole selection first. It uses already available requested-scope tags or current measurements. Track mode can reuse FlacCompagnon analysis and a fresh cache; missing source loudness is measured while the track plays for a **later** listen. Gain stays fixed while that track plays, so it does not suddenly jump halfway through.

Album mode needs complete album-scope tags or a cached measurement of the complete ordered programme. Aède never averages track LUFS to invent album LUFS. Without ready album data, the session keeps one unchanged level and can learn the full uninterrupted programme for later playback. Skipping, stopping, decoding failure or file changes prevent an incomplete capture from being saved as a complete album.

Playback cache reuse and publication compare file size and modification time including its nanosecond fraction. A same-size change within one second invalidates a derived measurement. Older derived caches without that precision expire and may be learned again during a complete later listen; imported FlacCompagnon reports retain their original, whole-second source evidence. Native remote PCM reuses ready values but does not capture missing loudness; the local player can learn it. Neither path rewrites audio or tags.

A quiet track can therefore remain quiet on first play, or never reach the target because there is not enough peak headroom. Check the reported normalization source/gain/headroom before assuming a bug. For tone-independent listening, also use `--bass 0 --treble 0`.

The interactive example changes synthetic levels to demonstrate constant gain and peak reserves; it does not control your playback.
