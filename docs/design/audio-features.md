# Cooperative acoustic discovery

## Responsibilities and first measurement profile

Aède owns original-source decoding, source identity, persistence and task admission. The existing FlacCompagnon-backed native decoder and Aède's bandlimited resampler feed `aede-moods`; the latter owns bounded signal descriptors and exemplar distance, without a decoder, filesystem or network dependency. Existing forensic analysis records remain intact. Approved `rustfft` 6.4.1 and workspace `serde` are already locked; no new registry package/version is required.

Profile 1 accepts native mono/stereo PCM between 8 and 192 kHz, resampled to 22,050 Hz. Unsupported/external-decoder-only sources remain explicitly unavailable. Fixed Hann windows measure centroid, 85-percent rolloff, flatness, zero crossings and a twelve-bin chroma profile. An onset autocorrelation provides optional tempo/periodicity; silence, insufficient evidence and ambiguous rhythm never imply a tempo of zero. RMS is a source measurement, not LUFS or emotional energy. Chroma does not establish a key. Model/profile changes invalidate older descriptors independently of the catalog format.

## Resource and foreground policy

One extraction worker is admitted per server. It checkpoints before source decode and transforms, waits while native audio/foreground transfers own the independent playback permits, and yields after a five-millisecond work quantum. Open paused native sessions conservatively retain priority until closed. WebKit radio/podcast and external playback can additionally renew a bounded authenticated foreground lease; leases expire after a stopped or disconnected client, rather than indefinitely starving work. The foreground path never waits on an analysis lock or performs analysis in an audio callback.

Source descriptors and FFT/scratch/onset state are bounded independently of duration. A 24-hour source-duration and 180-second active-computation budget refuse unusable inputs. Waiting at checkpoints does not consume the active budget. Decoder/OS reads already in progress cannot be interrupted universally, and cooperative scheduling does not certify physical audio, other platforms or arbitrary external system load. Target hardware resource measurements and audio transport acceptance are recorded separately.

## Persistence and privacy

Each complete input publishes one bounded atomic JSON record below the protected data directory, keyed by profile, canonical catalog path, original size and nanosecond mtime. Interrupted sources publish nothing; completed prior files remain reusable after process restart. Profile/source mismatch is missing evidence, not a fresh result. Malformed caches and unsafe links are reported rather than resetting source/personal data. No original audio, embedded tag, FlacCompagnon report or personal store is rewritten.

Acoustic descriptors are shared original-audio facts. Exemplar labels and resulting mood projections are derived exclusively from the current owner's rules and annotations. Another owner's tags, stars, notes and history never become training examples or shared models. Shared features cannot persist personal target names or rule membership.

## HTTP and client integration

An explicit trusted-loopback administrative task performs extraction, publishes per-file progress and supports the existing task Stop/polling contract. Client navigation does not terminate the server-owned job; shutdown/cancellation retains completed records. A fresh request resumes by reusing matching records. Ordinary mood reads may load bounded cached descriptors but never decode source audio or contact a provider. Remote HTTPS administrative exclusion remains intact.

Phémios presents coverage, real progress, foreground suspension, Stop and explicit restart of remaining work. Neighbor selection must retain exact personal labels, use at least two usable specific examples before acoustic expansion, rank finite weighted distances and explain the evidence separately from a measured emotion. Genre-only recipes remain transparent fallback where a model lacks examples; mastering loudness does not substitute for tempo/timbre or emotional classification.
