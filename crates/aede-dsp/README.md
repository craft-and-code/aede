# Aède DSP

`aede-dsp` processes decoded audio samples. It has no decoder, audio device, terminal interface, network access, or dependency on the catalog. The first stage accepts finite, interleaved `f32` PCM and applies one gain to every channel of a frame. Gain can change smoothly across calls, so a future gapless player can keep one processing stream across track boundaries.

The crate measures sample peaks and counts samples above full scale. It does not clip them: floating-point PCM retains headroom for later stages, and the eventual output adapter must make an explicit conversion decision. It also does not control system volume. `aede-core::playback::normalization` now chooses a gain from ReplayGain or Opus R128 tags; this crate applies that choice without knowing its source. Decoded loudness measurement remains future work.

The processing call allocates no memory. Invalid samples, incomplete frames, and gains that would overflow `f32` are rejected before the buffer or gain ramp changes. The sample rate and channel count are fixed per `Dsp` instance; create a new one on a format change.

This establishes the sample contract and gain stage. `aede-core::playback::decoder` supplies progressive PCM for supported local files, and `aede-core::playback::Queue` holds the playing order. The `aede play <audio-file>` command now connects one file to this DSP and ffplay for local audio output. It uses unity gain; applying selected ReplayGain or R128 values remains future work. Queue playback and gapless transitions also remain future work. Equalization, filtering, spatial processing, and other optional effects can be added as separate processing stages when their sound and controls are specified.
