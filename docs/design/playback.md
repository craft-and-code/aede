# Playback (M3)

## Code ownership

| Crate | Responsibility |
| --- | --- |
| `aede-dsp` | PCM format, gain, broad bass/treble shelves and output protection, LUFS/true-peak metering, spectrum analysis, and fixed-ratio, bandlimited sample-rate conversion. It receives samples and has no file or catalog dependency. |
| `aede-core::playback` | Decode files, read tags, choose normalization scope, reuse FlacCompagnon analyses, cache measurements, manage queue state, prepare PCM blocks, and provide the ffplay fallback session. `PcmStreamFormat` is an alias for the DSP format. |
| CLI and future server playback handlers | Select an output device or network transport, provide controls, and record playback history. The server audio handler has not been implemented. |

The sample-processing foundation lives in [`aede-dsp`](../../crates/aede-dsp/README.md). It accepts decoded PCM and provides a continuous gain stage plus an independent loudness and true-peak meter. [`aede-core::playback::decoder`](../../crates/aede-core/src/playback/decoder.rs) reads local files progressively into caller-owned, interleaved `f32` buffers and rejects invalid PCM formats, incomplete frames and non-finite samples. The repository fixtures verify FLAC, WAV, MP3 and native Vorbis with exact playable frame counts, including MP3 encoder delay/padding and Vorbis prefix/final granules. Vorbis uses the existing Symphonia codec with corrected container bounds: a CRC-checked initial audio page distinguishes a real prefix crop from a single-page ending, packet trims are cleared, and the source frame budget is applied once. The same file handle supplies bounded header inspection and progressive decoding. This path accepts headers up to 8 MiB, requires a known EOS bound and refuses detected multiplexing or chained stream resets. Independent Xiph libvorbisfile reference samples cover very short, single-page, multiple-page, cropped and positive-origin cases. An installed ffmpeg decodes Opus, AAC and ALAC when the native decoder cannot open them. Opus pre-skip and end trimming are checked against a real fixture. [`PcmTrack`](../../crates/aede-core/src/playback/stream.rs) owns this progressive decoding, applies a caller-supplied DSP function, rejects non-finite processed samples and returns complete blocks with both interleaved samples and `f32le` bytes. Its [`PcmStreamFormat`](../../crates/aede-core/src/playback/format.rs) is a compatibility alias for `aede-dsp::PcmFormat` and provides the rate and channel count independently of a device. No audio file is modified.

`aede play <selection>` streams files from this decoder through the DSP into the local audio output. A selection can be one file, a folder (recursively traversed in name order), an M3U/M3U8 file, a saved collection, or a scanned artist, album or track name. M3U entries play in written order, including repeats; relative paths resolve beside the playlist, while remote URLs and missing files are refused. A collection evaluates its saved query against the current catalog at playback time and follows catalog order. A bare collection name works when no music name matches; `collection:<name>` selects it explicitly when names overlap. An artist plays releases credited to them as album artist in year order, with each release's tracks in disc and track order. Matching album editions and tracks with the same title are all played in deterministic order; several matching artists require a fuller name. A file, folder or M3U does not need a catalog. The playback label shows the album title (or parent folder when uncatalogued), an em dash, and the filename without its extension so the track number remains visible. Each file playback that reaches at least one millisecond of audio records a listen in `user.json`; a stream that fails after producing audio records an incomplete listen. A zero-duration or immediately skipped selection does not increment the all-time count. `aede history` can show direct file plays even before a catalog exists. `aede played` remains available for manually recording listening done in another player.

