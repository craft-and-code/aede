# Native DSD fixtures

`tone.dsf` and `tone.dff` contain the same mono 1 kHz sine, amplitude 0.35, sampled by an elementary first-order one-bit sigma-delta encoder at 2,822,400 Hz for 65,536 source bits. They are synthetic project fixtures, free of third-party recordings. DSF stores LSB-first channel blocks of 4,096 bytes; DFF stores MSB-first interleaved bytes. Headers follow Sony DSF 1.01 and Philips DSDIFF 1.5.

`stereo128.dsf` and `stereo128.dff` use the same method at 5,644,800 Hz, with two independent channels: 1 kHz left and 2.7 kHz right. Each channel has 65,536 source bits at amplitude 0.35. Both containers represent exactly 8,192 measurement PCM frames at 705,600 Hz or 1,024 listening PCM frames at 88,200 Hz. Their different channel content checks ordering and prevents a duplicated mono channel from passing as stereo.

To reproduce the bitstream, initialize `error = 0`, and for each bit index `i`: `x = 0.35 * sin(2*pi*1000*i/2822400)`; select `q = +1` if `x + error >= 0`, else `-1`; update `error += x - q`; store one for positive `q`. The test-only generators in `src/dsd_tests.rs` document both complete container layouts and construct independent duration, rate and channel cases. No FFmpeg encoder or decoder is required for these fixtures.

## Native Opus fixtures

`generate-opus.py` generates original PCM16 tones and invokes a reference FFmpeg/libopus encoder. The decoder tests themselves need neither executable. All sources use 48 kHz and amplitude 6500/32768. `opus-mono.opus` has 12,037 playable frames at 1 kHz; `opus-stereo.opus` has 48,013 with 1 kHz left and 2.7 kHz right; `opus-short.opus` has 121 mono frames. They exercise non-packet-aligned endings, a single audio page and stereo order. `opus-surround.opus` contains 2,401 frames in six distinct-tone channels; it now verifies native multistream admission and keeps its original encoded data. No copyrighted recording is included; fixtures are covered by the project license. Different reference encoder versions need not produce identical compressed bytes.

The [native ALAC corpus](native-alac/README.md) adds original ALAC16 mono/44.1 kHz and ALAC24 stereo/96 kHz integer ramps, including active low bits. Its generator uses FFmpeg only as an independent fixture encoder; native tests check every decoded sample, codec properties and exact duration without an external decoder.

`generate-opus-multistream.py` adds explicit RFC 7845 family-one streams for each channel count from one through eight, all at 48 kHz with exactly 12,037 playable frames. Each canonical speaker has its own original tone at `700 + 500*c` Hz; LFE (canonical channel 3 for six or more channels) instead uses 80 Hz. Native tests compare the known tones with each decoded speaker and validate masks, framing, pre-skip/gain, duplicate/silent mappings, reset, truncation and cancellation without running a codec executable.

- `opus-family1-1.opus` SHA-256: `8e79892fc77d0e385d52d9695b73284a594e844526d2065a03ae6de9743b92f8`
- `opus-family1-2.opus` SHA-256: `228869731e73d3322f315969a4b9c057f74201ae50fcd5ff0792a855b733cb21`
- `opus-family1-3.opus` SHA-256: `a1d3ca12acf29ed028a15cfafe017c2da696f53e1442b591a66d874b256fbcf1`
- `opus-family1-4.opus` SHA-256: `fea1f55b74f746a7b32af4b563a775e7f2146937647dd02640978f72a253751e`
- `opus-family1-5.opus` SHA-256: `71b35dbc2fea7255894de7e11a4e7b334eb54709e59361007eec3a8a5fa8829b`
- `opus-family1-6.opus` SHA-256: `09219ce0a150457ac247970ab0210bfae96d63f6e0066c16f524945c2b06be7e`
- `opus-family1-7.opus` SHA-256: `b7e5a2539e6c211842d4063f5b26d99334691d6ce707f607060e133f99fb7ad7`
- `opus-family1-8.opus` SHA-256: `4c095766ef25add8ca2e7be61c90cf1c2917b2de5dc1c2d65013c5e104ed89e6`
