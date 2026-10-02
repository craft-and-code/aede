# Integer output and TPDF dither

The processing stream is floating-point `f32` PCM. A native device can accept floating samples directly or require integers with a finite number of representable levels. **Quantization** rounds a value to one of those levels. At very low levels, deterministic rounding error can correlate with the signal and create distortion.

## What Aède does

The native backend prefers compatible floating formats. That path bypasses integer quantization and dither. If the selected supported output is integer-only, the callback converts guarded PCM using **TPDF dither**: two independent uniform random draws form triangular-distribution noise in least-significant-bit units before rounding. The small noise decorrelates quantization error; it does not add resolution to the source or repair mastering.

Signed/unsigned 8-, 16-, 24- and 32-bit native formats are supported by this converter. After the earlier sample guard, quantization bounds the result to the chosen integer range. Dither's state continues across blocks and natural tracks sharing the same output stream. Queue-shortage silence stays exact digital silence rather than having noise added to it.

```sh
aede play /path/to/song.flac
```

There is no current CLI bit-depth/dither setting. Read the output diagnostics to know which format was negotiated. Forcing `AEDE_AUDIO_BACKEND=native` requires a usable native route rather than falling back, but does not choose a specific device bit depth.

## What the meters and claims mean

Aède's output meters observe guarded PCM **before** dither and device conversion. Their peak readings therefore do not include that final rounding/noise, host mixer behavior or DAC reconstruction. The ffplay fallback receives `f32le`; its later integer/device conversion is outside Aède.

The native integer path is not claimed **bit-perfect**: audio has been decoded to `f32`, and integer conversion intentionally dithers. Normalization, EQ, downmix or resampling also change samples. A flat tone setting alone cannot prove untouched delivery through an operating system/device.

The interactive model intentionally uses exaggerated low bit depths/noise so rounding is visible. The real dither operates in the selected format's least-significant-bit scale; a visible staircase in the diagram is not a prediction of audible artifacts from your playback.

See [Output](output.md) and [Headroom](headroom.md) for the preceding safety stages.
