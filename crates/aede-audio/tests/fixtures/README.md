# Native DSD fixtures

`tone.dsf` and `tone.dff` contain the same mono 1 kHz sine, amplitude 0.35, sampled by an elementary first-order one-bit sigma-delta encoder at 2,822,400 Hz for 65,536 source bits. They are synthetic project fixtures, free of third-party recordings. DSF stores LSB-first channel blocks of 4,096 bytes; DFF stores MSB-first interleaved bytes. Headers follow Sony DSF 1.01 and Philips DSDIFF 1.5.

`stereo128.dsf` and `stereo128.dff` use the same method at 5,644,800 Hz, with two independent channels: 1 kHz left and 2.7 kHz right. Each channel has 65,536 source bits at amplitude 0.35. Both containers represent exactly 8,192 measurement PCM frames at 705,600 Hz or 1,024 listening PCM frames at 88,200 Hz. Their different channel content checks ordering and prevents a duplicated mono channel from passing as stereo.

To reproduce the bitstream, initialize `error = 0`, and for each bit index `i`: `x = 0.35 * sin(2*pi*1000*i/2822400)`; select `q = +1` if `x + error >= 0`, else `-1`; update `error += x - q`; store one for positive `q`. The test-only generators in `src/dsd_tests.rs` document both complete container layouts and construct independent duration, rate and channel cases. No FFmpeg encoder or decoder is required for these fixtures.
