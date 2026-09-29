# Playback (M3)

## Code ownership

| Crate | Responsibility |
| --- | --- |
| `aede-dsp` | PCM format, gain, broad bass/treble shelves and output protection, LUFS/true-peak metering, spectrum analysis, and future signal stages such as resampling. It receives samples and has no file or catalog dependency. |
| `aede-core::playback` | Decode files, read tags, choose normalization scope, reuse FlacCompagnon analyses, cache measurements, manage queue state, prepare PCM blocks, and provide the ffplay fallback session. `PcmStreamFormat` is an alias for the DSP format. |
| CLI and future server playback handlers | Select an output device or network transport, provide controls, and record playback history. The server audio handler has not been implemented. |

The sample-processing foundation lives in [`aede-dsp`](../../crates/aede-dsp/README.md). It accepts decoded PCM and provides a continuous gain stage plus an independent loudness and true-peak meter. [`aede-core::playback::decoder`](../../crates/aede-core/src/playback/decoder.rs) reads local files progressively into caller-owned, interleaved `f32` buffers and rejects invalid PCM formats, incomplete frames and non-finite samples. The repository fixtures verify FLAC, WAV and MP3 decoding with exact playable frame counts after MP3 encoder delay and padding. An installed ffmpeg decodes Opus, AAC and ALAC when the native decoder cannot open them; it also handles Vorbis because the current native path can emit samples beyond the final Ogg granule on short streams. Without ffmpeg, native Vorbis playback remains available but its end trimming is not guaranteed. Opus pre-skip and end trimming are checked against a real fixture. [`PcmTrack`](../../crates/aede-core/src/playback/stream.rs) owns this progressive decoding, applies a caller-supplied DSP function, rejects non-finite processed samples and returns complete blocks with both interleaved samples and `f32le` bytes. Its [`PcmStreamFormat`](../../crates/aede-core/src/playback/format.rs) is a compatibility alias for `aede-dsp::PcmFormat` and provides the rate and channel count independently of a device. No audio file is modified.

`aede play <selection>` streams files from this decoder through the DSP into the local audio output. A selection can be one file, a folder (recursively traversed in name order), an M3U/M3U8 file, a saved collection, or a scanned artist, album or track name. M3U entries play in written order, including repeats; relative paths resolve beside the playlist, while remote URLs and missing files are refused. A collection evaluates its saved query against the current catalog at playback time and follows catalog order. A bare collection name works when no music name matches; `collection:<name>` selects it explicitly when names overlap. An artist plays releases credited to them as album artist in year order, with each release's tracks in disc and track order. Matching album editions and tracks with the same title are all played in deterministic order; several matching artists require a fuller name. A file, folder or M3U does not need a catalog. The playback label shows the album title (or parent folder when uncatalogued), an em dash, and the filename without its extension so the track number remains visible. Each file playback that reaches at least one millisecond of audio records a listen in `user.json`; a stream that fails after producing audio records an incomplete listen. A zero-duration or immediately skipped selection does not increment the all-time count. `aede history` can show direct file plays even before a catalog exists. `aede played` remains available for manually recording listening done in another player.

On macOS, Windows and glibc Linux, the CLI first tries CPAL with a floating-point device configuration matching the decoded sample rate and channel count. One device stream and a bounded PCM queue span naturally advancing tracks of the same format; the callback keeps samples in order across block boundaries. History writes run on a separate thread so they cannot hold up the next decode. The output restarts at a format change or after Next, Previous or Stop. If a matching device output is unavailable, the CLI falls back to ffplay. The static musl build uses ffplay; building the glibc Linux backend needs ALSA development headers. Set `AEDE_AUDIO_BACKEND=ffplay` to choose the fallback, or `AEDE_AUDIO_BACKEND=native` to require CPAL and receive an error if it is unavailable. ffmpeg is additionally required for Opus and M4A fallback and for exact Vorbis end trimming. Tests check exact PCM concatenation in the ffplay path and callback continuity in the native path. A physical gap can still occur if file opening or decoding starves the bounded queue; there is no hardware loopback test yet. A different source and device rate needs the later resampler or a device format change. ReplayGain or Opus R128 normalization defaults to album gain when the selection resolves to catalogued releases. An album gain tag applies the same adjustment to its tracks, preserving their relative levels while aligning albums. Direct files, folders, M3U playlists, collections, artist selections and track selections default to track gain. `--normalize off|track|album` overrides that choice. An exact-scope ReplayGain or Opus R128 tag takes priority. When it is absent, track mode reuses a current FlacCompagnon LUFS and true-peak report or measures the decoded track with Rust ebur128. Album mode measures the ordered album programme if any album tag is missing; one measured gain applies to all its tracks, preserving their relative levels. A tag from the other scope is used only when a measurement is unavailable. The target is -18 LUFS. Silent or unsupported-layout files keep their decoded level unless a usable tag exists. For Opus, the decoder applies the ID header output gain first and the selected R128 comment gain is added afterward; a fixture with nonzero header gain verifies this order. The CLI reports both requested and applied gain when sample headroom reduces normalization. A final hard ceiling reports any unexpected out-of-range samples. Seeking remains future work.

