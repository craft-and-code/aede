# Read the 24-band spectrum

The moving bars in `aede play` are a **frequency spectrum display**, not an equalizer, loudness reading or integrity check. Low frequencies appear on the left, higher ones on the right. The display observes samples; it does not change the audio sent to output.

```sh
aede play /path/to/song.flac
```

## How the display is built

The analyzer gathers 2048 frames, applies a Hann window to reduce frequency-edge artifacts, and uses a real-input FFT to estimate frequency components. It groups peaks into **24 logarithmically spaced bands**, from approximately 45 Hz to the smaller of 16 kHz and Nyquist. Log spacing gives low-frequency regions usable space instead of letting upper frequencies occupy most of the drawing.

Mono is observed directly. For multi-channel display, a weighted display mix gives the first channel 75% and the others an averaged 25%, preventing exactly opposite-phase stereo from disappearing completely in the bars. That visualization mix is not the audio downmix. The analyzer only reads PCM, so displaying a spectrum does not alter playback channels.

Levels are smoothed with a quicker rise and slower fall, then mapped to display values from 0 to 1. They are not calibrated LUFS, true peak or linear energy measurements. A loud cymbal and a bass note can excite several bands. Empty/low bars do not prove absent high-resolution content or a damaged file.

## Terminal and metering limits

The CLI groups the 24 analysis bands into twelve broad, segmented Retro display bands, from low to high frequencies. Their widths follow the terminal width; narrow terminals combine bands further rather than wrapping. The columns fill from the bottom with green lower segments, yellow upper segments and red top segments. Separate peak markers fall more slowly after the current level drops. These colors indicate display height, not calibrated clipping thresholds.

Animation requires interactive terminal output. `NO_COLOR` or `--no-color` retains the blocks and peak markers in monochrome; `--lyrics` replaces the spectrum with lyric cues. Redirected output is not an audio animation feed. The labels show album and numbered filename, without the whole path. FFT snapshots are indexed by output frame and displayed only after the shared playback clock reaches them. Native output uses callback-consumed frames; ffplay uses the marked active-time estimate. At 48 kHz the 2048-frame analysis window spans about 43 ms, timestamp quantization adds at most 256 frames (about 5 ms), and redraws are spaced by 50 ms. Compatible joins preserve partial windows and peaks; seeking and output resets clear them. The 256-snapshot lookahead is bounded and drops excess visualization rather than delaying audio. The view approximates material consumed by the selected output; it does not measure the host mixer, DAC or room.

For sample/true peaks and guard intervention, read [output diagnostics](output.md). For an image of the file over time, the separate CLI `spectrum` command generates spectrogram files using the analysis workflow; it is different from the live Retro display. See the [spectrum command reference](../cli/spectrum.md) for its size/threads/full options.

The website's animated spectrum uses a synthetic example so band behavior remains visible without sending your audio or accessing the device. It does not analyze a file dropped into the documentation.
