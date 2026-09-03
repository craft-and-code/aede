# Browsing

## Browsing by facet

The model is a graph of entities carrying roles towards one another, and every entity deserves a page. `artist`, `album` and `track` had one; `genre` and `label` now do too.

```sh
aede genre metal            # the albums, and who is audible on them
aede label "Blue Note"      # the catalogue, and its artists
```

A name matching nothing exact widens to the names containing it — `aede genre metal` reaches Black Metal and Doom Metal — and says it did. What the page gathers is a **selection**, so `--csv` and `--m3u` apply: `aede genre jazz --m3u` is a playlist of every jazz track.

The listings take the same facets as filters:

```sh
aede albums --genre metal --year 1994
aede albums --label "Blue Note"
```

**Roles read both ways.** `--role` means two things, depending on what it is attached to — and both are useful:

```sh
aede artists --role producer                    # who produces, in my library
aede artist Ozzy --role performer               # what Ozzy sang on
aede artist Ozzy --role performer --m3u         # …as a playlist
aede artists --role composer --csv --output=composers.csv
```

On the **listing**, it answers "who does this here". On one person's **page**, it answers "what did they do in that role" — one step finer than the performing/writing split the page already shows. That a role can be read in both directions is the whole reason credits store one rather than a bare artist column.

It needs a person, so `aede album "<title>" --role performer` is refused: a role with nobody attached asks nothing. There, `--artist` is the filter.

A role is typed the way it is **shown**: `--role "album artist"` as well as `--role album`, in either case, with or without quotes. What a screen displays must be what the parser accepts, or the program contradicts itself — as it did, denying a role and listing it among the artist's credits in the same message.

Three different answers, because they are three different situations: a word that names no role at all lists the ones that do; a real role this library happens not to hold says so; and a role the _person_ does not hold names the ones they do. `aede stats` shows the whole vocabulary **this** library holds, with counts — so a role that returns nothing can be told apart from a bug: your files simply never carried that tag.

```
Roles

  Role      Artists  Credits
  ────────  ───────  ───────
  composer       48      412
  producer       11       87
```

`main` and `album` are left out: every track and every release carries them by construction, so they say nothing. At M1, a role coming from MusicBrainz will work here without a line of code, because the list is read from the credits rather than fixed.

## What an artist page names

Beyond the records themselves, the page names the two things a reader asks next and would otherwise have to remember:

```
  3 studio albums MusicBrainz credits to them and this shelf does not hold: aede missing "Portishead"
```

Silent when nothing has been browsed for that artist, or when the shelf holds everything — a line about an empty answer is noise on a page that is about something else.

## Where the artists are from

```sh
aede countries                        # the whole shelf, by country
aede artists --country france         # who is from there
aede artists --country united         # both kingdoms, and it says so
aede countries --csv --output=map.csv
```

**This one is not built on your tags**, and it is the only listing that is not. There is no usable tag for an artist's country: `RELEASECOUNTRY` exists, but it is the country a *pressing* came out in, which answers a different question and answers this one wrongly — an American pressing of a French band is not an American band. So the fact comes from MusicBrainz, and it is there only for artists `aede fetch` has asked about.

That has a visible consequence, and the commands say it rather than let you discover it:

```
Countries (4 in total)

  Country         Code  Also  Artists  Tracks  Duration      Size
  ──────────────  ────  ────  ───────  ──────  ────────  ────────
  France          FR              2       2      0:02   40.3 kB
  United Kingdom  GB    UK        1       1      0:01   20.1 kB
  United States   US              1       4      0:04   80.6 kB
  County Antrim                   1       1      0:01   20.1 kB
  1 place with no ISO code: MusicBrainz gave none, or the artists were fetched
  before Aède kept it — aede fetch --full asks again
  these are the areas MusicBrainz holds for your artists: usually a country,
  sometimes a county or a city
  7 artists not asked about yet: aede fetch
  2 artists asked about, with no area on record
```

