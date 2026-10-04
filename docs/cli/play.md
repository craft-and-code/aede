# play — Play local music

play accepts an existing file, recursively ordered folder, M3U/M3U8, saved collection or catalogued name. Use collection:NAME to select a collection explicitly. Relative playlist entries resolve beside the playlist, not the terminal’s working directory. File/folder playback can start without scanning; catalog names and collections need the catalog.

In macOS/Linux terminals and the native Windows console, playback accepts the keys below without Enter. With redirected standard input, interactive controls are disabled and the selection advances automatically using the requested repeat and shuffle modes. Listening history is recorded in personal data. Windows console input is implemented; acceptance on an actual Windows console and audio device remains pending.

Album normalization is the default for a catalogued album; other selections use track normalization toward -18 LUFS. Current FlacCompagnon LUFS/true-peak results, tags or cached measurements can supply gain. Missing loudness is measured during playback, without changing the current track’s level midway. Bass/treble boosts reserve headroom. CPAL uses a compatible native output where available, otherwise ffplay; Opus/AAC/ALAC decoding can need ffmpeg. Linux release archives require ffplay. The meter and output guard report sample overloads; there is no dynamic limiter or guarantee of a true-peak ceiling. Hardware gapless behavior remains unmeasured.

AEDE_AUDIO_BACKEND chooses local output: unset tries native then ffplay; native requires native output and refuses instead of falling back; ffplay explicitly selects that program. Other values are refused. For example, on macOS/Linux: `AEDE_AUDIO_BACKEND=ffplay aede play "/path/to/track.flac"`. On PowerShell, set `$env:AEDE_AUDIO_BACKEND = "ffplay"` before running play. This selects the output backend, not the decoder or DSP quality.

## Syntax and arguments

```text
aede play <file|folder|m3u|collection|artist|album|track> [--seek TIME] [--repeat off|one|all] [--shuffle off|random|smart] [--seed U64] [--lyrics] [--normalize off|track|album] [--bass DB] [--treble DB]
```

One selection; quote names/paths containing spaces. Prefix a saved collection with collection:.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--seek TIME` | Start the first played occurrence at seconds, `mm:ss` or `hh:mm:ss`, with up to three decimal places. Default zero. |
| `--repeat off\|one\|all` | Stop at selection end, repeat the current track, or repeat the selection. Default off. |
| `--shuffle off\|random\|smart` | Keep selection order, use uniform random order, or prefer progressive genre transitions. Default off. Smart mode needs a catalog. |
| `--seed U64` | Reproduce a shuffled order with an unsigned 64-bit seed. Requires random or smart mode. Otherwise a seed is generated and displayed. |
| `--lyrics` | Replace the spectrum with local lyric cues or a four-line untimed preview. Requires terminal output; never downloads lyrics. |
| `--normalize off\|track\|album` | Choose off, track or album loudness normalization. Default album for a catalogued album selection, track for other selections. |
| `--bass DB` | Broad bass shelf, -12 to +12 dB. 0 is flat; positive boosts reserve headroom. |
| `--treble DB` | Broad treble shelf, -12 to +12 dB. 0 is flat; positive boosts reserve headroom. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede play "$HOME/Music/album/01.flac"
aede play "Kind of Blue" --normalize album
aede play collection:Road --bass 2 --treble -1
aede play "$HOME/Music/album/album.m3u" --normalize off
aede play "Kind of Blue" --seek 02:15.500
aede play collection:Road --shuffle random --repeat all
aede play collection:Journey --shuffle smart --seed 42
aede play "Kind of Blue" --lyrics
```

## Terminal controls

Keep the terminal focused; these keys control Aède directly, without Enter. Windows Terminal and the classic console use native key events; PowerShell ISE and other hosts without console input do not provide these controls. Letter commands are case-insensitive. Key releases and unrelated mouse/window events do not trigger transport actions.

| Key | Action |
| --- | --- |
| Space | Pause/resume. |
| `n` or Right | Next occurrence, including when repeat-one is enabled. |
| `p` or Left | Previous occurrence, or restart the current track after three seconds. |
| `[` / `]` | Move ten seconds backward/forward in the current track. |
| `r` | Cycle repeat: off → one → all → off. |
| `z` | Cycle shuffle: off → random → smart → off. Without a catalog, smart is skipped with an explanation. |
| `q`, `s` or Ctrl-C | Stop, save listening history and return to the shell. |

