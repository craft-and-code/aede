# Identification (M1)

What MusicBrainz brings, beyond what is already planned.

## Who is the same person

The catalog builds one artist per **normalised name**, so a library holding
`Ozzy Osbourne` on one album and `O. Osbourne` on another holds two musicians.
Every count, every listing and every page is wrong by one.

The problem has two halves, and only one of them has an exact answer.

**Files tagged by Picard carry `MUSICBRAINZ_ARTISTID`**, and two spellings under
one identifier are not a resemblance to be judged: they are the same artist,
said so by the only authority there is on the question. That half is **done**,
in `model::identity`, and it needs no heuristic at all.

Three rules keep it safe:

- a file speaks only where it names **exactly one** artist and carries
  **exactly one** identifier. A tag naming two arrives as a single string that
  `split_artists` cuts in two, while the identifiers arrive as their own list in
  an order nothing guarantees to match — pairing them by position would attach
  an identifier to whichever name happened to sort first;
- the spelling that **names the most tracks** survives, ties broken by the
  normalised key so that two runs over one library answer alike. It is derived
  from the shelf rather than chosen, and it is always a key that already
  existed, so merging can only shrink the set of keys: whatever you filed under
  the survivor keeps pointing at it;
- the decision is taken **before anything is interned**, because which spelling
  wins is a property of the library and cannot be settled file by file.

The merge is **shown**, not merely done — a decision the reader cannot see is
one they cannot check:

```
Ozzy Osbourne

  MusicBrainz: 8cd2b0ed-…
  also spelled o osbourne in your tags, merged by that identifier
  performing: 13 albums · 141 tracks
```

And a name that reaches two artists is refused rather than arbitrated, with the
identifiers on the lines, since two similar names under *different* identifiers
are genuinely two people:

```
$ aede artist ozzy
Error: "ozzy" matches 2 artists, and a page is about exactly one.
Name the one you mean:
	Ozzy Osbourne (141 tracks, 13 albums) · musicbrainz 8cd2b0ed-…
	Ozzy Tribute (9 tracks, 1 album) · musicbrainz 4f1c2a90-…
```

## Who is not a person at all

Before two spellings can be one artist, the shelf has to stop inventing artists
that nobody is. `aede artist ozzy` once refused between five names:

```
	Ozzy Osbourne (152 tracks, 16 albums) · musicbrainz 8aa5b65a-…
	Judas Priest & Ozzy Osbourne (1 track, 1 album)
	Ozzy Osbourne & Travis Scott (1 track, 1 album)
	Rob Zombie & Ozzy Osbourne (1 track, 1 album)
	Ozzy Osbourne w/Therapy? (1 track, 1 album)
```

Four of those are collaborations, not musicians — and every track they held was
missing from the page of the man who plays on it. Making the command prefer the
biggest match would hide them; they would still be there, still holding those
tracks. They have to stop being created.

**`&` can never be split out of a string.** `Rob Zombie & Ozzy Osbourne` is two
artists and `Kool & the Gang` is one, and nothing in either string says which —
both of those are on the shelf this was measured on. So the string is the wrong
place to look, and there are two right ones.

**The tag written for the question.** `ARTISTS` and `ALBUMARTISTS` hold **one
value per artist**, which is why taggers write them, and all four files above
carried them (`ARTISTS=Rob Zombie;Ozzy Osbourne`). They are read first, and
`ARTIST`/`ALBUMARTIST` with its guessed separators only where none exists.
`Kool & the Gang` carries `ARTISTS=Kool & the Gang`, one value, and stays one
band.

**The rest of the same tag.** `ARTISTS` says nothing about `PERFORMER`, and a
real file of *War Pigs (charity version)* carries:

```
PERFORMER=Ozzy Osbourne; Judas Priest; Judas Priest & Ozzy Osbourne
```

