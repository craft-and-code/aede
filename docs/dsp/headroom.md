# Headroom, clipping and the final guard

In floating-point PCM, nominal full scale is −1 to +1. **Headroom** is the remaining margin before that range is exceeded. A signal boosted too far can **clip** when sent to a bounded output, flattening peaks and causing distortion. Aède makes the policy explicit rather than silently pretending that every requested gain is safe.

## First, cap normalization gain

Aède uses the declared ReplayGain peak, a valid measured true peak, or a conservative full-scale assumption if neither is available. It limits the requested gain so that this supplied peak should not exceed 1.0. For a peak of 0.5, there is about +6 dB available; if normalization asks for +9 dB, it is capped near +6. With unknown peak assumed 1.0, positive normalization boost is avoided.

This is a **static** decision based on available information. A tag can be wrong; a sample peak can underestimate an inter-sample peak. The cap does not prove true-peak safety. Requested −18 LUFS can therefore remain unattained without this being an error.

## Then reserve tone headroom

Positive bass/treble boosts reserve their summed gain as preamp attenuation. +6/+3 dB reserves 9 dB; cuts alone reserve zero. This margin is distinct from normalization's cap. It reduces overload risk without dynamically changing music's loud/quiet contrast. See [Tone](tone.md).

## Finally, protect submitted samples

After processing, the final guard hard-clamps any unexpected sample outside `[−1,+1]` and counts changed samples. In-range values are untouched. Floating intermediate DSP can retain headroom; this guard is the explicit last sample ceiling before native output or ffplay. Nonfinite samples, incomplete frames and gains that would overflow are rejected rather than sent as valid audio.

A changed-sample count above zero means the guard actually intervened; it is not the amount of gain reduction in dB. The CLI also reports the pre-guard peak, making overload visible. Try flatter tone settings and check source/normalization facts before increasing gain further.

```sh
aede play "An Album" --normalize off --bass 0 --treble 0
```

This removes optional gain/EQ controls for comparison while retaining output protection and any required format conversion. It is not an unsafe bypass and does not establish a bit-perfect path.

## Guard versus limiter

A look-ahead **true-peak limiter** would analyze forthcoming waveform peaks and smoothly reduce gain under a chosen ceiling, with latency and a gain-reduction meter. Aède does not have that processor today. Its hard sample guard cannot guarantee a reconstructed true-peak ceiling, and dynamic gain reduction is explicitly unavailable. A true-peak reading can exceed full scale even when no stored sample exceeds it.

Output meters observe submitted guarded PCM, before dither/device conversion. True peak is unknown at 192 kHz and above. [Measurements](measurements.md) and [Output](output.md) explain these limits; a [future Premium limiter](premium.md) remains a proposal.