On macOS, Windows and glibc Linux, the CLI first tries CPAL with a supported floating-point configuration matching the channel count, then a supported integer configuration; it converts the decoded rate when needed. One device stream and a bounded PCM queue span naturally advancing tracks of the same format; the callback keeps samples in order across block boundaries. History writes run on a separate thread so they cannot hold up the next decode. The output restarts at a format change or after Next, Previous or Stop. If no compatible floating-point or integer device output is available, the CLI falls back to ffplay. The static musl build uses ffplay; building the glibc Linux backend needs ALSA development headers. Set `AEDE_AUDIO_BACKEND=ffplay` to choose the fallback, or `AEDE_AUDIO_BACKEND=native` to require CPAL and receive an error if it is unavailable. ffmpeg is additionally required for Opus and M4A fallback; Vorbis does not require it. A real output-format change closes input and drains the previous output while polling transport controls. The native stream stays alive for a 100 ms host-buffer allowance measured in active, unpaused time. This allowance is an estimate, not a device acknowledgement. Native drain fails explicitly after five active seconds without consumed-frame progress; ffplay supplies no comparable progress counter and has no guessed timeout. Tests check exact PCM concatenation in the ffplay path and callback continuity in the native path. A physical gap can still occur if file opening or decoding starves the bounded queue; there is no hardware loopback test yet. A different source and device rate uses the stateful converter before the normal DSP stages. ReplayGain or Opus R128 normalization defaults to album gain when the selection resolves to catalogued releases. An album gain tag applies the same adjustment to its tracks, preserving their relative levels while aligning albums. Direct files, folders, M3U playlists, collections, artist selections and track selections default to track gain. `--normalize off|track|album` overrides that choice. An exact-scope ReplayGain or Opus R128 tag takes priority. When it is absent, track mode reuses a current FlacCompagnon LUFS/true-peak report or a valid cached source measurement. Missing values are measured from source PCM during playback for a later listen; they never trigger a full-file decode before the first sound. Track mode can use an other-scope tag when no measurement is ready. Album mode accepts a complete set of album tags or a valid ordered-programme cache. With neither, it keeps the decoded level uniformly across the programme and captures one album measurement during an uninterrupted complete sequence. It never substitutes per-track LUFS for album loudness or changes the album gain mid-programme. The target is -18 LUFS. Silent or unsupported-layout files keep their decoded level unless a usable tag exists. For Opus, the decoder applies the ID header output gain first and the selected R128 comment gain is added afterward; a fixture with nonzero header gain verifies this order. The CLI reports both requested and applied gain when sample headroom reduces normalization. A final hard ceiling reports any unexpected out-of-range samples. Seeking remains future work.

For each selected gain, `aede-dsp::gain_with_headroom_db` limits the applied gain to `min(requested gain, -20 log10(source peak))` dB when a positive ReplayGain peak or measured true peak is available. A missing peak is treated as 1.0, so positive gain is withheld rather than risk clipping; a declared zero peak leaves the requested gain intact. This calculation preserves the chosen album or track scope but can fall short of the -18 LUFS target. The shared `Dsp::process_for_output` call applies gain, then uses `aede-dsp::protect_output` to leave samples in [-1, 1] untouched and hard-clamp any out-of-range sample, reporting the count. This is an explicit emergency safety ceiling for inaccurate tags, codec overshoot and files without gain metadata; it is not a transparent limiter. It does not measure inter-sample true peaks, so it does not guarantee a true-peak ceiling. Current, attributed FlacCompagnon true-peak data or a fresh ebur128 measurement can avoid the conservative missing-peak assumption. The future server audio route must apply the same headroom plan and final guard before transmitting PCM.

FlacCompagnon's in-process and imported reports retain integrated track LUFS and true peak. Playback accepts those values only when the source is FlacCompagnon, analysis succeeded, and file size and modification time still match. `ReadyNormalization` inspects only the current track, or the current album's lightweight tags/identities, and chooses known gains without predecoding a selection. `PcmTrack::read_block_observed` supplies original source PCM before downmix, rate conversion, gain and EQ to the optional source meter. This reuses the playback decode and leaves the chosen gain fixed. Only complete decoded tracks or uninterrupted complete programmes are published; skips, unexpected meter failures and source changes discard pending results. Silence or an unsupported source layout records an unavailable result only after the same complete-decoding and identity checks, avoiding repeated futile measurement attempts. An interrupted unknown album can begin a fresh capture when playback returns to its first track, while its session gain stays fixed. A separate playback record worker merges the new measurements under the data-folder lock, outside the decode loop, alongside the unchanged listening-history rule. Preparing or caching loudness creates no listening event. Album measurement combines gated programme energies rather than averaging track LUFS. Consecutive matching formats share a programme meter; a format change combines separate gated histories, so 400 ms windows do not cross that boundary. Cached loudness method version 4 invalidates earlier Aède-derived measurements with potentially underestimated final true peaks, mislabeled high-rate sample peaks or incorrect Vorbis frame bounds; imported FlacCompagnon reports are retained. Finite measurements resolve the interpolation tail with a separate peak-only, zero-extended meter; no silent frames enter loudness gating, playback or frame counters. Snapshots leave capture state unchanged. Integrated LUFS remains available at those rates, while measured true peak is unknown. The explicit core `plan_normalization` preparation remains a blocking API for offline work, not the interactive playback path. No audio file or tag is changed.

The CLI calls `PcmTrack::read_block_observed` with its source-loudness observer and a bypass processing callback, then feeds its decoded/downmixed samples to the shared [`PcmSession`](../../crates/aede-core/src/playback/session.rs). The session applies rate conversion, gain, tone and the final guard, returning PCM and output spans attributed to individual playback tokens. A future server playback handler should use the same decoder/session, send format metadata followed by processed audio to the requesting device, and discard the session when that client disconnects. The local output session does not belong in that path: the remote device owns playback and its clock. Both CLI and server playback must record a `Play` through the shared `UserData::record_play` rule, using the same path-based track reference, all-time count and completed/incomplete semantics. The existing administrative history route records explicit events but does not observe playback automatically; the future server audio route needs a playback acknowledgement or equivalent evidence before marking a remote listen complete. The transport framing, encoding, client buffer limits, authentication and owner-scoped authorization must be designed with the remote API before a playback route is exposed. The current loopback server has no playback route and does not transmit audio.

