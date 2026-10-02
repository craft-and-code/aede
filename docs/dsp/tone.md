# Broad bass and treble controls

Aède has two optional **shelf filters**. A shelf raises or lowers a broad low/high-frequency region, unlike a narrow parametric bell filter. Bass primarily affects the low end; treble the upper end. They are listening adjustments, not automatic room correction.

## Set and reset

```sh
aede play "An Album" --bass 3 --treble -2
aede play "An Album" --bass -6 --treble 0
aede play "An Album" --bass 0 --treble 0
```

Each value is a finite number from −12 to +12 dB; both default to zero. Negative values are accepted as separate arguments or with `--bass=-6`. Positive raises that region, negative lowers it. Zero removes the corresponding filter; both zero are an **exact flat filter bypass**. This bypass concerns EQ only: normalization, channel/rate conversion and output protection can still run.

The ordinary shelf midpoints are **120 Hz** for bass and **4 kHz** for treble. A midpoint is the transition region, not a brick-wall cutoff. At unusually low sample rates these midpoints are reduced to remain below Nyquist (half the sample rate). Frequencies/slope/Q are not user-configurable in this version. Coefficients follow the Audio EQ Cookbook shelves through the `biquad` implementation; state is separate for each channel.

## Why boost also reduces the preamp

Boost needs space before full scale. Aède conservatively reserves the sum of positive boosts: bass +6 dB and treble +3 dB reserve **9 dB** of preamp attenuation. Bass +3 and treble −2 reserve 3 dB. Cuts do not return a positive boost allowance. This reserve is additional to normalization's peak-aware cap, not a limiter constantly chasing peaks.

That can make the overall programme quieter while its tonal balance changes. Raise your system volume if needed; do not assume missing overall loudness means the bass filter failed. Filter transients or wrong peak metadata can still overrun a static estimate, so the final sample guard remains active and reports interventions. See [Headroom](headroom.md).

## Continuous processing

Shelf state survives compatible natural joins, so an album boundary does not simply reset a filter. A new/incompatible stream or interruption resets processing. Replacing controls clears filter state and is configured between streams; the CLI currently selects controls at command startup rather than providing a live filter editor with smooth parameter changes.

The animated curve shows broad shelf behavior and the reserve on a synthetic example. It is a teaching model, not a calibrated measurement of your room, headphone or the Rust filter at every frequency.

More detailed parametric EQ, portable device profiles and convolution are [Premium proposals](premium.md), not present tone options.