**`Code` and `Also` are two different things, which is why they are two columns.** The code is the only thing in the table an authority assigned; the `Also` column is initials this program worked out from the name. Held in one column they misled in exactly the way that matters: `US` is the initials of "United States", so a table whose codes were all missing still looked like a table of codes — and Canada, a one-word name with nothing to derive, looked like a country Aède had no short form for rather than one nobody had asked MusicBrainz about since.

**And `County Antrim` is not a mistake.** MusicBrainz answers the most specific area it holds for an artist, and that is usually a country and sometimes a county or a city. Aède lists what the source said: dropping the row would be this program overruling it, which is the one thing the whole attributed layer exists not to do. What it will not do is let that row borrow somebody else's short form — see below.

Those two last lines are two different silences. **An artist nobody has asked about** and **an artist MusicBrainz has no area for** are not the same state, and a listing that dropped both without a word would have you counting rows and wondering. A library that has never fetched anything gets a third message again, naming the command that would fill it.

**The short forms in the `Also` column are the ones that work**, and there is no table of synonyms anywhere in the program. Each is derived from the source or from the name itself, in four steps:

1. the **name**, exactly — `--country "united kingdom"`;
2. the **ISO code** MusicBrainz states — `--country gb`, `--country fr`;
3. the **initials** of a multi-word name — `--country uk`, `--country nz`, computed from the name, so no country has to be known about in advance, and offered only where they name one place on *your* shelf;
4. any **substring** — `--country kingdom`, `--country united`, which may reach several and says which.

`USA`, `Royaume-Uni` and `Great Britain` are refused, and that is the intended answer rather than a gap: none is the source's name or its code, and a list of vernacular spellings has no bottom — whose, in which language, maintained by whom. The error names `aede countries`, which shows every form that works.

There is likewise no table of countries: `aede countries` lists what your shelf actually holds, the same way `aede stats` lists the roles your files actually carry.

**Derived is not the same as usable**, and step 3 is where that bites. `County Antrim` derives `CA`, which is Canada's ISO code; on a shelf holding Canadians, `--country CA` would have meant two things and the listing would have shown the collision as though it were a helpful abbreviation. So initials are dropped when they would answer for anything else: when two places derive the same ones, when they are some other place's code — a code an authority assigned beats initials this program worked out — and when they repeat their own place's code, where `US` in two columns reads as two facts. Nothing becomes unreachable: the name always works.

The collision is a property of *your* library, not of a world atlas this program does not hold. A shelf with no Canadian artist keeps `CA` for County Antrim, which is correct there.

A note on the codes: they were not always kept, so an artist fetched before this existed has a country but no code — the listing says how many, and `aede fetch --full` asks again. One artist re-fetched is enough to give the whole country its code.

## Listing albums rather than tracks

The grammar evaluates over **tracks**, because that is the finer question and
the coarser one is a fold of it. When what you want back is a list of _albums_,
`aede albums` takes the same expression:

```sh
aede albums --query "album.rating:>=4"        # the albums, not their tracks
aede albums --query "album.tag:vinyl"
aede albums --artist ozzy --query "album.rating:>=4"
```

An album is kept when any of its tracks answers. The option filters and the
expression compose by AND, so `--artist ozzy --query "…"` narrows twice.

Ranges are inclusive and either end may be left open (`year:1990..`,
`duration:..3:30`); comparisons are `>`, `>=`, `<`, `<=`; `field:=value` asks
for an exact match where a bare value asks for a substring; a length may be
typed `3:45` or in seconds; `-` or `NOT` negates; juxtaposition means AND.

A track with nothing to compare is **absent** from a numeric answer rather than
counted as zero — otherwise every untagged file would file itself under "before
1970".

What the command produces is a **selection**, so `--csv`, `--json` and `--m3u`
apply to it exactly as they do to an album page. That is why the grammar
evaluates over tracks: a saved query is a smart collection, and a smart
collection that is already a selection is already playable.
