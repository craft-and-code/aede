# Convert sample rate when the device needs it

The **sample rate** is the number of PCM frames per second, for example 44100 Hz (44.1 kHz). It is not the file's bitrate or the DAC's integer bit depth. Changing it requires reconstructing/filtering the signal, then sampling at new times; merely dropping or duplicating samples would introduce errors.

## Automatic native negotiation

The native player chooses a compatible device format/channel count and rate. Without-effects prefers the source rate before sample format; if that rate is unsupported, an adaptation may still be selected and reported. DSP retains floating-format preference before rate distance. Strict bit-perfect permits only the original rate and refuses an incompatible route instead of resampling. Read the policy and source/output rates; a source marked 96 kHz does not imply that an ordinary output receives 96 kHz.

```sh
aede play /path/to/96k-song.flac
```

There is no current CLI output-rate selector. If decoded and selected rates match, Aède bypasses its converter. Otherwise its decoding thread uses stateful **bandlimited** conversion before gain/tone and final protection. The ffplay fallback receives the decoded rate; any device-rate conversion performed by ffplay/system lies outside Aède's converter diagnostics.

## Why filtering matters

Nyquist is half the sample rate. Frequencies above the new Nyquist cannot be represented and can fold into false lower frequencies (**aliasing**). Downsampling therefore filters the high end before changing rate. Upsampling creates additional sample positions; it does not restore detail absent from the source or make a lossy recording lossless.

Aède uses Rubato FFT conversion for ordinary fixed rate pairs and sinc conversion for unusual ratios. Converter state is retained across blocks. Startup filter delay is removed; the tail is drained at the end of an uninterrupted stream. Compatible consecutive tracks share state and cumulative frame boundaries, so each file is not separately rounded/reset. Skips or incompatible formats discard the pending processing state.

## Validation and limits

Software signal tests cover documented rate/tone grids, with at most 0.1 dB passband error through 80% of the lower Nyquist, at least 80 dB residual/alias rejection and at most one frame residual phase delay. The high-frequency transition beyond that passband is characterized separately. These values are test coverage, not a promise for every signal, DAC, latency or NAS CPU. The [DSP review](../coding/dsp-review.md#reference-signal-validation) records the exact checks and limits.

The interactive example illustrates sample positions and the need for a low-pass filter; it is not a Rust-converter benchmark. Device buffering and physical joins still need hardware measurement; see [Continuity](continuity.md).
