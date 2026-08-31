# Identification (M1)

What MusicBrainz brings, beyond what is already planned.

## Country, formation, membership

There is no widely-used tag for an artist's country of origin — `RELEASECOUNTRY`
exists but that is the country a _release_ came out in, which is a different
question and answers it wrongly (an American pressing of a French band). So this
waits for M1 and comes from the artist entity: its **area**, its **begin and end
dates** (formation and split, with an explicit "ended" flag), and its type
(person, group, orchestra, choir…).

Band membership needs **no new table**: it is an artist-to-artist link, and the
`relation` table exists for exactly that. MusicBrainz's "member of band"
relationship carries begin and end dates and an instrument, so the one model
change is a **dated relation** — an optional period on a link. Which is worth
doing carefully, because "who was in the band in 1979" is a question the graph
should be able to answer, and dated links are how.

Then `aede artists --country FR`, `aede artist "Iron Maiden" --members`, and a
band page that shows a line-up rather than a list of names.

## Editions: single, EP, live, remaster, deluxe

Half of this is cleaner than expected and half is messier.

**Clean:** MusicBrainz release _groups_ carry a primary type — Album, Single,
EP, Broadcast, Other — and secondary types: Compilation, Soundtrack, Live,
Remix, DJ-mix, Demo, Mixtape, Spokenword, Interview, Audiobook, Audio drama,
Field recording. That is the vocabulary, it is stable, and it is exactly what an
interface needs for its icons.

**Messy:** _remaster_ and _deluxe edition_ are **not types.** MusicBrainz keeps
a remaster in the same release group as the original and distinguishes it at
release level — by date, label, catalogue number, barcode, and a disambiguation
comment such as "2011 remaster" — plus an explicit release-to-release "remaster
of" link. So a remaster is not a category to display; it is _another release of
the same thing_, which is a better model anyway and one this catalog can already
express.

Partly available before M1: Picard writes `RELEASETYPE` and `RELEASESTATUS`
into the tags, so a well-tagged library already knows its EPs from its albums.
Titles can be mined for "(Deluxe Edition)" and "Remastered 2011" — but a guess
from a title is a guess, and this project records where a fact came from
(read from a tag, inferred by a rule, fetched from MusicBrainz) rather than
flattening the three into one field that looks equally certain.

## What is missing from the shelf

The completeness report — the thing worth building:

```
Collection completeness
──────────────────────
Pink Floyd
Albums:
██████████████░░░ 82%
✓ The Dark Side of the Moon
✓ Wish You Were Here
✓ Animals
✗ The Final Cut
✗ The Division Bell
```

Three things decide whether this is useful or infuriating:

- **Compare release groups, not releases.** Otherwise every reissue of _Animals_
  counts as an album you are missing, and the figure is noise.
- **Say what the percentage is of.** 82% of studio albums is a fact; 82% of
  everything MusicBrainz holds, bootlegs and DJ-mixes included, is a number that
  will never reach 100 and therefore means nothing. The secondary types are the
  filter, and the heading must name it.
- **An absence is not a defect.** Nobody wants every Frank Zappa release. The
  report answers a question; it does not nag, and it belongs beside `doctor`
  rather than inside it.

Discogs is the obvious second source and adds real depth — pressings, matrix
numbers, plants, editions — but its API terms are far more restrictive than
MusicBrainz's CC0: authentication, rate limits, no commercial use without
permission, no caching beyond serving the immediate user. Worth it for editions
of physical media, not worth it as the primary source, and not worth it at all
before MusicBrainz is working.

**Done, without any of the above:** completeness at the level of one album needs
no network at all, because the files say what they belong to. `doctor` reported
an incomplete album by finding _gaps_ — 2 missing between 1 and 3 — which left
two shapes invisible, and both are the ordinary ones:

- an album **cut short at the end**: truncated after track 9 of 12, there is
  nothing between 1 and 9 left to be missing, and that is exactly what an
  interrupted rip looks like. `TRACKTOTAL` answers it.
- a **whole disc absent**: every disc that is there is complete, and the
  numbering says nothing about how many there should be. A four-disc soundtrack
  ripped as three looks perfect until the day it is played. `DISCTOTAL` answers
  it, and so does a hole in the disc numbers themselves.

```
warning  incomplete album  "FINAL FANTASY VII: Original Soundtrack": missing disc 4 of 4
```

With one precaution: before calling a disc missing, it is looked for in the rest
of the library. A set laid out as `Box CD1` beside `Box CD2` — sibling folders
rather than a common parent, which the disc-folder rule does not recognise —
arrives as two releases, and each would otherwise report the other as missing
while it sits right there.

What still waits for MusicBrainz is the _other_ completeness question — which
albums are missing from an artist's discography — because nothing in your files
can know what was released.
