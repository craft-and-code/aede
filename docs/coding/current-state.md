# Aède — Current State

This file is the short-term memory of the project.

It describes the state of the repository at the beginning of a development session. It is intentionally concise.

Do not turn this file into a development diary.

---

## Project status

**Current milestone:** M2 — API and persistence separation

**Previous milestone:** M0.6 — Catalog and local library

**Status:** M1 is done. M2 persistence separation and the HTTP/JSON/WebSocket catalog API in `aede-server` are implemented: the frozen `/api/v1/events` stream remains catalog-only, while `/api/v1/activity` reports task activity. CLI-shaped read routes and opt-in JSON scan/fetch jobs have a complete [route README](../../crates/aede-server/README.md); HTTP jobs support authenticated polling/cancellation, keep bounded in-memory results and survive client disconnection. The original bodyless scan remains synchronous.

Local HTTP remains loopback-only by default. Direct HTTPS accepts an explicit listener IP, certificate/private-key pair and exact public authority; it requires initialized accounts, including for a TLS listener on loopback. HTTPS leaves the catalog and personal API contract unchanged, but disables every `/api/admin` family and `AEDE_ADMIN_TOKEN`. Connection admission and remote request work are bounded, while target NAS capacity and browser interoperability still need validation. See the [operating guide](../operating.md) and [remote-access guide](../server/remote.md).

The native playback WebSocket accepts one current catalogued track for a user or administrator session. It streams shared-DSP, processed `f32le` PCM, requires cumulative client acknowledgements and writes private listening history from acknowledged playback. Up to four playback decoders run concurrently. This is a transport contract, not a browser/mobile UI or a device-output integration; real client, physical-playback and target-NAS validation remain open.

On Unix, store-changing CLI commands delegate to a running server over a private socket; all writers share a data-folder lock. Account commands run directly under that lock. Delegated scans and fetches can be stopped with `aede cancel <task-id>`; activity reports lifecycle phases with an unknown work total, while detailed output stays on the CLI or authenticated task result. M2's [storage baseline](m2-storage-benchmark.md) is recorded; SQLite remains deferred pending target NAS budgets and real-library validation. Windows drive/UNC/verbatim paths are corrected without rewriting stored paths; native Windows CI validates scanning, sidecars and copying. Local delegation remains Unix-only (see [Paths](../design/paths.md)). M0.5 is also done (query grammar covers relations, and the command options are shorthand for it). A representative manual verification pass was completed before M2; the owner has since confirmed AcoustID, Fanart.tv, successful lyrics and cover downloads in real use. The checks are recorded in `docs/coding/m1-manual-verification.md`.

Aède has completed the local catalog foundation and the external identification layer.

The [account foundation](../design/accounts.md) provides CLI and HTTP administration, admin/user/auditor roles, salted Argon2id credentials, bounded process-local bearer sessions, expiry/revocation and authenticated personal owners. The first administrator keeps `local`; name/status changes preserve opinions and history. Activating accounts protects catalog/notification reads; missing or unreadable credentials fail closed. Users may change only their own personal data; auditors may only read their own personal views and cannot manage accounts, jobs, credentials or playback. Personal operations recheck sessions under the writer lock. Version-3 backups include private accounts, preserve old envelopes and rotate the credential epoch on restore; reset retains accounts. Argon2 0.5.3 was explicitly approved, adding only blake2, password-hash and base64ct as new registry packages, without changing existing versions. The subsequent HTTPS addition uses the approved tokio-rustls 0.26.4, reusing existing locked Rustls dependencies without changing their versions. The registry audit passes for 235 locked versions with the existing accepted `paste` maintenance warning and unaudited Git dependency. CLI/server documentation is published in both languages. A browser login UI, browser/mobile playback client and Subsonic/OpenSubsonic compatibility remain future work.

M0, M0.5 and M0.6 have received a complete hardening pass. Scans preserve inaccessible paths, use precise file timestamps and offer a no-write JSON preview. Personal references retain optional identity evidence and support explicit, conflict-checked reattachment with undo. Copying uses exclusive temporary output, refuses destination links and filesystem aliases, verifies existing content on request and generates/remaps destination playlists. Lossless conversions preserve known PCM precision or refuse an incapable encoder. Query/JSON parsing, deterministic construction, empty machine-readable results, documentation and test isolation are covered by regressions. The new persisted evidence and timestamp fields remain optional for compatibility; imported analyses and playback caches still use their existing whole-second file identity.