For each selected gain, `aede-dsp::gain_with_headroom_db` limits the applied gain to `min(requested gain, -20 log10(source peak))` dB when a positive ReplayGain peak or measured true peak is available. A missing peak is treated as 1.0, so positive gain is withheld rather than risk clipping; a declared zero peak leaves the requested gain intact. This calculation preserves the chosen album or track scope but can fall short of the -18 LUFS target. The shared `Dsp::process_for_output` call applies gain, then uses `aede-dsp::protect_output` to leave samples in [-1, 1] untouched and hard-clamp any out-of-range sample, reporting the count. This is an explicit emergency safety ceiling for inaccurate tags, codec overshoot and files without gain metadata; it is not a transparent limiter. It does not measure inter-sample true peaks, so it does not guarantee a true-peak ceiling. Current, attributed FlacCompagnon true-peak data or a fresh ebur128 measurement can avoid the conservative missing-peak assumption. The future server audio route must apply the same headroom plan and final guard before transmitting PCM.

FlacCompagnon's in-process and imported reports retain integrated track LUFS and true peak. Playback accepts those values only when the source is FlacCompagnon, analysis succeeded, and file size and modification time still match. Missing track values are measured before playback by `aede-dsp::loudness`, which wraps the pure-Rust `ebur128` crate; `aede-core` handles decoding and caches measured results in the independent conclusions store with a method version. Album measurement combines gated programme energies rather than averaging track LUFS. Consecutive tracks with a matching format share one meter; a format change starts another meter and combines its gated history, so the 400 ms windows at that boundary are not continuous. The preflight decode can delay first playback on a large uncached selection. No audio file or tag is changed.

The CLI calls `PcmTrack::read_block` with its DSP instance and writes the resulting bytes to the selected local output. A future server playback handler should use the same `PcmTrack` and DSP call, send format metadata followed by the processed audio to the requesting device, and cancel decoding when that client disconnects. The local output session does not belong in that path: the remote device owns playback and its clock. Both CLI and server playback must record a `Play` through the shared `UserData::record_play` rule, using the same path-based track reference, all-time count and completed/incomplete semantics. The existing administrative history route records explicit events but does not observe playback automatically; the future server audio route needs a playback acknowledgement or equivalent evidence before marking a remote listen complete. The transport framing, encoding, client buffer limits, authentication and owner-scoped authorization must be designed with the remote API before a playback route is exposed. The current loopback server has no playback route and does not transmit audio.

`--bass DB` and `--treble DB` optionally apply broad low and high shelf filters, each accepting -12 to +12 dB and defaulting to zero. Supplying zero, or omitting the options on the next invocation, restores exact flat bypass. The 120 Hz and 4 kHz shelf midpoints move below Nyquist for low-rate sources. The `aede-dsp` filters use separate per-channel state and retain it across PCM blocks. For playback, `ToneControls::safe_preamp_db` subtracts the sum of positive shelf gains from the already peak-capped normalization gain; cuts need no extra preamp. This conservative reserve can make playback quieter than the -18 LUFS target. The final sample guard still catches filter transients and bad peak estimates; no true-peak ceiling is promised. A future server playback handler must apply the same tone settings, preamp rule and output guard while using its own network transport.