The CLI also reports the active downmix, rate conversion, normalization, tone, output guard, output format and optional spectrum display for each track. It reports normalization headroom withheld from the requested gain and tone preamp reserve, and explicitly says that dynamic gain reduction is unavailable without a limiter. `aede-dsp::OutputMeter` measures the processed PCM submitted to the sink, after the sample guard and before any integer dither or device conversion. Its summary separates pre-guard sample peak, submitted sample peak, estimated true peak and guard intervention count. `ebur128` oversamples below 192 kHz; at 192 kHz and above, or after a meter failure, true peak is marked unavailable. None of these values are a measurement of the device's analogue output or a guaranteed true-peak ceiling. The spectrum visualizer is a terminal display and does not alter audio.

For native CPAL playback, Aède prefers a floating-point device format with the required channel count, then an integer format. Format preference is ranked before rate distance: the decoded rate is kept when the preferred format supports it, otherwise the nearest rate in that format is selected, even if another format supports the source rate. A differing selected rate uses `aede-dsp::RateConverter` on the decoding thread before gain, tone and the final output guard. The converter is stateful across compatible consecutive source tracks, trims its startup delay once, and emits its tail only at the end of that compatible group. It does not run in the CPAL callback. ffplay continues to accept the decoded rate directly. When the selected device format is signed or unsigned 8-, 16-, 24- or 32-bit PCM, the CPAL callback uses `aede-dsp::TpdfQuantizer` after the final output guard. Its state survives callback blocks and naturally advancing tracks on the same stream; a callback underrun writes exact digital silence. Floating-point output bypasses quantization and dither. ffplay still receives `f32le` and handles any later conversion itself. Aède does not claim bit-perfect integer output from its decoded `f32` path. A remote audio route can make its own rate choice and feed decoded/downmixed PCM to the same `PcmSession`. The standalone `PcmTrack::set_output_rate` API still finalizes one file independently; a queue that needs continuous joins uses the session instead.

`--bass DB` and `--treble DB` optionally apply broad low and high shelf filters, each accepting -12 to +12 dB and defaulting to zero. Supplying zero, or omitting the options on the next invocation, restores exact flat bypass. The 120 Hz and 4 kHz shelf midpoints move below Nyquist for low-rate sources. The `aede-dsp` filters use separate per-channel state and retain it across PCM blocks. For playback, `ToneControls::safe_preamp_db` subtracts the sum of positive shelf gains from the already peak-capped normalization gain; cuts need no extra preamp. This conservative reserve can make playback quieter than the -18 LUFS target. The final sample guard still catches filter transients and bad peak estimates; no true-peak ceiling is promised. A future server playback handler must apply the same tone settings, preamp rule and output guard while using its own network transport.

For known native multichannel sources, `PcmTrack::open_stereo` preserves the source speaker mask and supplies a stereo downmix to the local player. Conventional 2.1, 3.0, 3.1, quad, 4.0, 5.0, 5.1, 7.0 and 7.1 layouts are recognized; unrecognized layouts are refused rather than mapped by channel count. The matrix uses center/surround coefficients based on the ITU-R BS.775 stereo example and omits LFE. The 7.1 side and rear pairs use -6 dB each. A conservative row-sum scale prevents clipping when all contributing channels are coherent at full scale, though an isolated channel can play more quietly. The final output guard still applies. Mono and stereo pass through without channel remapping. The shared PCM format exposes source positions to a future server handler, which can choose passthrough or the same downmix for the client device. The FFmpeg decoder fallback currently cannot confirm a multichannel output mask and therefore does not downmix such streams. ffplay receives explicit layout names for raw PCM rather than a channel count that could be misinterpreted.

On a macOS or Linux terminal, playback accepts single-key controls without Enter: Space pauses or resumes, `n` or Right skips to the next track, `p` or Left goes to the previous track (or restarts the current track when more than three seconds have played), and `q` stops playback and returns to the shell. Pause suspends the selected output as well as the PCM feed. At the end of a selection, Next returns to the shell. In noninteractive use, playback runs through the selection without waiting for keyboard input. Terminal controls are not yet available on Windows.

