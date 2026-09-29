# Aède — Current State

This file is the short-term memory of the project.

It describes the state of the repository at the beginning of a development session. It is intentionally concise.

Do not turn this file into a development diary.

---

## Project status

**Current milestone:** M2 — API and persistence separation

**Previous milestone:** M0.6 — Catalog and local library

**Status:** M1 done. M2 persistence separation and the local HTTP/JSON/WebSocket API in `aede-server` are implemented: the frozen `/api/v1/events` stream remains catalog-only, while `/api/v1/activity` reports task activity. CLI-shaped read routes and opt-in JSON scan/fetch jobs now have a complete [route README](../../crates/aede-server/README.md); HTTP jobs support authenticated polling/cancellation, keep bounded in-memory results and survive client disconnection. The original bodyless scan remains synchronous. On Unix, store-changing CLI commands automatically delegate to a running server over a private local socket; all Aède writers still share a data-folder lock. Delegated scans and fetches can be stopped explicitly with `aede cancel <task-id>`; activity reports `running` and `refreshing` lifecycle phases with an explicit unknown work total, while detailed per-item fetch output stays on the connected CLI or authenticated HTTP task result. The API contract, migration safety and operating guidance are covered by tests and [operating documentation](../operating.md). M2's [synthetic storage baseline](m2-storage-benchmark.md) is recorded; SQLite remains deferred pending target NAS budgets and validation on real large libraries. Account management, remote access and mobile playback are planned after M2 step 9. Windows drive/UNC/verbatim path handling is corrected without rewriting stored paths; path-specific test ignores are removed and native Windows CI now validates scanning, sidecars and copying. Local delegation remains Unix-only (see [Paths](../design/paths.md)). M0.5 is also done (query grammar covers relations, and
the command options are shorthand for it). A representative manual verification
pass was completed before M2; optional untested services are recorded in
`docs/coding/m1-manual-verification.md`.

Aède has completed the local catalog foundation and the external identification layer.

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
- imported analyses;
- direct album analysis through FlacCompagnon's Rust library;
- backup and restore;
- a local read-only catalog API with CLI-shaped album/entity navigation, attributed artist origins, diagnostics, statistics and public search/query, plus opt-in asynchronous scan/fetch jobs with polling/cancellation and WebSocket task activity (see the [complete route reference](../../crates/aede-server/README.md));
- audio fingerprinting;
- MusicBrainz fetching;
- MusicBrainz discography information;
- Wikipedia/Wikidata summaries;
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

The `aede-dsp` crate defines the decoded PCM contract, a continuous gain stage, optional broad bass/treble shelves with a flat bypass and preamp headroom, peak reporting, integrated LUFS and true-peak measurement over PCM, and a 24-band spectrum analyzer for playback visualization. `aede-core` has an in-memory playback queue with transport state, repeat and seeded uniform shuffle, plus a selector for ReplayGain and Opus R128 playback gains. Its progressive file decoder produces finite, complete PCM frames into caller-owned buffers; fixtures verify FLAC, WAV and LAME MP3 frame counts after encoder trimming, plus an optional ffmpeg path for precise Vorbis ending and for Opus, AAC and ALAC in M4A. Opus pre-skip/end trimming has a fixture count check. Without ffmpeg, native Vorbis playback may include trailing padding. The reusable `PcmTrack` layer decodes, calls the DSP and supplies `f32le` blocks plus format metadata to an output sink; it has no device or network dependency. F7 preserves known source speaker masks, offers a peak-safe multichannel-to-stereo downmix with LFE omitted, and refuses unknown multichannel layouts for local playback. F8 negotiates a floating-point native device rate and uses stateful bandlimited conversion in `aede-dsp` only when the source rate is unsupported; ffplay keeps the decoded rate. `aede play` streams a file, a recursively ordered folder, an M3U/M3U8 playlist, a saved collection, or a catalogued artist, album or track selection through this layer to a CPAL device stream when a compatible floating-point channel format exists, or to ffplay otherwise, recording each listen in personal history and showing 24 animated bars that fill and follow the terminal width. The playback label shows album and numbered filename without the full path. The CPAL callback uses a bounded PCM queue across matching-format tracks, while the ffplay fallback reuses one process; both restart for format changes and discard queued output on skips. History writes are asynchronous to the decode loop. A real FLAC-to-MP3 fixture test checks that matching-format tracks arrive as exactly concatenated PCM. On Unix terminals, Space, Next, Previous and Stop control playback without leaving the shell; Windows terminal controls remain future work. This does not yet drive the in-memory queue. ReplayGain or Opus R128 metadata normalization toward -18 LUFS defaults to album gain for catalogued album selections and track gain for files, folders, playlists, collections, artists and tracks; `--normalize off|track|album` overrides it. A headroom cap limits normalization gain using ReplayGain peak metadata, measured true peak, or a conservative full-scale assumption when no peak is available. The final output guard hard-clamps and reports unexpected over-full-scale samples before CPAL or ffplay; this does not guarantee a true-peak ceiling. An underrun can still occur if decoding or file opening takes longer than the bounded buffer, and physical gapless playback has not been measured on hardware. The F5 measured fallback reuses current FlacCompagnon track LUFS and true peak; `aede-dsp` measures missing track or whole-album programme loudness with pure-Rust ebur128, while `aede-core` decodes files and caches results in the independent conclusions store. This may delay the start of an uncached selection. The future server path will transmit processed audio to the remote device using the same PCM and tone-control layer; the server exposes no playback routes yet. FLAC MD5 verification is not implemented in the playback path yet.

The [M2 server security review](m2-server-review.md) now has a corrective pass:
local Host/Origin validation, bounded WebSocket/IPC clients, graceful command
connection shutdown, coordinated scan publication, read-only legacy loads and
non-replacing sidecar publication. CLI operating help is expanded. The patched
TLS dependency is preserved in the versioned lockfile. These protections do not
provide accounts or authorize remote exposure; HTTP query-cost limits and the
account/session model remain future requirements. New navigation and inspection run on a shared bounded worker pool; the original list routes still need comprehensive remote-access cost budgets. Server code is separated into routing, runtime/state, catalog/navigation, inspection, security, events and job/delegation modules.

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

## Test status

**Last recorded test count:** 922

**Last verified:** 2026-09-26

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