Three values, two musicians. The third is made of the other two and nothing
else, which makes it the same credit written a second way — the list answers a
question the string could not. A value that consumes **two or more** of the
others of its tag, leaving nothing but joining words behind, is dropped;
everybody it named is still named. Two rather than one, because a single match
would mean any credit containing another restates it, and `Therapy?` sitting
beside `Ozzy Osbourne w/Therapy?` would delete the collaboration instead of the
duplicate.

With the four gone there is one Osbourne on the shelf, so `aede artist Osbourne`
and `aede artist "Ozzy Osbourne"` answer with the same page, and the
collaborators are where they were always useful:

```
$ aede artist "Ozzy Osbourne" --with "Rob Zombie"
```

What this cannot do is split a joined credit in a file that carries neither
`ARTISTS` nor a list naming the parts. Nothing can, from outside: `aede file
"<a track>"` shows whether yours carry it.

**The other half is files that never met MusicBrainz** — old rips, downloads,
a friend's drive. Nothing outside can help: nobody on earth knows that your
`O. Osbourne` is Ozzy except you. So Aède asks instead of guessing:

```sh
aede merge "O. Osbourne" "Ozzy Osbourne"   # the first gives way to the second
aede merge --list                          # what has been said, and whether it is in effect
aede merge --list osbourne                 # narrowed, on either side of the arrow
aede merge --forget "O. Osbourne"          # take it back
```

The statement is kept in `user.json` — the file that holds what *you* say —
with both names normalised, because that is the form every merge in this
program is decided on. **Nothing in your files changes**, ever: not the audio,
not the tags, not what any source said. What changes is how the shelf is read.

It takes effect on the **next scan**, and the listing says so per row rather
than leaving you to guess:

```
Spelling      Filed as        Said    State
o osbourne    ozzy osbourne  2m ago   waiting for a scan
```

The reason is not a shortcut: the spelling a track is filed under is decided as
the artist is interned, and there is no later moment at which one row can become
another without rebuilding everything that points at it. An incremental scan
re-reads nothing it does not have to, so this costs seconds.

One statement is refused. Where **both** spellings carry a MusicBrainz
identifier and the two differ, they are two people, said so by the only
authority on the question, and the disagreement is with MusicBrainz rather than
with your shelf:

```
$ aede merge "Angus Young" "Neil Young"
Error: MusicBrainz says these are two people, so they were not merged:
	Angus Young · musicbrainz 1eaa5f1a-…
	Neil Young · musicbrainz 24f1766e-…
	If that is wrong, it is wrong at the source — correcting it there fixes it
	for everybody, and a later fetch brings the correction back.
```

### What doctor suggests, and never does

Finding the pairs by hand on a shelf of forty thousand tracks is not a plan, so
`aede doctor` names them — and applies none of them, because the whole point is
that it cannot know:

```
info   possibly one artist
       "O. Osbourne" (9 tracks) and "Ozzy Osbourne" (141 tracks): one is the
       other with a first name reduced to its initial. If they are,
       aede merge "O. Osbourne" "Ozzy Osbourne" says so — nothing in your
       files changes
       /music/Ozzy Osbourne/Bark at the Moon/01.flac
```

Two shapes are reported, and each has to earn it. **An initialism** — the same
number of words, each identical or a single letter opening the other — stands on
its own, because the abbreviated spelling usually lives on a different album and
there is nothing to corroborate it with. **A dropped first name** — one name's
words ending the other's, whole words and never a substring — is far too loose
alone, so it is only reported when the two are credited on the same release. A
pair whose rows carry different identifiers is never reported at all.

Angus Young and Neil Young survive both rules, which is the point: neither is
the other abbreviated and neither is the other shortened.

A note on a tempting wrong turn: *correcting MusicBrainz* does not help with
either half. The two spellings are in **your files**; there is nothing at the
source to correct. Fixing MusicBrainz is the right move for a different
problem — a wrong release type, a missing date — and Aède prints the address of
the page for exactly that, wherever it shows you a fetched fact.

## Country, formation, membership

