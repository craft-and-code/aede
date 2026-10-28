# Cooperative acoustic discovery

## Responsibilities and first measurement profile

Aède owns original-source decoding, source identity, persistence and task admission. The existing FlacCompagnon-backed native decoder and Aède's bandlimited resampler feed `aede-moods`; the latter owns bounded signal descriptors and exemplar distance, without a decoder, filesystem or network dependency. Existing forensic analysis records remain intact. Approved `rustfft` 6.4.1 and workspace `serde` are already locked; no new registry package/version is required.

Profile 1 accepts native mono/stereo PCM between 8 and 192 kHz, resampled to 22,050 Hz. Unsupported/external-decoder-only sources remain explicitly unavailable. Fixed Hann windows measure centroid, 85-percent rolloff, flatness, zero crossings and a twelve-bin chroma profile. An onset autocorrelation provides optional tempo/periodicity; silence, insufficient evidence and ambiguous rhythm never imply a tempo of zero. RMS is a source measurement, not LUFS or emotional energy. Chroma does not establish a key. Model/profile changes invalidate older descriptors independently of the catalog format.

## Resource and foreground policy

One extraction worker is admitted per server. It checkpoints before source decode and transforms, waits while native audio/foreground transfers own the independent playback permits, and yields after a five-millisecond work quantum. Open paused native sessions conservatively retain priority until closed. WebKit radio/podcast and external playback can additionally renew a bounded authenticated foreground lease; leases expire after a stopped or disconnected client, rather than indefinitely starving work. The foreground path never waits on an analysis lock or performs analysis in an audio callback.

Source descriptors and FFT/scratch/onset state are bounded independently of duration. A 24-hour source-duration and 180-second active-computation budget refuse unusable inputs. Waiting at checkpoints does not consume the active budget. Decoder/OS reads already in progress cannot be interrupted universally, and cooperative scheduling does not certify physical audio, other platforms or arbitrary external system load. Target hardware resource measurements and audio transport acceptance are recorded separately.

## Persistence and privacy

Each complete input publishes one bounded atomic JSON record below the protected data directory, keyed by profile, decoder provenance, canonical catalog path, original size and nanosecond mtime. Interrupted sources publish nothing; completed current-generation files remain reusable after process restart. Profile/decoder/source mismatch is missing evidence, not a fresh result. Decoder corrections expire derived records without changing the descriptor definition or deleting old artifacts. Malformed caches and unsafe links are reported rather than resetting source/personal data. No original audio, embedded tag, FlacCompagnon report or personal store is rewritten.

A legacy catalogue row without scanned nanosecond precision cannot match a known zero fraction. Cache warming and mood projections leave that source missing; extraction reports it unavailable with a rescan instruction while continuing other sources. Completed cache files are retained. Source validation before cache reuse and after extraction requires the same exact, known scan stamp.

Acoustic descriptors are shared original-audio facts. Exemplar labels and resulting mood projections are derived exclusively from the current owner's rules and annotations. Another owner's tags, stars, notes and history never become training examples or shared models. Shared features cannot persist personal target names or rule membership.

Exemplar vectors and models are rebuilt for each projection from current descriptors; they have no separate persisted training store. A decoder revision therefore leaves personal labels intact while withholding stale vectors until their source descriptors are recomputed.

Cache warming uses the installation's shared catalogue, including when requested through a supervised account. A filtered view must neither replace the shared descriptors with its subset nor invalidate other accounts' unchanged paging evidence. The account's own catalogue projection still limits descriptor coverage, examples and returned albums; retaining the shared graph for cache loading does not widen those responses.

## HTTP and client integration

An explicit administrative task performs extraction, publishes per-file progress and supports the existing task Stop/polling contract. HTTPS requires a current named administrator session; local HTTP retains its administrative authentication. Client navigation does not terminate the server-owned job; shutdown/cancellation retains completed records. A fresh request resumes by reusing matching records. Ordinary mood reads may load bounded cached descriptors but never decode source audio or contact a provider.

Phémios presents coverage, real progress, foreground suspension, Stop and explicit restart of remaining work. Neighbor selection must retain exact personal labels, use at least two usable specific examples before acoustic expansion, rank finite weighted distances and explain the evidence separately from a measured emotion. Genre-only recipes remain transparent fallback where a model lacks examples; mastering loudness does not substitute for tempo/timbre or emotional classification.


## Catalogue lookup and allocation checkpoint

Mood evidence retains the validated graph file ID. Descriptor lookup therefore uses the existing constant-time catalog file access and bounded cache map, rather than resolving a track path with a linear catalog scan for each recipe/candidate. Only requested page rows allocate explanation JSON; counts and candidate ordering retain compact typed reasons. The worker uses the scanned ID as its normal source-validation fast path and falls back to path lookup only after catalog reindexing. No new global index, source mutation or private storage is introduced.

A 2,048-source synthetic regression preserves the measured/proximity counts and refuses two placements of the same source as two training examples. Illustrative same-host unoptimized test runs take approximately 175 ms before the lookup/allocation change and 13 ms afterward. These individual runs are not a production benchmark or physical audio acceptance; the structural removal of repeated linear resolution and unused JSON is independent of timing.