During interactive playback, terminal input has no echo. Aède restores the original input mode after a normal stop or an error. Ctrl-C follows the same orderly stop/history path as `q`; on Windows it is read as a console key rather than an immediate process termination. If playback input cannot be read or the input mode cannot be restored, the command reports the error.

Windows accepts AltGr brackets on French keyboards. Classic-console mouse selection is disabled during playback to prevent it from freezing console I/O and starving audio; the original setting is restored afterward.

Repeat and shuffle can be changed while paused. Changing shuffle retains the current occurrence and the already-played prefix; it reorders only the remaining occurrences. Turning shuffle off restores their original relative order. A mode change does not cut the current audio. Previous follows the actual playing order, including the preceding repeat-all cycle while it is retained.

## Moving within a track

`--seek` applies to the first occurrence actually played, including the first shuffled one. Later tracks and repeats start at zero. Backward movement clamps to zero; movement past EOF ends the current occurrence and follows the active repeat mode. Pause is retained across a move.

The decoder reopens the source and progressively discards the prefix, with bounded working memory. It does not send skipped audio through the DSP, output or listening history. This is sample-based positioning, rounded down to a source frame; the displayed position uses milliseconds. It is not an indexed instant seek: work grows with the target position and slow file/decoder reads can delay a control between cancellation checks. Moving resets queued audio and DSP state.

Multiple moves during one visit create one incomplete listening record, counting only the estimated submitted-and-active audio duration, excluding pauses and skipped prefixes. A partial-track visit never publishes a full-track loudness measurement. Local history reflects submitted playback and active time, not a physical device acknowledgement.

## Repetition

`off` plays every occurrence once and stops. `one` restarts the current occurrence at natural EOF. `all` starts another complete cycle after the final occurrence. Next bypasses repeat-one; Next after the last occurrence stops with repeat off or one, and starts the next cycle with repeat all. Stop always ends the session. Empty audio cannot repeat indefinitely.

With shuffle and repeat-all, each cycle uses a new seed derived from the previous one. Previous can return to the retained preceding cycle; advancing again reuses the already planned forward cycle. No queue, mode or position is persisted when the command exits. The separate history worker queues at most 64 events; persistently slow storage back-pressures playback instead of growing memory indefinitely. This can delay audio and controls until storage advances, especially with extremely short repeated tracks.

## The two random modes

`random` is classic uniform shuffle: a seeded Fisher–Yates permutation gives each selection occurrence the same chance. It deliberately has no genre or artist restrictions. M3U duplicates remain distinct occurrences and are all played once per cycle.

