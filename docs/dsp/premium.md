# DSP Premium: future directions

This is a product/research proposal, **not implemented Premium commands or an approved release contract**. Pricing, entitlement, dates and final feature scope remain undecided. The current processing is documented in the other DSP pages and remains distinct from these ideas.

## First priorities under consideration

| Proposal | What it would allow | Work needed before delivery |
| --- | --- | --- |
| Parametric equalizer | Several bell/shelf/high-pass/low-pass filters with frequency, gain, Q and bypass. | Stable filters, safe headroom, presets and smooth changes without clicks. |
| Per-device profiles | Separate headphone/speaker/output settings with import/export and clear active profile. | Profile validation, format/version rules and safe switching. |
| Convolution with user impulses | Apply an FIR filter supplied for a headphone/room response. | Bounded latency, rate policy, safe transitions and response validation. This engine alone is not automatic room correction. |

**Q** describes how narrow a parametric filter is. An **impulse response** is a filter's time-domain description; convolution applies it to the music. Such filters need measured/provenanced inputs rather than a claim that any downloaded curve improves sound.

## Other research ideas

Headphone-model EQ depends on measurement provenance, licensing and individual fit. Crossfeed would mix a delayed/filtered portion of each stereo side into the other for headphone listening. A true-peak limiter would add explicit look-ahead latency, ceiling and gain-reduction metering; it is different from the current hard sample guard.

Night-mode compression could reduce loud/quiet contrast for specific listening situations. Optional crossfade could blend unrelated tracks with configurable curves, while preserving continuous albums by default. Channel balance/trims/delay/polarity and bass-management crossovers require suitable output layouts. Measurement-assisted room correction needs calibrated microphones, several positions and acoustic expertise; it is not committed for the first release.

Dynamic EQ/loudness compensation, mid/side width, time stretch/pitch and binaural/spatial rendering have additional perceptual/CPU/data requirements. They remain research candidates. No current CLI/API option activates them, and the future server transport is a separate prerequisite rather than a Premium DSP effect.

## How future features should be validated

Keep an explicit bypass; never modify files/tags. Measure response, stability, aliasing, sample/true peaks, loudness scopes, processing joins and block-size invariance. Show actual stage/latency/profile status. Listening preference and fidelity claims require listening validation, not only attractive curves. Hardware joins and NAS budgets still require target testing even for the present foundation.

The [DSP product proposal](../design/dsp-product-proposal.md) records the detailed research, proposed priorities and source library. It is the reference for future updates; percentages there are planning estimates, not measured quality or availability guarantees.
