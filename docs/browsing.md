# Browsing: Navigating Your Collection

## The Art of the Facet: Reading the Liner Notes

Your music library is not a flat list of files; it is a rich, interconnected tapestry of creators, movements, and histories. In Aède, every entity deserves its moment in the spotlight. Just as `artist`, `album`, and `track` have their own dedicated pages, `genre` and `label` are now elevated to equal status.

```sh
aede genre metal            # Survey the albums, and the musicians behind them
aede label "Blue Note"      # Wander through the catalog of a legendary imprint
```

When you search for a name that doesn't perfectly match, Aède acts like a helpful record store clerk, widening the net. Typing `aede genre metal` will intuitively gather Black Metal and Doom Metal, politely letting you know it did so. Because every page represents a curated **selection**, you can instantly export your discoveries: `aede genre jazz --m3u` instantly presses a playlist of every jazz track on your shelves.

These facets double as powerful filters for your crates:

```sh
aede albums --genre metal --year 1994
aede albums --label "Blue Note"
```

**Roles are a two-way street.** The `--role` flag is your magnifying glass for the liner notes, and it works beautifully in two distinct ways:

```sh
aede artists --role producer                    # The sonic architects in your library
aede artist Ozzy --role performer               # Every track Ozzy lent his voice to
aede artist Ozzy --role performer --m3u         # …pressed instantly to a playlist
aede artists --role composer --csv --output=composers.csv
```

When scanning your **entire library**, it answers: _"Who performed this role across my collection?"_
When visiting an **individual's page**, it answers: _"What exactly did they contribute here?"_
This bi-directional elegance is exactly why Aède stores rich credits rather than a flat, lifeless "artist" text column.

_Note:_ A role requires a human attached to it! Asking `aede album "<title>" --role performer` will be gently refused, because a role without a performer is an empty question. If you are looking for an artist on an album, `--artist` is the filter you need.

A role is typed exactly the way it is **shown**: `--role "album artist"` or simply `--role album`. Quotes are optional. What you see on the screen is exactly what Aède understands. We believe a tool should never contradict itself by displaying a credit but refusing to let you search for it.

### Your Unique Vocabulary

Aède dynamically adapts to the tags _you_ actually have. `aede stats` reveals the unique vocabulary of your specific CDthèque, complete with counts. If a role returns nothing, you'll know it's because your files simply don't carry that tag, rather than suspecting a bug.

```text
Roles

  Role      Artists  Credits
  ────────  ───────  ───────
  composer       48      412
  producer       11       87
```

_(Note: `main` and `album` are omitted here, as every track carries them by default. When Aède fully integrates MusicBrainz, these roles will populate magically without a single line of code, drawn directly from global liner notes rather than a hardcoded list.)_

## The Collector's Compass: What's Missing?

Beyond celebrating the records you own, an artist's page acts as a completionist's guide. It anticipates the next question every curator asks: _"What am I missing?"_

```text
  3 studio albums MusicBrainz credits to them and this shelf does not hold: aede missing "Portishead"
```

Aède remains respectfully silent if your collection is complete or if you haven't queried that artist. We believe an empty line is just noise, and your pages should only display meaningful insights.

## Mapping Your Collection: Where is the Music From?

```sh
aede countries                        # Map your entire shelf
aede artists --country france         # Discover who calls France home
aede artists --country united         # Matches both UK and US, and tells you so
aede countries --csv --output=map.csv
```

**Geography is not built on your local tags**, and for good reason. Standard metadata is notoriously bad at this: `RELEASECOUNTRY` tells you where a piece of plastic was manufactured, not where the band’s soul resides. An American pressing of a French band does not make them an American band. Aède respects this nuance by fetching the true origin from MusicBrainz.

Because this data relies on the broader community database, Aède is transparent about what it knows and what it doesn't:

```text
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

**`Code` and `Also` represent two different truths.**
The `Code` is the strict, official ISO designation. The `Also` column contains the intuitive, derived initials Aède provides for your convenience. If we merged these, Canada (which has no secondary initials) would look like an error rather than a complete record.

**And `County Antrim` is not a bug.** MusicBrainz provides the most beautifully specific location it can—often a country, but sometimes a county or city. Aède preserves this historical accuracy. Overruling the community database would be a disservice to the archivist ethos.

Aède also distinguishes between _silences_. **An artist you haven't looked up yet** is very different from **an artist with an unknown origin**. Aède won't drop either from the record; it informs you so you never have to guess.

### Smart, Collision-Free Abbreviations

The abbreviations in the `Also` column are dynamically generated from _your_ specific library. Aède checks:

1. The **exact name** (`--country "united kingdom"`)
2. The **official ISO code** (`--country gb`)
3. The **initials** (`--country uk`, `--country nz`)
4. Any **substring** (`--country kingdom`)

We reject endless lists of vernacular spellings (`USA`, `Royaume-Uni`). The source of truth is singular and clear.

Crucially, **derived does not mean usable if it causes confusion.** `County Antrim` derives to `CA`, which is Canada's official code. If you have both Canadian artists and artists from County Antrim, Aède drops the derived `CA` for the county to prevent a collision, deferring to the official authority. But if your shelf has no Canadians? Aède keeps `CA` for County Antrim. Your library's unique geography defines its own rules.

## The Crate Digger: Querying Albums vs. Tracks

Sometimes you want to look at the granular details (tracks), and sometimes you want to step back and look at the whole package (the album). When you want a list of _albums_, Aède’s query engine seamlessly adapts:

```sh
aede albums --query "album.rating:>=4"        # Pull the great albums, not just scattered tracks
aede albums --query "album.tag:vinyl"
aede albums --artist ozzy --query "album.rating:>=4"
```

An album earns its place on the list if _any_ of its tracks meet your criteria. Options and expressions stack effortlessly, allowing you to narrow your search precisely.

The syntax is built for collectors:

- **Ranges:** `year:1990..` or `duration:..3:30`
- **Comparisons:** `>`, `>=`, `<`, `<=`
- **Matches:** `field:=value` for exact, bare values for substrings.
- **Durations:** Type naturally (`3:45`) or in seconds.
- **Logic:** `-` or `NOT` negates; putting terms side-by-side means AND.

A track missing a specific tag is honestly reported as **absent** rather than silently converted to a zero. Otherwise, every untagged bootleg would falsely claim it was recorded before 1970!

Because every query results in a beautifully curated **selection**, you can pipe it directly into `--csv`, `--json`, or `--m3u`. A saved query isn't just text; it’s a living, playable smart collection of your musical heritage.
