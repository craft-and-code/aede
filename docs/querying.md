# Asking questions

## Asking a question

Options compose by AND and by nothing else. That ceiling is what a grammar
lifts: there is no `--genre metal OR --genre jazz`, no "everything except this
label", no "between 1990 and 1999" — and all three come free with one parser.

```sh
aede query "genre:metal year:1990..1999 -label:earache"
aede query "(artist:ozzy OR artist:dio) album.rating:>=4"
aede query "loved played:0" --m3u          # what I love and have never played
aede query "lossless:false size:>50000000" # big, and not lossless
```

Fields: `title`, `artist`, `album`, `albumartist`, `genre`, `label`, `comment`,
`lyrics`, `path`, `codec`, `year`, `duration`, `size`, `bitrate`, `samplerate`,
`lossless`, `compilation`, `played`, and what you wrote — `rating`, `loved`,
`tag`, `note`.

`lyrics:` looks in the words the file carries — the `LYRICS`/`USLT`/`©lyr` tag,
or a `.lrc` sitting beside the track — which is what makes "that song that goes
something about a train" answerable: `aede query "lyrics:train"`. The tag costs
nothing, since raw tags are in the catalog; a sidecar is opened only for the
tracks that have one. `aede track "<title>" --lyrics` shows them in full, timed
where the file gives times.

**And who did what**, which is what a graph is for: `composer`, `lyricist`,
`producer`, `engineer`, `performer`, `conductor`, `remixer`, `featured`,
`mainartist`, and `performing` for anyone audible on it — the class that
counts a guest verse and not the words behind it. `artist:` matches any credit
in any role, which is why
`artist:ozzy artist:"zakk wylde"` already means "both are on it"; the role
fields ask the finer question.

```sh
aede query "composer:rhoads mainartist:ozzy"   # Ozzy singing what Randy wrote
aede query "producer:\"rick rubin\" year:1990.."
```

A value naming nothing in the library — a genre that does not exist, an artist
nobody ever heard of — is an **error**, not an empty result: the two are
different questions and deserve different answers.

A flag reads either way round: `lossless:false` and `-lossless` ask the same
thing, and accepting only one would make the other a silent trap.

Those last four also read `album.rating`, `artist.loved` and so on, because
**where** an opinion was written is part of what it says: five stars on the
artist is not five stars on the track, and a field that folded the two together
could never say which was meant.

## Searching what you wrote

Everything you write is queryable, and searching _inside_ a note or a tag is
the same field with a value:

```sh
aede query "tag:vinyl"              # tracks carrying that label
aede query "album.tag:vinyl"        # tracks whose album carries it
aede query "note:remaster"          # the note says "remaster" somewhere
aede query "artist.note:live"       # something written about the artist
aede query "album.rating:>=4 -played"
```

**The scope is part of the question, and it is the one thing that surprises
people.** A bare `loved`, `rating`, `tag` or `note` asks about the **track**. If
you marked an _album_ a favourite, `aede query "loved"` finds nothing — you
asked a different question from the one you meant. So an empty answer says
where what you wrote actually is, and offers the expression that finds it:

```
$ aede query "loved"
nothing matches "loved"
  1 track if you ask it of the album — that is where you wrote it
  aede query "album.loved"
```

The query still means exactly what it says; the line is a hint, not a
correction. Folding the scopes together instead would be worse: five stars on
an artist is not five stars on a track, and a field that merged them could
never say which was meant.

**A field written alone asks whether there is one at all**, and `-field` asks
the opposite — which is how a library is combed for what has _not_ been
annotated yet:

```sh
aede query "note"        # everything you have written a note on
aede query "-rating"     # everything you have never rated
aede query "tag"         # everything carrying at least one label
```

The two questions were one predicate here until it turned out they were two,
and the consequence was that "which things have I written a note on" could not
be asked at all: a bare `note` fell through to a text search for the word, and
`note:true` searched for the word "true". The cost of separating them is that a
bare `note`, `tag` or `rating` is no longer a text search for those three
words; written with a field they still are, as `title:note`.

## Comments

The `comment` tag is the one field _you_ write: where a rip came from, which pressing this is, what still needs replacing. It is read from every format and it is searchable, but only when asked:

```sh
aede search --comments "vinyl rip"
aede search --comments "to replace" --m3u --output=todo.m3u8
aede track "So What" --comment "2009 remaster"
aede albums --comment "vinyl"
```

Off by default on `search`, because a comment is free prose: a common word in one would bury the album that actually bears the name. Comment hits are shown in **their own section** and marked `found_in: comment` in the JSON — a hit says by which route it was found, the same rule that keeps an imported analysis in its own panel.

`--notes` does the same for what _you_ wrote:

```sh
aede search "vinyle" --notes
aede search "remaster" --notes
```

And `--lyrics` searches the words themselves, from the tag that carries them or from a `.lrc` beside the track:

```sh
aede search "all aboard" --lyrics
```

It shows **the line that matched**, not the song: a table cell holding four hundred lines is one nobody can read, and the line is what was half-remembered in the first place.

The three are deliberately not folded together, and the difference is worth stating: a **comment lives inside the audio file**, put there by whoever tagged it; a **note lives in `user.json`**, put there by you; the **words are the song**, and belong to nobody here. Searching one is searching the library, searching another is searching yourself — so they keep separate sections and separate options, and a hit says by which route it was found (`found_in: comment`, `lyrics` in the JSON). A note can be about anything, so its results name the kind: an artist, an album, a label.

## Saving a question

A query worth typing twice is worth a name.

```sh
aede collection wishlist --query "loved played:0"
aede collection wishlist                 # what it holds now
aede collection wishlist --m3u           # …as a playlist
aede collections                         # every saved query, and its size
aede collection wishlist --remove
```

It keeps the **question**, not the answer, which is the whole difference
between a smart collection and a playlist: it answers with what the library
holds now. And since running one produces a selection, `--m3u`, `--csv` and
`--json` apply to it with nothing written for the purpose.

An expression that does not parse is refused **when it is saved**, not the next
time somebody opens it: a collection that only fails when you reach for it is a
trap left for later.