There is no widely-used tag for an artist's country of origin — `RELEASECOUNTRY`
exists but that is the country a _release_ came out in, which is a different
question and answers it wrongly (an American pressing of a French band). So this
waits for M1 and comes from the artist entity: its **area**, its **begin and end
dates** (formation and split, with an explicit "ended" flag), and its type
(person, group, orchestra, choir…).

Band membership needs **no new table**, and it no longer needs the one this
paragraph originally named. Written before the attributed layer existed, this
said to use `relation` — an artist-to-artist link, which is what a line-up is.
That would be wrong now: `relation` is rebuilt by the **scan** from your files,
so a line-up written there would be erased by the first scan after the fetch,
which is precisely the fault the attributed layer was built to prevent. A
line-up is what *somebody else says*, so it lives in `ArtistFacts` beside the
area and the formation date, and it rides on the lookup already being made —
`artist-rels` in `ARTIST_INCLUDES`, no extra request.

### A line-up is not an album's data

It is a **dated relation between two artists**: Ozzy Osbourne was in Black
Sabbath from 1968 to 1979 and again from 1997. MusicBrainz holds it on the
artist, never on a record, and it exists for a band whose albums you do not own.
Albums enter only when the two are **crossed**, and that is where the dates pay
for themselves:

```
$ aede artist "Black Sabbath" --members

Played with them

  Name              Relation                          As                          Years
  ────────────────  ────────────────────────────────  ──────────────────────────  ─────────
  Bill Ward         member of band                    drums (drum set), original  1968–1980
  Ozzy Osbourne     member of band                    lead vocals, original       1968–1979
  Tony Iommi        member of band                    guitar, original            1968–
  Ronnie James Dio  member of band                    lead vocals                 1979–1982
```

```
$ aede album "Paranoid"

Paranoid
  Black Sabbath
  1970
  /music/Black Sabbath/Paranoid
  line-up in 1970: Bill Ward · Geezer Butler · Ozzy Osbourne · Tony Iommi
```

The album line is **derived when shown**, never stored. A stored line-up would
be a claim the catalog had stopped being able to justify the moment either side
changed — the same rule the missing-albums list already follows.

### Three things the live answers said that guessing did not

The parser was written from the documented shape and then checked against real
responses for a person and for a band. All three of these were wrong before:

**The direction is not what it looks like.** MusicBrainz states one relation and
returns it on both artists with a `direction`. Both answers — the person's and
the band's — say `"backward"`, because these relationships are defined *from the
musician towards the group*: a backward one is being read from the group's end
and the artist it names is the player. Assuming a person's own record would read
"forward" is reasonable, and wrong, and it does not lose data — it inverts it.
Black Sabbath would have appeared in the list of Ozzy Osbourne's members.

**`member of band` alone is not enough.** Ozzy Osbourne's record holds none at
all — a solo artist is not a band — and every musician who played with him is an
`instrumental supporting musician`: Randy Rhoads on guitar, Bob Daisley on bass.
Reading only the obvious relationship would have shown him an empty line-up
while MusicBrainz plainly holds his band. The two are kept apart rather than
merged, in MusicBrainz's own words, because a founding member and a guitarist
hired for one tour are both on the record and only one of them was in the band.

**An attribute is not always an instrument.** Judas Priest's line-up carries
`["guitar family", "original"]`, where `original` marks an original member. The
field is `attributes` and the column is headed "As", because calling it
`instruments` would put a lie in a column header.

### What a date cannot say

`covers(year)` answers with three values, not two. A membership with no start
date places nobody; one the source says has **ended without saying when** places
nobody either — the end is somewhere, and "somewhere" cannot be compared with a
year. Both come back "don't know", and the album page **counts** them rather
than dropping them in silence:

```
  and 1 musician MusicBrainz does not date closely enough to place
```

Including them would put a musician on a record they may not be on, which is the
one mistake here that matters. Hiding them would be a filter the reader cannot
see, which is the other one.

Two spells in one band stay **two rows**. Folding `1968–1979` and `1997–2017`
into one span would claim Ozzy was in Black Sabbath in 1985.

Then `aede artists --country FR`, and a band page that shows a line-up rather
than a list of names.

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