For known native multichannel sources, `PcmTrack::open_stereo` preserves the source speaker mask and supplies a stereo downmix to the local player. Conventional 2.1, 3.0, 3.1, quad, 4.0, 5.0, 5.1, 7.0 and 7.1 layouts are recognized; unrecognized layouts are refused rather than mapped by channel count. The matrix uses center/surround coefficients based on the ITU-R BS.775 stereo example and omits LFE. The 7.1 side and rear pairs use -6 dB each. A conservative row-sum scale prevents clipping when all contributing channels are coherent at full scale, though an isolated channel can play more quietly. The final output guard still applies. Mono and stereo pass through without channel remapping. The shared PCM format exposes source positions to a future server handler, which can choose passthrough or the same downmix for the client device. The FFmpeg decoder fallback currently cannot confirm a multichannel output mask and therefore does not downmix such streams. ffplay receives explicit layout names for raw PCM rather than a channel count that could be misinterpreted.

On a macOS or Linux terminal, playback accepts single-key controls without Enter: Space pauses or resumes, `n` or Right skips to the next track, `p` or Left goes to the previous track (or restarts the current track when more than three seconds have played), and `q` stops playback and returns to the shell. Pause suspends the selected output as well as the PCM feed. At the end of a selection, Next returns to the shell. In noninteractive use, playback runs through the selection without waiting for keyboard input. Terminal controls are not yet available on Windows.

During interactive terminal playback, twenty-four vertical bars display smoothed frequency-band levels computed from the PCM sent to the local output. The bars expand to fill the terminal width, and the width is checked periodically so they follow a window resize. The renderer leaves the last column unused to avoid wrapping. On a very narrow terminal, it removes gaps and then combines adjacent frequency bands so the display still fits. Half-height tips soften the movement without block-shaped cells. The display refreshes at most twenty times per second, clears itself at the end of a track and is absent when output is redirected. It is an approximate view of the samples entering the output, which can buffer audio before it reaches the device; it is not a measurement of the device's current output. The analysis does not change the samples or their playback gain.

The native decoder dependency skips corrupt packets, so this playback path cannot report every damaged frame. It also does not verify FLAC decoded-audio MD5. A future integrity-aware decoder must make those results explicit; successful playback decoding alone must not be treated as an integrity verdict.

The in-memory [`playback` queue](../../crates/aede-core/src/playback.rs) now accepts the same ordered track IDs as playlist rendering. It provides the basic transport state, repeat modes, and seeded uniform shuffle. Other shuffle styles remain designs for later work.

## The queue is a selection, not a new idea

Every page in Aède gathers a **selection** — that is what `--csv` and `--m3u`
already render, through one helper that no command knows the details of. A queue
is that same selection with a cursor on it. So the three ways a queue gets
filled are not three features:

- built by hand in the interface, track by track;
- taken from any Aède result — `aede artist Ozzy`, `aede genre metal`,
  `aede albums --year 1991` — anywhere `--m3u` works today;
- read from an M3U file, which is the same list written down.

Which means playback should not need a query language of its own. If a command
can hand its tracks to a playlist, it can hand them to the queue.

The transport is the small part: play, pause, stop, next, previous, seek. One
convention worth settling early because everyone has an opinion about it:
**previous** restarts the current track when more than a few seconds have been
played, and only goes back a track before that. Anything else makes the button
unusable for its actual purpose, which is "wait, play that again".

## Order is a permutation, not a coin flip

Repeat (one track, the whole queue) and shuffle are properties of the _order_,
not of the queue. This distinction is load-bearing:

**A shuffle produces an order, once, and the queue then holds that order.** It
does not draw a new track at each end-of-track. Drawing each time is how players
end up unable to say what comes next, and unable to go back — "previous" has
nothing to be previous to. Producing the order up front costs nothing and buys
both.

**The order is seeded, and the seed is stored with the queue.** Determinism is
the first invariant of this project, and a shuffle is no exception: the same
queue and the same seed give the same order, on any machine, for ever. That
makes a session reproducible, a complaint about a bad sequence reproducible, and
a "what is coming next" panel possible without the server having to commit to
anything it might contradict later. A shuffle that reaches the end and repeats
draws its next seed from the current one, so the second pass is not the first
one again.

## Styles of shuffle

Several are worth having, and they should be _one_ algorithm with two knobs
rather than six algorithms:

| Style      | What it does                                                                                                                                                       |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `uniform`  | The classic. A seeded Fisher–Yates over the queue.                                                                                                                 |
| `by-album` | Shuffles albums, keeps each album in its own order. An album is a sequence somebody intended.                                                                      |
| `spread`   | Uniform, but never two tracks of the same artist within _n_. Fixes "it played four Deicides in a row" without pretending that is not what uniform randomness does. |
| `similar`  | Stays close to the track it started from.                                                                                                                          |
| `journey`  | Deliberately drifts: ends somewhere else, having got there by walking.                                                                                             |
| `discover` | Weighted towards what has never been played, or not for a long time.                                                                                               |