`smart` prefers short steps between styles, then varies artists and albums within those nearby choices. For example, a selection containing suitable recordings can move through classical → Third Stream → jazz → soul/funk → hip-hop → rap. This illustrates musical meeting points rather than a historical timeline: music has multiple influences, and release year alone does not determine its style. [NEC describes Third Stream's classical/jazz meeting point](https://necmusic.edu/on-campus/library/archives-and-special-collections/archival-collections/gunther-schuller/), and [the Library of Congress describes several hip-hop influences](https://www.loc.gov/collections/songs-of-america/articles-and-essays/musical-styles/popular-songs-of-the-day/hip-hop-rap/).

The planner uses catalog genres and credits, not audio analysis, BPM, an AI service or network access. Track genres take precedence; a compilation's genre union does not make all its tracks stylistically alike. Scan first and retain useful track tags. Files absent from an existing catalog remain in the selection with unknown metadata. With no usable genres, smart falls back to uniform order and reports the lack of evidence.

These modes control `aede play`. Subsonic clients such as Submariner handle their own repeat, seek and shuffle controls; their ordinary shuffle button does not request Aède's smart order. Native PCM v1 additionally accepts finite ordered queues for continuous compatible joins. Its optional [interactive extension](../server/playback.md#interactive-queues-seeking-and-persistent-resume) adds seeking, future-tail edits and private checkpoint resume, with separate client commands; native repeat/shuffle remain client decisions. These CLI controls do not remotely control that transport.

Every occurrence is retained. If a bridge is missing, already played or exhausted, smart still plays the remaining styles and reports the planned breaks, then names a discontinuity when that transition starts. It also reports transitions it cannot assess because genre tags are missing. This is a bounded heuristic, so a reported break does not prove that no better global route exists. It never invents intermediate tracks or filters the selection to one genre.

## Expert — smart shuffle version 1

### Metadata and genre graph

Profiles retain up to eight canonical genre names, eight main/featured artists and four labels per occurrence, plus track identity, release identity and release year. Direct track genre links win over release genres; release fallback and release co-occurrence evidence exclude compilations. Normalization handles case, accents and punctuation, with conservative aliases such as R&B/rhythm and blues, hiphop/hip-hop and classique/classical. A complete-word compound can connect to its base genre; arbitrary substrings are not matches.

An undirected affinity backbone contains 69 genre nodes and 96 edges, with costs from 90 to 280. These links and costs are listening design choices, not scientifically measured similarities or ancestry claims. The graph adds frequent custom selected genres up to 512 total nodes; omitted or capped metadata is reported. Raw and normalized genre labels above 256 UTF-8 bytes are omitted and counted as limited metadata; a rejected direct tag does not acquire a release's genre union instead. Canonical labels are cached and shared, and bounded label profiles are prepared once per selected release.

Local co-occurrence evidence supplements this backbone: direct co-tags use base cost 140, release contexts 220 and artist contexts 240. Repeated identical genre sets within one release count once. Release/artist associations need at least two contexts and cannot create shortcuts between established styles whose prior distance exceeds 300. For a pair observed together `c` times, with frequencies `fA` and `fB`, the integer Dice score is `s = floor(1000 × 2c / (fA + fB))`; an edge costs `base + floor(180 × (1000 − s) / 1000)`. The shortest available edge wins.

Shortest-path distances use Dijkstra and cap at 1000; disconnected genres have distance 1000, and missing genres yield an unknown result. For multi-genre tracks, the distance is one quarter of three times the closest genre pair plus the mean of both directional nearest-genre distances. This allows a hybrid recording to act as a bridge without treating one shared tag as complete similarity.

### Candidate selection and weighting

Each step examines at most 96 candidates: up to six from each of the twelve nearest active genre pools, eight without genres and sixteen from the global pool, using seeded priorities. When the current track has no genres, the pool comes from the remaining selection. The nearest known candidate defines a local band of its distance plus 60, capped at the preferred maximum step of 300. If no preferred step is available, broader candidates remain eligible and the discontinuity is recorded.

The base integer weight is `(1100 − distance)²`, with unknown distance treated as 600 for weighting only. Shared labels add 10%; a release-year gap below fifty years adds up to 10%; known artist collaborations add 12.5%. These adjustments cannot make a candidate outside the chosen genre band eligible.

Recent artists over five occurrences and albums over three receive soft, age-dependent penalties. A recently used track identity has its weight divided by sixteen, preserving intentional playlist duplicates while tending to space them. One-step lookahead also divides a candidate's weight by sixteen when the remaining genre pools have no preferred onward step. All weights stay positive: no artist, album or occurrence is excluded.

### Reproducibility and limits

Both modes use SplitMix64 and integer rejection sampling; Fisher–Yates builds the initial priorities. The same selection, catalog metadata, seed and algorithm version reproduce the order across platforms. A repeat-all cycle increments the seed modulo 2⁶⁴ by `0x9e3779b97f4a7c15`; smart considers the preceding final occurrence when choosing its next opening. Retagging, changing the selection, interactive interventions or a future algorithm version can change the result.

The graph is bounded and candidate work does not build an all-track pairwise distance matrix. The planner prepares metadata before audio and reuses it across cycles. Shortest paths can traverse genre nodes with no selected recordings; these nodes influence distance but never add tracks to the selection. Artist/album memory covers the current plan; a repeat boundary supplies only the previous cycle's final occurrence. The planner favors coherent neighborhoods rather than forcing a timed drift or finding an optimal route through every style. Custom or inconsistent tags, unavailable bridge recordings and bounded candidate/lookahead sampling limit what it can infer. It computes an order only; it does not alter files, tags or stored catalog metadata.

## Terminal spectrum

Terminal playback shows a Retro spectrum with twelve broad, segmented bands, ordered from low to high frequencies. It groups the existing 24 analysis bands for display; it does not change the analysis or the audio. Each column fills from the bottom, with green lower segments, yellow upper segments and red top segments. A separate peak marker falls more slowly after the current level drops.

Band widths follow the terminal width. Narrow terminals combine bands rather than wrapping the display. `--no-color` or `NO_COLOR` keeps the same blocks and peak markers in monochrome. Redirected output has no animated spectrum; `--lyrics` replaces it with lyric cues.

The colors describe display height, not clipping or a calibrated level threshold. Use the separate output-meter and guard reports to inspect overloads. The visualizer adds no equalization or other audio processing.

## Lyrics during playback

`--lyrics` prints the active LRC cue when it changes, in place of the spectrum. Equal timestamps appear together, in source order, with up to four lines per cue. A timestamped blank clears the active words. Plain lyrics produce a four-line preview once per occurrence; unavailable or invalid lyrics give a diagnostic without stopping the music. Long lines are clipped to the terminal width. Cue output scrolls with playback; it is a compact display, not a full karaoke screen.

A nonempty lyrics tag takes precedence over an adjacent `.lrc`. Reads are bounded to 256 KiB of source text and 1 MiB after timestamp expansion. Existing catalog evidence must still match the audio; files played directly are read afresh. No request to LRCLIB or other service occurs. Use [fetch --lyrics](fetch.md) separately to retrieve missing words, or [track --lyrics](track.md) for a complete static listing. Redirected output refuses `play --lyrics`; playback without this option remains available.

With native output, cues follow frames consumed by the CPAL callback, including the current seek offset. They stay frozen during pause and reset when restarting, seeking or skipping. Compatible joins and natural repeats retain the output clock while starting a new lyric occurrence. This avoids showing the next track simply because its decoder has started preparing it. Lyric lookahead holds at most 64 occurrences; if very short tracks reach that bound before the resampler emits them, Aède flushes the processing group and waits for output consumption. The output stream stays open, but filter tails and rate-conversion rounding restart at that exceptional boundary. Device/host latency is not measured, so this does not promise alignment with the physical sound at the DAC. With ffplay, elapsed active playback supplies an explicitly announced estimate; buffering and output stalls can reduce its accuracy.

The [native lyrics API](../server/playback.md#lyrics-and-the-client-clock) gives Phémios or another authorized client a separate timed text resource. The client follows its own audio presentation position, pauses and seeks. Lyrics are not embedded in the PCM packets; this API does not enable the separate Subsonic lyrics methods.

## FLAC audio integrity during playback

For FLAC sources, Aède compares the decoded audio with the MD5 stored in STREAMINFO during the same progressive decode pass. The comparison uses original integer samples before float conversion, normalization, downmix, resampling or tone controls. It does not change the file or its sound, predecode the whole track, or run FlacCompagnon's full analysis again. A previous analysis result does not skip the check of the currently opened source.

Verification is conclusive only when the complete source reaches EOF. Stopping or skipping to another track before EOF leaves it pending. Seeking progressively decodes the discarded prefix, so reaching EOF after a seek can still verify the whole source; skipped audio remains absent from listening history and loudness capture. An all-zero stored MD5 means that no signature is available: playback is allowed without a verified verdict. A mismatch returns a decode diagnostic, stops the queue and prevents natural completion or saving a newly captured full-track loudness result. Earlier completed listens remain valid, and acknowledged/submitted listening to the failing track is incomplete. No integrity verdict is written to the catalog.

This checks decoded-audio consistency, not mastering quality or authenticity. FLAC file transfers to Subsonic clients do not decode audio on the server, so this playback check applies to local playback and native remote PCM. See [integrity](../integrity.md#flac-audio-md5-during-playback) for the exact scope and codec limits.

## Result and errors

The terminal names the album and numbered filename as playback advances, shows the Retro spectrum (or lyric cues with `--lyrics`), and reports active DSP stages, normalization/tone headroom and output-meter values. The meter observes guarded PCM submitted to local output before dither/device conversion, not sound measured at the loudspeaker. Read normalization source and guard intervention reports when interpreting level changes. Completion follows the selected repeat mode. A missing decoder/output device, unsupported channel layout, malformed playlist or file decode/output failure returns a diagnostic; completed listening records may already be saved. A device shortage/underrun is an output problem, distinct from a catalog tag issue. See the DSP guide for measurement limits.

## Related reading

[track](track.md), [analyze](analyze.md), [history](history.md).

Detailed existing guide: [design/playback.md](../design/playback.md).
