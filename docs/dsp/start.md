# Understand Aède's audio processing

DSP means **digital signal processing**: calculations on the sound during playback. Aède's current DSP changes the playback stream, never the original file or its tags. It does not replace the system volume control. The local `aede play` command and the authenticated native PCM route use shared processing.

## Follow a sample from file to output

1. The decoder turns compressed/file audio into PCM: a sequence of numerical samples. Each frame contains one sample per channel; stereo has left and right together.
2. Known multichannel layouts can be mapped to stereo, with LFE omitted and conservative scaling.
3. The native output chooses a supported sample format/rate; when needed, a stateful bandlimited converter changes the rate.
4. A fixed track/album normalization decision changes level, subject to a peak-aware cap.
5. Optional broad bass/treble shelves apply their tone correction, with additional headroom for boosts.
6. A final sample guard clamps unexpected values outside full scale and reports its interventions.
7. Meters observe the submitted PCM; the selected sink sends it to the device. Native integer output adds TPDF dither while quantizing; floating output bypasses that conversion.

The order above describes local playback. Missing-loudness capture watches **original source PCM before processing**, so later normalization does not measure its own gain/EQ as if it were the source. Output metering observes guarded submitted PCM **before** integer dither/device conversion. It does not measure your speakers.

The [native PCM route](../server/playback.md) also decodes, maps known channels to stereo, applies rate conversion when needed, and uses normalization, tone/headroom and output protection. Its client owns the audio device and final device conversion. This one-track route reuses existing loudness data without learning new measurements or joining successive remote tracks. [Subsonic/OpenSubsonic](../server/subsonic.md) transfers original encoded audio without server DSP; its client owns decoding and any playback processing.

## Your current controls

```sh
aede play "An Album"
aede play "An Album" --normalize album --bass 3 --treble -2
aede play /path/to/song.flac --normalize off --bass 0 --treble 0
```

Replace example names/paths with yours. Normalization defaults to album scope for catalogued album selections and track scope for files, folders, playlists, collections, artists and tracks. `--normalize off|track|album` overrides it. Bass/treble each accept −12 to +12 dB, default zero. Zero bypasses the shelves exactly; normalize off alone does **not** disable tone processing or the output guard.

The CLI does not expose a target-LUFS knob, parametric filter editor, limiter, convolution profile, manual resampling rate or dither toggle. Those are not hidden flags to guess. The [play command reference](../cli/play.md) covers selections and options; read [Output and diagnostics](output.md) to identify what actually ran.

## Start with the question you have

| Question | Guide |
| --- | --- |
| Why do two albums play at different levels? | [Normalization](normalization.md) and [measurements](measurements.md). |
| How can I adjust bass or treble? | [Tone controls](tone.md) and [headroom](headroom.md). |
| What happens to 5.1 audio? | [Channel mapping](channels.md). |
| Why did output change from 96 to 48 kHz? | [Rate conversion](resampling.md). |
| Does Aède send untouched integer samples? | [Dither and integer output](dither.md). |
| Are album joins gapless? | [Continuous playback](continuity.md). |
| What do the moving bars mean? | [Spectrum display](spectrum.md). |
| Which effects might Premium add? | [Future Premium](premium.md). |

The website's interactive diagrams use synthetic signals to explain these ideas. They are not device measurements or audio-processing controls. The technical implementation/validation is recorded in the [DSP review](../coding/dsp-review.md) and [playback design](../design/playback.md).