During interactive terminal playback, twenty-four vertical bars display smoothed frequency-band levels computed from the PCM sent to the local output. The bars expand to fill the terminal width, and the width is checked periodically so they follow a window resize. The renderer leaves the last column unused to avoid wrapping. On a very narrow terminal, it removes gaps and then combines adjacent frequency bands so the display still fits. Half-height tips soften the movement without block-shaped cells. The display refreshes at most twenty times per second, clears itself at the end of a track and is absent when output is redirected. It is an approximate view of the samples entering the output, which can buffer audio before it reaches the device; it is not a measurement of the device's current output. The analysis does not change the samples or their playback gain.

The shared decoder dependency used for other native codecs skips corrupt packets, so this playback path cannot report every damaged frame. The Vorbis wrapper reports codec errors, missing EOS bounds, early PCM ending and detected chained resets, and refuses an EOS that discards more than the final packet can supply. Symphonia can still skip corrupt Ogg pages, and malformed trailing bytes or an incomplete second link after a valid EOS can look like ordinary EOF. These completion checks do not validate every physical byte. It also does not verify FLAC decoded-audio MD5. A future integrity-aware decoder must make those results explicit; successful playback decoding alone must not be treated as an integrity verdict.

The in-memory [`playback` queue](../../crates/aede-core/src/playback.rs) now accepts the same ordered track IDs as playlist rendering. It provides the basic transport state, repeat modes, and seeded uniform shuffle. Other shuffle styles remain designs for later work.

### Native output callback and diagnostics

The native CLI output uses a preallocated `rtrb` single-producer/single-consumer ring of `f32` PCM. Its capacity is 500 ms at the selected device rate, rounded up to whole frames; it does not depend on decoder block sizes. One producer and one callback consumer own the two handles. A full queue accepts only an available prefix of complete frames or returns `WouldBlock`, so the driver can poll transport controls before retrying. The native sink receives the DSP's original sample slice directly instead of decoding transport bytes into a new vector. The shared blocks still carry `f32le` for byte transports; ffplay retains arbitrary partial-byte write behavior.

The rendering callback reads a bounded chunk, maps it to the device format and updates atomic counters once per callback. It performs no Aède allocation/deallocation, logging, I/O or mutex operation. Integer quantizer state remains continuous across chunks and tracks; queue shortages use exact digital silence without advancing dither. Only complete frames are consumed, so a shortage cannot shift channel positions. Startup before the first submitted frame and normal silence after input closure are excluded from queue-shortage counters. Closing input retains queued samples for draining; a skip discards the old stream and ring.

CPAL's error callback records fixed-size atomic status only. The driver reports route changes and refused real-time scheduling as warnings while playback continues; host xruns are counted separately. Permanent device/stream failures stop writes and drain with an explicit error. At stream completion or abort, diagnostics report callback-consumed frames, current/capacity queue frames, missing frames/callbacks and host xruns. Queue consumption means handoff to the host, not physical device acknowledgement; the existing 100 ms final buffer allowance remains an approximation. These rules constrain Aède's own callback work and do not guarantee the scheduling or implementation of every OS/CPAL backend. See [rtrb's contract](https://docs.rs/rtrb/0.4.0/rtrb/) and [PortAudio's callback guidance](https://portaudio.com/docs/v19-doxydocs/writing_a_callback.html).

### Continuous processing and track attribution

`PcmSession` retains one `aede-dsp::Dsp` and optional `RateConverter` across naturally advancing tracks with the same PCM input format after downmix (rate, channels and speaker positions), selected output rate and tone settings. Codec/container changes do not reset the filters. A gain change is applied at its track's output boundary without recreating tone filters. The processing order stays downmix → rate conversion → gain → tone → output guard.

A source EOF seals a track boundary without flushing the converter. Output boundaries use `ceil(cumulative source frames × output rate / input rate)`, so rounding is shared by the whole group. The converter may receive the following track while emitting delayed frames of its predecessor. Each output span carries the original token, correct gain, process statistics and a unique completion marker; a very short or empty track may complete in a later block or with no new samples. This attribution also keeps the listening identity and duration correct.

At selection end or incompatible input/tone/output format, flush the compatible group exactly once, then create fresh processing state if needed. Next, Previous, Stop and an error during a track discard the converter, tone state and queued output; they do not play the interrupted filter tail. A next file that cannot be prepared follows a completed source: the previous group's valid tail is delivered before its error is reported. Previous during the final output drain can restart playback with fresh state.

Listening counters follow complete frames accepted by the output, including a prefix accepted before a partial write fails. Aggregate peak/guard diagnostics are retained only for fully submitted spans; interrupted partial-span statistics are explicitly unavailable. Completed loudness updates wait for their attributed output completion before asynchronous publication. The last listen remains incomplete if output drain is interrupted. As before, submitted frames are not acknowledgements of physical device consumption; exact consumed-frame history remains future work.

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
