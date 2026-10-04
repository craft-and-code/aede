# Aède — Current State

This file is the short-term memory of the project.

It describes the state of the repository at the beginning of a development session. It is intentionally concise.

Do not turn this file into a development diary.

---

## Project status

**Current milestone:** M3 — playback completion

**Previous milestone:** M2 — API and persistence separation (implemented and hardened; deployment acceptance pending)

**Status:** M1 is done. M2 persistence separation and the HTTP/JSON/WebSocket catalog API in `aede-server` are implemented: the frozen `/api/v1/events` stream remains catalog-only, while `/api/v1/activity` reports task activity. CLI-shaped read routes and opt-in JSON scan/fetch jobs have a complete [route README](../../crates/aede-server/README.md); HTTP jobs support authenticated polling/cancellation, keep bounded in-memory results and survive client disconnection. The original bodyless scan remains synchronous.

The current priority is completing M3. Local playback now drives the queue with repeat off/one/all, seeded uniform and metadata-based smart shuffle, initial seeking and runtime ten-second moves. Windows console transport controls, elapsed/total terminal progress, opt-in synchronized terminal lyrics and decoded FLAC MD5 verification are implemented; actual Windows console/output and physical local/remote output acceptance remain open. The native lyrics API supplies local text/timestamps separately from PCM so graphical clients such as Phémios can follow their own playback clock. Native remote finite queues preserve compatible processing joins; optional interactive playback adds seeking, revision-checked future-tail editing and private profile checkpoints for explicit resume. Additional named shuffle modes, local CLI live queue editing and local CLI persistent sessions remain future designs. The local Submariner browsing/audio/ratings/favourites workflow is accepted. Linux/Windows CI confirmation and Docker/NAS deployment are deferred; M4 device protocols will wait until after M3. These deferrals do not establish platform or deployment acceptance. See the [roadmap](../design/roadmap.md) and [M3 software hardening](m3-review.md).

Local HTTP remains loopback-only by default. Direct HTTPS accepts an explicit listener IP, certificate/private-key pair and exact public authority; it requires initialized accounts, including for a TLS listener on loopback. HTTPS leaves the catalog and personal API contract unchanged, but disables every `/api/admin` family and `AEDE_ADMIN_TOKEN`. Connection admission and remote request work are bounded, while target NAS capacity and browser interoperability still need validation. See the [operating guide](../operating.md) and [remote-access guide](../server/remote.md).

The native playback WebSocket accepts one current catalogued track or an opt-in finite ordered queue of at most 64 occurrences for a user or administrator session. It streams shared-DSP, processed `f32le` PCM, requires cumulative client acknowledgements and writes private listening history from acknowledged playback. Up to four playback decoders run concurrently. Compatible joins retain the shared DSP/conversion state; output rate stays fixed and mono/stereo changes wait for old-format consumption. Histories are saved in a bounded end-of-session batch, with separate occurrence boundaries and confirmations. An optional interactive mode adds source seeking, future-tail edits and account/profile checkpoint resume, while preserving the original single/finite wire contracts. Epoch/reset acknowledgements prevent discarded prefetched PCM from becoming consumption; checkpoint writes are bounded and explicit resume revalidates sources without duplicating prior history. It does not start audio automatically after restart. This is a transport contract, not a browser/mobile UI or a device-output integration; real client, physical-playback and target-NAS validation remain open.

The [Subsonic/OpenSubsonic adapter](../server/subsonic.md) adds public extension discovery, individually revocable API keys, strict GET/form-POST parsing, XML/JSON envelopes, ID3 browsing/search, alphabetical/year/genre/random album lists, genre/random songs, actual scan activity, original-file stream/download with byte ranges, and catalogued JPEG/PNG sidecar artwork with optional response-only thumbnails. Private favourites, artist/album/track ratings and ordered playlists share the native personal store. Client scrobbles have unknown played duration/completion and are stored separately from acknowledged PCM `Play` events; now-playing declarations are ephemeral and owner-private. Legacy desktop clients can use a complete application key as `u+p` (plain or `enc:`), while account passwords and MD5 authentication remain unsupported. Supersonic macOS setup is documented, including its required Title (A-Z) album sort. Local authentication, artist/album browsing, original FLAC playback, ratings and artist/album favourites are confirmed with Submariner 3.4 on macOS; the guide makes key creation and refreshing legacy file identities explicit. For exact `c=submariner` (ASCII case-insensitive), artist indexes/details use canonical album artists, including consistent favourite album counts; other clients also associate editions with their main track/recording artists. Contributor-only and albumless artists stay searchable through songs and the native graph without empty index entries. IDs, canonical album artists and explicit favourites are preserved. Keys and image handling reuse approved dependencies. Native bearer/PCM contracts stay separate; original music and artwork files are never rewritten. Transfers and workers are bounded and recheck revocation through consumer completion. Queue/bookmark persistence, transcoding, unsupported album sorts and broader client/deployment validation remain open.

