# Native ALAC references

These original integer ramps contain no third-party recording. `generate.py` creates 16-bit mono at 44.1 kHz (10,001 frames) and 24-bit stereo at 96 kHz (48,013 frames), with distinct channel values and active least-significant bits. FFmpeg is a fixture encoder only; tests decode these files without invoking it. M4A uses fast-start metadata so truncated audio can retain its declared duration. Expected sample `i`, channel `c` is `((i * 104729 + c * 49999) % 2^bits) - 2^(bits-1)`.

- `mono16.m4a` SHA-256: `531881b2124c655f2fe6b2fd2549c972d3ca1755aa898dd01191bcbadb09566b`
- `stereo24.m4a` SHA-256: `d2a93145f810e872456271ab0e91d47c17a32c65272f8eba2cc31b68caf53429`

`stereo24-clock192k.m4a` has exactly the same encoded audio as `stereo24.m4a`, with the media timescale, sample-duration ticks and edit media positions doubled. Its 96,026 ticks at 192 kHz still describe 48,013 PCM frames at 96 kHz; the native reader must preserve the original samples and analysis must not mistake ticks for PCM frames. The generator performs this container-only transformation.

- `stereo24-clock192k.m4a` SHA-256: `d76a4739ac2c9a2c77c3210ac0e03843c8ee39c440ffdfa275f8cbb51f7c9f97`

`surround16.m4a` is a 513-frame 16-bit six-channel/48 kHz reference for metadata and retained compatibility only. It does not extend the initial native mono/stereo admission.

`stereo24-clock48k.m4a` retains the same encoded stereo24 packets, with the media timescale halved and media durations/edit positions rounded up to whole ticks. Its 24,007 ticks at 48 kHz describe 48,013 PCM frames at 96 kHz: the rounded clock is not an exact PCM frame bound. Packet coverage still has to reach the declared media duration. The missing-tail tests remove the final packet and repair the `mdat` extent, while retaining the original packet table and declared duration.

- `stereo24-clock48k.m4a` SHA-256: `af2b63036d05b9802ed5ed11e0253eadd3c6884c5dfe92f4f88bfe3bd020f8af`

`generate_layouts.py` adds original PCM16/48 kHz ramps with 10,001 frames and explicit Apple `chan` cookies for 3.0, 4.0, 5.0, 5.1, 6.1 and wide 7.1. Native tests assert every sample and the actual speaker mask. The generator also checks independently decoded FFmpeg PCM; its rear-speaker order for 6.1(back)/7.1(wide) is explicitly reconciled with the side-speaker mask used by the source and native reader. These fixtures qualify explicit layouts, not unspecified discrete channels. The original `surround16.m4a` remains a refusal reference.

- `layout3-16.m4a` SHA-256: `339c93fbe2fb7531cf0cfdae0935df2f95f9c01ce6bc8c315f5d7c26ad8f3b43`
- `layout4-16.m4a` SHA-256: `3c48e62a574bc5b9b1d2dd4ea6c18024b4a6750ec2a3175f9880de8f0f1e7d34`
- `layout5-16.m4a` SHA-256: `2d24b12f6c4be845878bc86fd1aaf2b0913c855356b5eefe442f42bc1c7866af`
- `surround16-layout.m4a` SHA-256: `b5414b090853354926c8c54b0aaacd24bcbb76df6466c682e4f8a3b49f0496bc`
- `layout7-16.m4a` SHA-256: `fcffd8954a3bfdf1638eba9652939a3463fcd00c7adad0f3796b3387972c9aea`
- `layout8-16.m4a` SHA-256: `fbd57d8304cf3ec2f0ce2c409b8c8923f3fa39faf88375c9116e8e06ee66c437`