The follow-up security and performance review adds isolated replacement writes for all four stores and named backups, protected atomic exports before personal-data mutations, literal terminal display and no-replace copy publication. Restores preflight their included destinations; reset preserves legacy inline conclusions before removing the catalog. Special scan/copy sources are refused; temporarily unavailable watched roots retain their entries while other roots advance. Malformed store tables and duplicate catalog paths are refused, imports preserve simultaneous listens, and playlists combine every contribution to a physical output folder. LRC reads and timestamp expansion are bounded; extreme LRC, MP4 and Ogg durations and malformed MP4 sizes cannot overflow. Statistics page their subsidiary lists consistently and saturate oversized aggregates. Query evaluation, personal reconciliation, scan retention and companion planning now avoid repeated full-table or directory passes; reproducible synthetic measurements and filesystem limitations are recorded in [the benchmark notes](m2-storage-benchmark.md#local-evaluation-and-scan-scaling-2026-10-03). No dependency or unsafe code was added. The registry security lookup passed for 226 locked versions; the accepted `paste` maintenance warning and the unaudited Git dependency remain visible.

M0 diagnostics require tagged identity and positive durations for probable duplicates, bound missing-position reports and saturate size estimates. Local browsing applies validated pagination, sorting and file-output options consistently. Edition relations compare disc positions and each track's encoding; version 3 of the relation rules repairs older inferred links on load. Repeated genre links and automatic alias bridges through names shared by distinct MBIDs are prevented on the next ordinary scan; exact same-name artists still share an ambiguous local row. Embedded WAV/AIFF ID3 and signed FLAC fields are hardened. FLAC/Ogg checksum checks allocate file-sized buffers, refuse streams over 2 GiB rather than verify a prefix, and clear old verdicts after failed rechecks. Earlier large-file verdicts require an explicit `check --full` refresh, as explained in [integrity guidance](../integrity.md).

M1 has received a follow-up quality, security and performance review. Approximate or rejected identities cannot become certain through secondary fetches, equally ranked homonyms are refused with their MBIDs, and malformed source tables cannot manufacture trusted records. Fetched edition credits require a non-empty exact edition ID; explicit manual credits stay anchored to their local release key. Precise exclusions survive an artist entering or leaving local tags while existing relationship selectors stay compatible. Repeated graph observations prefer trusted then recent evidence and keep the original claims. HTTP bodies are bounded before and after decompression, invalid UTF-8 and malformed service answers are refused, and 429 stops without blind retry. Artwork retries preserve individual completed files and distinguish a complete extra-image response from an existing folder. Wikipedia disambiguation extracts and unusable portrait statements are refused. Source decisions can be exported and forgotten without cached records; rules imports enforce their scope and preflight both stores. Source prose, review cards, previews and fingerprint output display terminal controls literally. Read-only M1 views take no writer lock; mutations and the two-store rules export retain it. Writer locks explicitly unlock on drop, so a temporarily inherited helper descriptor cannot delay their release. Fingerprinting requires a regular local file and a usable duration. Source-credit resolution, coverage, image selection and fingerprint planning avoid repeated full-table scans; URL encoding and filesystem validation reuse shared helpers. This initial pass added no dependency or unsafe code.

The optional M1 follow-ups are implemented. Complete MusicBrainz discographies, including empty answers, are cached across restarts with the optional `discography_fetched_at` timestamp; legacy nonempty lists remain reusable and legacy empty lists are queried once. Failed pagination or changing totals preserve the previous complete answer; unreadable stored rows invalidate the whole list rather than masquerading as a complete partial answer. Credit coverage reports canonical recordings and local edition lookups separately, retaining the existing JSON fields and counting manual corrections without fabricating a MusicBrainz lookup. All artwork publication, including local extraction, fully decodes JPEG and static PNG before any filesystem mutation, with input, dimension, pixel, output, chunk/scan and cooperative time budgets. Animated PNG is refused; JPEG decodability does not certify an unchanged original. The approved `png` 0.18.1 and `zune-jpeg` 0.5.15 dependencies add only `fdeflate` and `zune-core` transitively, without changing existing locked versions or requiring an external image helper. The updated registry security lookup passes for 230 locked versions, with the same accepted `paste` maintenance warning and unaudited pinned Git dependency. These additions preserve local tags, source attribution and the four live-service paths confirmed by the owner.

The current implementation includes:

- folder scanning;
- native tag reading;
- graph-based catalog;
- releases, recordings, tracks and artists;
- credits and relations;
- scoped MusicBrainz recording, work and edition credits, plus reversible
  manual corrections and precise exclusions that never change local tags;
- explicit sourced work/part navigation for classical movements, with local
  movement tags and performance personnel kept distinct from composer credits;
- catalog statistics and diagnostics;
- favourites, ratings, notes and user tags;
- listening history;
- saved collections;
- copying selections;
- catalog export;
- integrity checks;
- imported artist- and album-level analyses, retaining the newest dated
  result per file across scans and report deletion;
- direct album analysis through FlacCompagnon's Rust library;
- backup and restore;
- the versioned HTTP/JSON/WebSocket catalog API with CLI-shaped album/entity navigation, attributed artist origins, diagnostics, statistics and public search/query, plus opt-in asynchronous scan/fetch jobs with polling/cancellation and WebSocket task activity; local HTTP is loopback-only by default and direct HTTPS adds account-backed remote access without changing catalog route semantics (see the [complete route reference](../../crates/aede-server/README.md));
- audio fingerprinting;
- MusicBrainz fetching;
- MusicBrainz discography information;
- Wikipedia/Wikidata summaries;
- a Discogs profile fallback for labels without Wikipedia prose, checked when
  the label page opens unless `--offline` is used, with source attribution and
  plain-text rendering of linked label and artist names, preserved source
  paragraphs with an emphasized label code, and an animated loading indicator
  during online requests;
- cover-art fetching;
- language selection for fetched prose;
- artist identity: spellings merged on a shared MusicBrainz identifier, and
  `aede merge` for the files that never met one, with `aede doctor` suggesting
  pairs and applying none;
- dated band membership, and the line-up of an album's year, derived on read;
- artist country/area and formation/end dates from MusicBrainz, with
  `--country` filtering on `artists`;
- lyrics fetching from LRCLIB, behind `--lyrics` and never on by default;
- folder narrowing on every option of `fetch`: a positional that exists on disk
  is a folder, anything else is a name, and the two narrow a run independently.

The `aede-dsp` crate defines the decoded PCM contract, a continuous gain stage, optional broad bass/treble shelves with a flat bypass and preamp headroom, peak reporting, integrated LUFS and true-peak measurement over PCM, and a 24-band spectrum analyzer for playback visualization. `aede-core` has an in-memory playback queue with transport state, repeat and seeded uniform shuffle, plus a selector for ReplayGain and Opus R128 playback gains. Its progressive file decoder produces finite, complete PCM frames into caller-owned buffers; fixtures verify FLAC, WAV, LAME MP3 and native Vorbis frame counts after encoder trimming, plus an optional ffmpeg path for Opus, AAC and ALAC in M4A. Vorbis prefix/final bounds and reference samples are checked independently against Xiph libvorbisfile, including very short and multiple-page streams; detected chained resets and missing EOS bounds are errors. Opus pre-skip/end trimming has a fixture count check. The reusable `PcmTrack` layer decodes, calls the DSP and supplies `f32le` blocks plus format metadata to an output sink; it has no device or network dependency. F7 preserves known source speaker masks, offers a peak-safe multichannel-to-stereo downmix with LFE omitted, and refuses unknown multichannel layouts for local playback. F8 negotiates a native device rate, preferring floating sample formats before rate distance, and uses stateful bandlimited conversion in `aede-dsp` when the selected output rate differs from the source; ffplay keeps the decoded rate. F9 prefers floating-point native output and otherwise converts guarded PCM to supported integer formats with continuous TPDF dither in the CPAL callback. `aede play` streams a file, a recursively ordered folder, an M3U/M3U8 playlist, a saved collection, or a catalogued artist, album or track selection through this layer to a CPAL device stream when a compatible floating-point or integer channel format exists, or to ffplay otherwise, recording each listen in personal history and showing 24 animated bars that fill and follow the terminal width. The playback label shows album and numbered filename without the full path. The CPAL callback reads a preallocated `rtrb` PCM ring with 500 ms of negotiated-rate capacity across matching-format tracks, while the ffplay fallback reuses one process; both restart for format changes and discard queued output on skips. Output-format changes and final drain poll transport controls, including the native 100 ms host allowance measured in active time. Native drain reports five seconds without consumed-frame progress as an error; the allowance is not a physical playback acknowledgement. The native sink receives original DSP samples, accepts complete-frame prefixes without per-attempt vectors and retains channel alignment through queue shortages. Rendering and error capture use bounded sample work and atomics without Aède heap work, locks or logging in the callback. Driver diagnostics report consumed frames, bounded queue occupancy, shortages and host xruns; recoverable route/scheduling notifications remain warnings. History writes are asynchronous to the decode loop. The shared `PcmSession` retains EQ and rate-conversion state across compatible natural track joins, flushes once at group end and attributes delayed output to its original token/gain using cumulative frame boundaries. Input/output/tone incompatibility starts a new session; skips, Previous, Stop and failed sources discard pending processing state. Complete-frame progress includes partial output writes, while incomplete-span peak/guard statistics are explicitly unavailable. A real FLAC-to-MP3 fixture test checks that matching-format tracks arrive as exactly concatenated PCM. On Unix terminals, Space, Next, Previous and Stop control playback without leaving the shell; Windows terminal controls remain future work. This does not yet drive the in-memory queue. ReplayGain or Opus R128 metadata normalization toward -18 LUFS defaults to album gain for catalogued album selections and track gain for files, folders, playlists, collections, artists and tracks; `--normalize off|track|album` overrides it. A headroom cap limits normalization gain using ReplayGain peak metadata, measured true peak, or a conservative full-scale assumption when no peak is available. The final output guard hard-clamps and reports unexpected over-full-scale samples before CPAL or ffplay; this does not guarantee a true-peak ceiling. An underrun can still occur if decoding or file opening takes longer than the bounded buffer, and physical gapless playback has not been measured on hardware. The F5 normalization session reuses current FlacCompagnon track LUFS/true peak, tags or cached measurements without predecoding the selection. Missing source loudness is captured during playback before all processing, with a fixed current gain, then cached outside the decode loop only after complete, unchanged decoding. Album capture requires its complete uninterrupted programme; an album without ready album data retains one unchanged level for the session. Loudness cache method version 4 discards earlier derived caches with potentially underestimated EOF true peaks or incorrect Vorbis bounds; imported analyses remain available. Finite source/output measurements resolve delayed interpolation through a separate peak-only tail, leaving LUFS gates and playback frames unchanged. Integrated LUFS works at high rates, while true peak is unknown at 192 kHz and above. The native server path now transports one processed PCM track through the same normalization and tone-control layer to an authenticated client; client acknowledgements control the stream and its private listening history. FLAC MD5 verification is not implemented in the playback path yet.

The F10 playback meter now observes guarded PCM submitted to the local output and reports sample peak, an available oversampled true-peak estimate, pre-guard peak, and guard intervention count. The CLI shows each active DSP stage, normalization and tone headroom reserves, and that dynamic gain reduction is unavailable without a limiter. These measurements precede dither and device conversion. The native server playback route uses the same PCM processing and selected normalization policy, but does not create a new one-track loudness measurement; it also does not provide a browser/mobile user interface or prove physical playback behavior. The [DSP review](dsp-review.md) records the continuous-join, native callback, Vorbis-bound and controlled-drain corrections, absolute synthetic LUFS/true-peak and converter validation, the 192→48 kHz filter bandwidth correction and release profiling, with remaining reference coverage, device timing and target-NAS performance work; the current free DSP is not yet validated for seamless processed joins on hardware.

The [M2 server security review](m2-server-review.md) records the original local-service findings and the corrective work: Host/Origin validation, bounded WebSocket/IPC clients, graceful command connection shutdown, coordinated scan publication, read-only legacy loads and non-replacing sidecar publication. The current implementation also adds account-backed direct HTTPS, bounded connection and remote-request admission, and authenticated PCM playback. Navigation, inspection and personal operations use bounded workers. Target NAS capacity, browser interoperability and physical-client behavior still need validation. Server code is separated into routing, runtime/state, catalog/navigation, inspection, security, accounts/sessions, events, playback and job/delegation modules.

A disposable macOS rehearsal verified scan, loopback serving, backup, restart and restore with separate music and data folders. A [vendor-neutral Docker deployment procedure](../operating.md#docker-image-on-a-linux-host-procedure-only) is documented, but no image is built or published. Container startup, permissions, persistent mounts, off-host backup, host reboot and any target NAS remain unverified until suitable hardware and an image are available. NAS-specific packages and service recipes are deferred.

---

## M1 principles carried into M2

M1 introduces identification and external metadata without replacing local data.

The fundamental rule is:

> External information is a claim beside local data, not a replacement for it.

Important consequences:

- local tags remain authoritative for local file metadata;
- external values are attributed to their source;
- disagreements remain visible;
- external data can be removed without destroying local metadata;
- approximate matching must retain confidence information;
- no automatic retagging is performed.

See:

- `docs/design/roadmap.md`
- `docs/annotating.md`
- `docs/sources.md`
- `docs/library.md`

---

## Website and user documentation

The Onde website and bilingual documentation are generated into ignored `dist-site/` from `site/`, Markdown under `docs/`/`docs/fr/`, and the `docs/site-*.json` page manifests. The user manual, all 58 CLI commands, the local server routes and current DSP stages have beginner-facing guides. Existing topic references are published from their source files. Navigation preserves sidebar position and expanded groups; DSP illustrations use synthetic signals and respect reduced motion.

`tools/check.sh` now verifies the website renderer, published links/metadata, complete CLI/HTTP documentation registration and included Rustdoc. The Site workflow validates pull requests and publishes the combined website/Rustdoc artifact on default-branch or manual runs. See [site maintenance](../../site/README.md). The signup form uses an email relay with manual list management; recipient activation is required by the external provider.

---

## Test status

**Last recorded test count:** 1536 at the account-foundation checkpoint (including three doctests; none ignored)

**Last verified:** 2026-10-03, through the complete `tools/check.sh` HTTPS/native-PCM checkpoint, on macOS with FFmpeg conversion coverage required. The default and no-default-features workspace tests pass, including all 98 server tests, with no ignored tests. Formatting, warning-free lint and Rustdoc, bilingual website checks and the release build pass. The registry audit passes for 235 locked versions with the existing maintenance warning and Git coverage exclusion. The synthetic graph invariant was checked separately during the earlier M0 review; Linux-only non-UTF-8 disk regressions, native Windows behavior and target-NAS deployment still need their respective hosts. The declared MSRVs of the new packages fit Rust 1.89; this local checkpoint ran Rust 1.98.1.

The count above is informational.

Do not change it after every implementation task.

Update this number only when deliberately recording a new project checkpoint, normally:

- at the end of a milestone;
- after a significant test-suite change;
- or when explicitly asked to update the project state.

The authoritative command is:

```sh
cargo test
```

If the test count changes during development, that does not by itself require this file to change.

---

## Verification status

The repository verification command is:

```sh
tools/check.sh
```

It covers the project's required formatting, tests, documentation and lint checks.

Before a milestone is considered complete, run:

```sh
tools/check.sh
```

Rust verification is performed locally when Claude Code does not have access to the Rust toolchain.

---

## Architecture state

### Catalog

The catalog is persistent and graph-based.

Important concepts:

- Release
- Recording
- Track
- Artist
- Credit
- Relation

Identifiers are deterministic across scans of the same library.

### Storage

The catalog and related stores use versioned JSON representations.

Current stores include:

- `catalog.json`
- `conclusions.json` (integrity verdicts, fingerprints and imported analyses)
- `user.json`
- `sources.json`
- optional private `accounts.json`, created when account access is initialized

Backup/restore operates on these stores independently.

### External sources

External information is stored separately from local file metadata.

Current external sources include:

- MusicBrainz
- Wikidata
- Wikipedia
- AcoustID
- Cover Art Archive
- Fanart.tv (artist and label logos, portraits, banners, 4K-preferred
  backgrounds, album covers, and cdART)

Network access is explicit and does not occur during ordinary local catalog operations.

---

## Important constraints

- Never modify audio files or their tags.
- Never silently overwrite local metadata with external data.
- Never silently ignore a CLI argument or option.
- Never introduce network access into local-only commands.
- Never add a dependency without justification.
- Never weaken tests to make an implementation pass.
- Never introduce unrelated refactoring into a task.
- Preserve provenance for external information.
- Preserve deterministic catalog construction.

---

## Working rule for Claude

At the beginning of a new session:

1. Read this file.
2. Read the relevant milestone/task document.
3. Inspect the existing implementation.
4. Work only on the requested task.
5. Update this file only when the project state itself has materially changed.

Do not reconstruct project history from previous conversations.

The repository and documentation are the source of truth.