The M2 hardening follow-up validates opened credential/backup descriptors and lock identities, refuses malformed graph tables and unreadable optional stores, bounds local HTTP connections/headers/output progress, and fixes early-shutdown and PCM-ACK cancellation races. Original heavy catalog routes now share bounded workers with navigation/inspection/personal work; query/history pages are rendered after pagination and query sort keys are computed once. New regressions and public RustDoc cover these rules, with [measured local HTTP improvements](m2-storage-benchmark.md#m2-http-hardening-comparison-2026-10-03). No dependency or unsafe code was added. Local HTTP's account-activation latch belongs to the running process; restarting without an account store intentionally restores legacy anonymous access. HTTPS always requires accounts.

On Unix, store-changing CLI commands delegate to a running server over a private socket; all writers share a data-folder lock. Account commands run directly under that lock. Delegated scans and fetches can be stopped with `aede cancel <task-id>`; activity reports lifecycle phases with an unknown work total, while detailed output stays on the CLI or authenticated task result. M2's [storage baseline](m2-storage-benchmark.md) is recorded; SQLite remains deferred pending target NAS budgets and real-library validation. Windows drive/UNC/verbatim paths are corrected without rewriting stored paths; native Windows CI validates scanning, sidecars and copying. Local delegation remains Unix-only (see [Paths](../design/paths.md)). M0.5 is also done (query grammar covers relations, and the command options are shorthand for it). A representative manual verification pass was completed before M2; the owner has since confirmed AcoustID, Fanart.tv, successful lyrics and cover downloads in real use. The checks are recorded in `docs/coding/m1-manual-verification.md`.

Aède has completed the local catalog foundation and the external identification layer.

The standalone [`aede-accounts`](../../crates/aede-accounts/README.md) crate owns account identities, roles, credential policy and API keys. `aede-core::accounts` retains protected file persistence and reexports the domain API; HTTP sessions and authorization remain in `aede-server`. Existing credential documents, backup formats and personal owners remain compatible. The crate reuses already approved dependencies.

The [account foundation](../design/accounts.md) provides CLI and HTTP administration, admin/user/auditor roles, salted Argon2id credentials, bounded process-local bearer sessions, expiry/revocation and authenticated personal owners. The first administrator keeps `local`; account changes preserve opinions/history while revoking its credentials. Users may change only their own personal data; auditors have read-only views and cannot manage accounts/jobs/credentials or start audio. Personal operations recheck sessions under the writer lock. Version-3 backups include private accounts and rotate the credential epoch when accounts are restored; legacy backups without accounts and reset preserve existing credentials. Optional API keys authenticate the separate adapter and are managed locally. Argon2 0.5.3 and tokio-rustls 0.26.4 provide password hashing and HTTPS. Local CLI password initialization, creation and reset now use masked terminal input with confirmation, using native terminal input without an additional Rust dependency; explicit `--password-stdin` remains available for scripts. Account publication reloads credentials under the writer lock after input, without blocking other writers while prompting. The registry audit covers 235 locked versions with the existing `paste` warning and Git exclusion. CLI/server documentation is bilingual. Browser login/playback interfaces and broader client interoperability remain future work.

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
- opt-in `play --lyrics`, with callback-consumed native timing, explicitly estimated ffplay timing and pause/seek/repeat/track-occurrence handling, plus a separate bounded native lyrics read API for graphical clients;
- folder narrowing on every option of `fetch`: a positional that exists on disk
  is a folder, anything else is a name, and the two narrow a run independently.

The `aede-dsp` crate defines the decoded PCM contract, a continuous gain stage, optional broad bass/treble shelves with a flat bypass and preamp headroom, peak reporting, integrated LUFS and true-peak measurement over PCM, and a 24-band spectrum analyzer for playback visualization. `aede-core` provides the in-memory queue used by the CLI, transport state, repeat, seeded uniform and metadata-based smart shuffle, plus a selector for ReplayGain and Opus R128 playback gains. Its progressive file decoder produces finite, complete PCM frames into caller-owned buffers; fixtures verify FLAC, WAV, LAME MP3 and native Vorbis frame counts after encoder trimming, plus an optional ffmpeg path for Opus, AAC and ALAC in M4A. Vorbis prefix/final bounds and reference samples are checked independently against Xiph libvorbisfile, including very short and multiple-page streams; detected chained resets and missing EOS bounds are errors. Opus pre-skip/end trimming has a fixture count check. The reusable `PcmTrack` layer decodes, calls the DSP and supplies `f32le` blocks plus format metadata to an output sink; it has no device or network dependency. F7 preserves known source speaker masks, offers a peak-safe multichannel-to-stereo downmix with LFE omitted, and refuses unknown multichannel layouts for local playback. F8 negotiates a native device rate, preferring floating sample formats before rate distance, and uses stateful bandlimited conversion in `aede-dsp` when the selected output rate differs from the source; ffplay keeps the decoded rate. F9 prefers floating-point native output and otherwise converts guarded PCM to supported integer formats with continuous TPDF dither in the CPAL callback. `aede play` streams a file, a recursively ordered folder, an M3U/M3U8 playlist, a saved collection, or a catalogued artist, album or track selection through this layer to a CPAL device stream when a compatible floating-point or integer channel format exists, or to ffplay otherwise, recording each listen in personal history and showing a twelve-band Retro spectrum grouped from the existing 24 analysis bands, with broad segmented green/yellow/red columns, slowly falling peak markers and terminal-width adaptation; bounded FFT snapshots now wait for the shared consumed-frame presentation clock, preserving compatible joins and resetting on transport/output changes; monochrome output retains its geometry. The playback label shows album and numbered filename without the full path. Terminal progress and lyric cues share a bounded occurrence clock: native callback-consumed frames determine elapsed position, while ffplay marks its active-time estimate; pause freezes it and seek adds the source offset. Metadata duration is corrected at source EOF. Progress remains visible with lyrics and is absent from redirected output. The CPAL callback reads a preallocated `rtrb` PCM ring with 500 ms of negotiated-rate capacity across matching-format tracks, while the ffplay fallback reuses one process; both restart for format changes and discard queued output on skips. Output-format changes and final drain poll transport controls, including the native 100 ms host allowance measured in active time. Native drain reports five seconds without consumed-frame progress as an error; the allowance is not a physical playback acknowledgement. The native sink receives original DSP samples, accepts complete-frame prefixes without per-attempt vectors and retains channel alignment through queue shortages. Rendering and error capture use bounded sample work and atomics without Aède heap work, locks or logging in the callback. Driver diagnostics report consumed frames, bounded queue occupancy, shortages and host xruns; recoverable route/scheduling notifications remain warnings. History writes run outside decoding behind a bounded 64-event queue; sustained storage lag back-pressures playback instead of growing memory indefinitely. The shared `PcmSession` retains EQ and rate-conversion state across compatible natural track joins, flushes once at group end and attributes delayed output to its original token/gain using cumulative frame boundaries. Input/output/tone incompatibility starts a new session; skips, Previous, seeking, Stop and failed sources discard pending processing state. Complete-frame progress includes partial output writes, while incomplete-span peak/guard statistics are explicitly unavailable. A real FLAC-to-MP3 fixture test checks that matching-format tracks arrive as exactly concatenated PCM. Unix terminals and native Windows consoles support Space, Next, Previous, relative seeking, repeat/shuffle cycling and orderly Stop/Ctrl-C without leaving the shell. Windows shares the account input guard, retains AltGr brackets and temporarily disables classic-console selection freezes; it restores the original mode on exit. Redirected stdin disables interactive controls. Actual Windows console/output acceptance remains pending. ReplayGain or Opus R128 metadata normalization toward -18 LUFS defaults to album gain for catalogued album selections and track gain for files, folders, playlists, collections, artists and tracks; `--normalize off|track|album` overrides it. A headroom cap limits normalization gain using ReplayGain peak metadata, measured true peak, or a conservative full-scale assumption when no peak is available. The final output guard hard-clamps and reports unexpected over-full-scale samples before CPAL or ffplay; this does not guarantee a true-peak ceiling. An underrun can still occur if decoding or file opening takes longer than the bounded buffer, and physical gapless playback has not been measured on hardware. The F5 normalization session reuses current FlacCompagnon track LUFS/true peak, tags or cached measurements without predecoding the selection. Missing source loudness is captured during playback before all processing, with a fixed current gain, then cached outside the decode loop only after complete, unchanged decoding. Album capture requires its complete uninterrupted programme; an album without ready album data retains one unchanged level for the session. Loudness cache method version 5 requires subsecond source identity and discards earlier derived caches with potentially underestimated EOF true peaks or incorrect Vorbis bounds; imported analyses remain available. Finite source/output measurements resolve delayed interpolation through a separate peak-only tail, leaving LUFS gates and playback frames unchanged. Integrated LUFS works at high rates, while true peak is unknown at 192 kHz and above. The native server path now transports a processed PCM track or finite ordered queue through the same normalization and tone-control layer to an authenticated client; client acknowledgements control the stream and its private listening history. Native and Ogg FLAC playback now verifies a present STREAMINFO MD5 from original integer samples in the same progressive decode before float conversion or DSP; complete EOF finalizes the comparison. Missing signatures and interrupted sources establish no verified digest. Mismatches stop CLI/native PCM queues, preserve prior completed histories and leave the failing occurrence incomplete without newly captured full-track loudness. Progressive seeking hashes the discarded prefix, independently of listening completion. Runtime verdicts are not persisted and imported analyses never bypass the current check. Native optional metadata is skipped without allocation on the same file descriptor; independent 32-bit channels are supported while the pinned codec's unsupported correlated 32-bit and explicit-header cases are refused. See [integrity](../integrity.md#flac-audio-md5-during-playback).

The [capture tooling](gapless-measurement.md) supplies quiet whole/split probes, explicit native playback, bounded CPU/I/O load and conservative captured-marker timing analysis. It distinguishes synthetic, digital-loopback and physical evidence; actual device measurements remain pending.

The [play manual](../cli/play.md) specifies the local queue, seeking and repeat/shuffle contract. Order is a permutation of occurrences, retaining M3U duplicates; runtime changes preserve the played prefix and current audio. Smart uses track genres and bounded genre/credit evidence, with non-compilation release fallback, without audio analysis or network access. It keeps every occurrence, reports style breaks/unknown metadata and falls back to uniform order when no usable genres exist. Source-frame seeking progressively discards the prefix with bounded memory and linear work; skipped samples bypass DSP, output, source metering and history. Multiple seeks within one visit retain one incomplete listening record and never publish partial loudness as a full-track measurement. The CLI queue, seed and position are not persisted. Native PCM v1 accepts a separate finite ordered queue with compatible continuous joins, cumulative acknowledgements and per-occurrence history. Its opt-in interactive extension adds absolute source seeking, future-tail editing and explicit private-profile resume with acknowledged cursor/settings persistence; it uses the documented epoch/reset handshake rather than CLI control messages.

The F10 playback meter now observes guarded PCM submitted to the local output and reports sample peak, an available oversampled true-peak estimate, pre-guard peak, and guard intervention count. The CLI shows each active DSP stage, normalization and tone headroom reserves, and that dynamic gain reduction is unavailable without a limiter. These measurements precede dither and device conversion. The native server playback route uses the same PCM processing and selected normalization policy, but does not create new loudness measurements; it also does not provide a browser/mobile user interface or prove physical playback behavior. The [DSP review](dsp-review.md) records the continuous-join, native callback, Vorbis-bound and controlled-drain corrections, absolute synthetic LUFS/true-peak and converter validation, the 192→48 kHz filter bandwidth correction and release profiling, with remaining reference coverage, device timing and target-NAS performance work; the current free DSP is not yet validated for seamless processed joins on hardware.

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

The Onde website and bilingual documentation are generated into ignored `dist-site/` from `site/`, Markdown under `docs/`/`docs/fr/`, and the `docs/site-*.json` page manifests. The user manual, all 58 CLI commands, the local server routes and current DSP stages have beginner-facing guides. Existing topic references are published from their source files. Homepage section links align below the measured sticky-header height, including after viewport or language changes. Feature bullets align with their first text line. Documentation navigation preserves sidebar position and expanded groups, with a visible gap after the active-item marker; DSP illustrations use synthetic signals and respect reduced motion.

`tools/check.sh` now verifies the website renderer, published links/metadata, complete CLI/HTTP documentation registration and included Rustdoc. The Site workflow validates pull requests and publishes the combined website/Rustdoc artifact on default-branch or manual runs. See [site maintenance](../../site/README.md). The signup form uses an email relay with manual list management; recipient activation is required by the external provider.

The bilingual [Compatible Aède specification](../server/compatible-aede.md) defines catalog, native-player and adapter client profiles with acceptance checks. The [project statistics](../manual/project-statistics.md) are generated from authored sources by crate; their rounded active-TU display comes from compiled lib/bin test inventories, excluding ignored tests and refusing stale Rust/Cargo inputs. Site publication and the local gate supply that inventory rather than maintaining numeric claims in Markdown.

---

## Test status

**Last recorded test count:** 1889 at the 2026-10-04 interactive native-playback checkpoint (including three doctests; none ignored). The preceding decoded FLAC MD5 checkpoint recorded 1862, synchronized lyrics 1837, Windows console transport 1798, native finite-queue 1783, local transport/shuffle 1772, Submariner 1714, and no-default-features 1659.

**Last verified:** 2026-10-04, the complete `tools/check.sh` after [Harden M3](m3-review.md), on macOS with Rust 1.89 and required FFmpeg coverage: **1923 default-feature Rust tests passed**, including three doctests, none ignored. This includes 1636 active lib/bin unit tests (13 accounts, 460 CLI, 868 core, 78 DSP, 217 server); public statistics use their generated rounded lower bound, separately from integration/doctests. Formatting, warning-free all-target lint, all helper/site tests, 15 real account pseudo-terminal tests, fresh statistics, 109 bilingual documentation pages, RustDoc/link checks and release compilation pass. Rust 1.99 all-target Clippy and the local release build also pass. Final documentation-only additions were followed by repeated documentation-index/link and bilingual-site regressions. No dependency, new unsafe code, minimum Rust version or CI workflow was changed.

M3 hardening regressions cover atomic numerical refusal/recovery, precise loudness reuse/capture/publication and backward-compatible v5 cache expiration, local-only FFmpeg protocol-like filenames, non-lossy persistent identities, pre-epoch lyric sources, native source paths and saturated decoder/ACK shutdown, consumed-frame spectrum snapshots across compatible joins/resets, bounded lookahead and tiny seek-segment history. Existing queue/shuffle, source-integrity, epochs, profile isolation and finite-tail signal tests retain their behavior. The matched DSP benchmark improves whole-block FFT processing 2.51× with unchanged checksums/frame counts; see the [review evidence](m3-review.md). Software timing still does not establish physical local/remote gapless, Windows output/console acceptance or target-NAS budgets. Local listening history remains submitted/active-time based, while native remote history follows client ACKs.

At the preceding Windows console checkpoint, the production console, playback-control and password modules compiled and passed Clippy with Rust 1.89 for `x86_64-pc-windows-msvc` through a standalone metadata harness; this does not establish MSVC linking or actual Windows console/output behavior. The existing Windows FFI is shared rather than duplicated, with one additional bounded console-event-count query and no new dependency. Native Linux/Windows execution and target NAS deployment remain separate acceptance work. The thirteen measurement-helper tests still pass; physical gapless acceptance awaits a suitable return cable and actual idle/load captures. The Harden M3 no-default-features workspace check also passes with Rust 1.89: 1906 Rust tests, none ignored, plus warning-free all-target Clippy and RustDoc.

The preceding 2026-10-04 complete Submariner checkpoint passed all gates after album-artist associations, canonical client counts and owner-scoped favourites. The owner confirms local Submariner 3.4 authentication, album browsing, original FLAC playback, seeking, random, ratings and artist/album favourites. The 2026-10-04 live registry audit covers 235 locked versions and passes with the existing `paste` maintenance warning and pinned-Git coverage exclusion. No dependency changed during M3 hardening.

**Previous verification checkpoint:** 2026-10-03, through the complete `tools/check.sh` after M2 hardening and the desktop-client Subsonic/OpenSubsonic lot, on macOS with FFmpeg conversion coverage required. The default and no-default-features workspace tests pass, including all 171 server tests, with no ignored tests. Formatting, warning-free lint and Rustdoc in both feature configurations, bilingual website checks and the release build pass. Real HTTP regressions cover legacy key authentication, ordered playlist form requests, owner isolation, artwork/original audio, scrobbles, now-playing and HEAD mutation refusal; a 4K image fixture verifies response-only reduction and unchanged source bytes. The final documentation checkpoint and local Supersonic instructions were checked again after updating these counts. At that checkpoint, no locked dependency had changed after the M2 registry audit of 235 versions, which passed with the existing maintenance warning and Git coverage exclusion. The synthetic graph invariant was checked separately during the earlier M0 review; Linux-only non-UTF-8 disk regressions, native Windows behavior, real Subsonic clients and target-NAS deployment still need their respective hosts. The default-feature gate also passes with Rust 1.89, matching CI; the no-default-features checkpoint above ran Rust 1.98.1.

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
