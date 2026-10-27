# Native DSD fixtures

`tone.dsf` and `tone.dff` contain the same mono 1 kHz sine, amplitude 0.35, sampled by an elementary first-order one-bit sigma-delta encoder at 2,822,400 Hz for 65,536 source bits. They are synthetic project fixtures, free of third-party recordings. DSF stores LSB-first channel blocks of 4,096 bytes; DFF stores MSB-first interleaved bytes. Headers follow Sony DSF 1.01 and Philips DSDIFF 1.5.

`stereo128.dsf` and `stereo128.dff` use the same method at 5,644,800 Hz, with two independent channels: 1 kHz left and 2.7 kHz right. Each channel has 65,536 source bits at amplitude 0.35. Both containers represent exactly 8,192 measurement PCM frames at 705,600 Hz or 1,024 listening PCM frames at 88,200 Hz. Their different channel content checks ordering and prevents a duplicated mono channel from passing as stereo.

To reproduce the bitstream, initialize `error = 0`, and for each bit index `i`: `x = 0.35 * sin(2*pi*1000*i/2822400)`; select `q = +1` if `x + error >= 0`, else `-1`; update `error += x - q`; store one for positive `q`. The test-only generators in `src/dsd_tests.rs` document both complete container layouts and construct independent duration, rate and channel cases. No FFmpeg encoder or decoder is required for these fixtures.

## Native Opus fixtures

`generate-opus.py` generates original PCM16 tones and invokes a reference FFmpeg/libopus encoder. The decoder tests themselves need neither executable. All sources use 48 kHz and amplitude 6500/32768. `opus-mono.opus` has 12,037 playable frames at 1 kHz; `opus-stereo.opus` has 48,013 with 1 kHz left and 2.7 kHz right; `opus-short.opus` has 121 mono frames. They exercise non-packet-aligned endings, a single audio page and stereo order. `opus-surround.opus` contains 2,401 frames in six distinct-tone channels; it verifies the explicit native-profile refusal and retained Aède compatibility. No copyrighted recording is included; fixtures are covered by the project license. Different reference encoder versions need not produce identical compressed bytes.

The [native ALAC corpus](native-alac/README.md) adds original ALAC16 mono/44.1 kHz and ALAC24 stereo/96 kHz integer ramps, including active low bits. Its generator uses FFmpeg only as an independent fixture encoder; native tests check every decoded sample, codec properties and exact duration without an external decoder.