## The smart shuffle, without a language model

The one that needs actual thought: a random that stays in the style of the first
track, may move to another, and never jumps — no Black Metal straight into Pop,
no Bach into rap.

The material is already in the catalog, which is the point of having built a
graph rather than a hierarchy:

- **A genre neighbourhood learned from the library itself.** Two genres are
  close when they keep turning up on the same releases and the same artists.
  Count the co-occurrences, normalise, and you have a weighted graph with no
  hand-written taxonomy in it and no external ontology to argue with. In a
  library like this one, Black Metal and Death Metal will sit next to each
  other because they genuinely do; Black Metal and Pop will have no edge at all,
  because nothing in the library connects them. The graph fits _this_ library
  rather than someone's idea of how music is organised, which is both the
  strength and the limit: a library of two genres has nothing to walk on.
- **The artist relation graph**, which `relations.rs` already builds from shared
  credits. Two artists who played together are near each other whatever their
  tags say.
- **Year and label**, worth small weights: a label is a curator, and a decade is
  a production sound.

From those, a distance `d(a, b)` between two tracks in `[0, 1]`, and then a
walk:

1. take the tracks within radius `r` of the one playing;
2. weight them by closeness — nearer is likelier, but not certain;
3. draw one with the seeded generator;
4. **never take a step longer than `d_max`.**

Rule 4 is the whole guarantee, and it is worth stating on its own: the style may
change only by _walking_, never by jumping. Black Metal reaches Pop only through
whatever lies between them in this library, one bounded step at a time, which is
to say it will usually not get there at all — and that is the desired behaviour,
not a limitation. The rule is local, cheap, and easy to test.

The drift is then a single parameter: `r` grows slowly with the number of tracks
played and resets when the user intervenes. `similar` keeps it low, `journey`
lets it climb, and `uniform` is the same walk with `r` unbounded. Two knobs,
six behaviours.

Plus memory, which every shuffle needs: no track twice within _m_, no artist
within _n_.

**All of this is pure computation over the catalog.** It belongs in
`aede-core`, it is unit-testable with no sound card and no audio at all, and it
should be written and tested before a single sample is decoded. The interesting
half of M3 does not need speakers.

## Volume, and position

Two questions worth answering before they get answered by accident.

**Volume is not Aède's business.** The system mixer owns it. A program that
keeps its own volume alongside the system's gives the user two knobs that
disagree and no way to tell which one is at fault.

**Loudness normalization is Aède's business**, and it is a different thing.
The raw tags already retain `REPLAYGAIN_*` and, for Opus, `R128_*`. The playback
gain selector now reads them, chooses track or album scope, and converts the
selected tag to an explicit target level before the DSP applies it. ReplayGain's
nominal reference is -18 LUFS; Opus R128's is -23 LUFS. The eventual decoder
must apply the Opus header's output gain before the R128 tag gain. For files
without tags, decoded audio can later be measured with EBU R128 and stored
exactly as an integrity verdict is stored: measured once, kept, recomputed only
on request. Aède decides what gain to apply to the stream; the user decides how
loud the room is.

**Position depends which position is meant.**

- The point reached in the current track is player state. The transport has to
  expose it — "next" and gapless are meaningless without it — and M2's WebSocket
  is where it belongs. It is not catalog data.
- Where the user stopped, kept for next time, is worth having. But not in
  `catalog.json`: that file is written whole, and a position that moves several
  times a second would rewrite the entire library on every tick. A small session
  file of its own, written often and cheap to lose.
- Resuming mid-track only makes sense for long pieces — an audiobook, a
  concert, an hour of Wagner. For a four-minute track, remembering the queue is
  enough and remembering the second is noise.

## Gapless, which is already half done

The hand-written parsers extract the LAME encoder delay and padding, the Opus
pre-skip and the ALAC magic cookie — none of which a general-purpose tag library
exposes, and all of which exist in this codebase for exactly this milestone.
M3 spends them. That was the bet made at M0, and it is the one worth checking
first: if the numbers turn out to be wrong, everything above is premature.

## What M3 must not become

No writing tags, no reorganising files on disk, no cloud, and no second
catalog. Playback reads; the scan is still the only thing that writes what the
library is.
