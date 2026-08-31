# Playback (M3)

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
The tags already carry `REPLAYGAIN_*` and, for Opus, `R128_*` — the parsers see
them today and throw them away. M3 reads them, and for files that have none the
decoder can compute EBU R128 and store it exactly as an integrity verdict is
stored: measured once, kept, recomputed only on request. Aède decides what gain
to apply to the stream; the user decides how loud the room is.

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
