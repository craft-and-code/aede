# Commands, options and output

## Where each option applies

Three groups, and an option that a command cannot honour is **refused**, never ignored. So is an **argument**: `aede artists ozzy` used to list every artist and drop the word, which looks like an answer. It now tells you exactly what to type instead, guiding you rather than leaving you guessing.

The main `aede help` page is an index: it groups library, fetch, artwork, filtering, copy, and import options without turning the terminal into a manual. Every command has its own page through `aede help <command>` or `aede <command> --help`; aliases work there too. `aede help fetch` adds its metadata, lyrics, artwork, Fanart.tv, and exclusion details.

`export` describes your entire **catalog**: `--csv` gives one row per album, `--tracks` one row per track. It takes no argument.

The **listings** — `albums`, `artists`, `genres`, `labels`, `years` — turn into a precise table of exactly what they show, filters included. This is how you gather several albums into one focused file:

```sh
aede albums --csv --artist="Deicide" --output=deicide.csv
aede albums --csv --year=1990
aede albums --csv --compilations --output=compilations.csv
aede artists --csv --limit=100 --output=artists.csv
```

`album`, `artist`, `track` and `search` describe a curated **selection**: `--csv` and `--m3u` both apply to it, acting as a table of tracks or as a ready-to-play playlist. For an artist, that means the tracks they are audible on; for a search, the track hits and not the artists or albums found.

`recording` and `work` traverse the canonical music graph. A recording gathers
the local album placements that share its MusicBrainz recording identity and
shows attributed work relationships and recording-level credits. A work gathers
the recordings that realize the composition and shows its composers, lyricists,
writers and arrangers when MusicBrainz provides them. `aede work` also accepts
a work obtained by `aede fetch --credits`: it is clearly marked as external
evidence and does not pretend that the fetch added a tag to the audio file.

`aede fetch --credits` uses recording identifiers already present in the local
tags. It keeps performers and production roles on the recording, creative roles
on the work, and preserves credited-as names, instruments or qualifiers, dates,
order and MusicBrainz relationship identifiers. The same sourced credits are
shown by `track`, `album`, `artist`, `recording` and `work`; `track --json`
keeps them separate from credits read locally from tags. The older
`--recordings` spelling remains an alias.

The local graph is traversable in both directions. `track` names the abstract
recording, its works and its release group; `recording` lists every local album
placement; `work` lists every recording; `album` identifies the release group
and its other local editions; `label` lists its releases; and `artist` keeps
discography, guest appearances, compilation appearances, writing/production
contributions, collaborations and dated memberships distinct. Ordinary
`search` also finds recordings, works and release groups by title or identity.

These pages are navigation points rather than terminal reports. Their
`Continue` section prints copyable commands for adjacent objects and `search`
includes an `Open` command on every named result. Stable MusicBrainz IDs are
used for recordings, works, release groups and precise editions whenever they
exist. `release-group <title|MBID>` is the page between an album identity and
all its local editions; `album` accepts a release MBID so two same-titled
pressings do not lead back to an ambiguous page.

`label` applies the same rule to identity. It says whether the MusicBrainz ID
came from a local tag, was confirmed by an identifier lookup, is only a
name-search proposal, or conflicts with the local tag. Proposals and conflicts
are never applied silently.

```sh
aede album "To Hell With God" --csv --output=album.csv
aede artist "Deicide" --csv --separator=tab | sort -t$'\t' -k9,9n     # sorted by size
```

`sort -t,` on the plain comma-separated form is a trap here and elsewhere: a title with a comma in it — a reissue, a live album, anything worded "Compilation, Vol. 2" — comes back quoted, exactly as RFC 4180 asks (`"Once Upon the Cross, Reissue"`), but standard tools like `sort` and `cut` know nothing about CSV quoting. They split on every comma they see, quoted or not, breaking your columns so that every field after the comma shifts one position to the right. This wreaks havoc on precise sorting parameters like `-k9,9n` (which tells `sort` to isolate the 9th column — in this case, the file size — and sort it numerically as a number rather than alphabetically). `--separator=tab` elegantly sidesteps this entire trap: nothing in a tag is ever tab-separated, so a title containing commas never needs quoting in the first place, keeping your data and your `-k9,9n` numeric sort perfectly aligned.

`aede album` takes **one** title — the words are joined so a title can be typed seamlessly without quotes — and gently reminds you which command lists several when given more.

The same fluid logic holds for an option whose value is a **name**: `--artist`, `--album`, `--with`, `--genre` and `--label` gracefully take the words that follow, up to the next option. Options like `--limit`, `--year`, or `--output` take exactly one word, because a number or a path is a single distinct entity.

```sh
aede artist Ozzy --with Zakk Wylde        # no quotes needed anywhere
aede artist Ozzy --with "Zakk Wylde"      # the same thing
aede track So What --artist Miles Davis --limit 1
```

Always put the positional argument before the option: `aede track --artist Miles Davis So What` gives the whole tail to `--artist`, and the command will simply tell you it was given no title — ensuring it never pretends to answer a question you didn't ask.

`--output <file>`, or `-o`, writes to a file instead of filling your terminal — but only alongside `--csv`, `--json` or `--m3u` on a selection or a listing, or on `export`, `sources --export` and `notes --export`. Those are the only commands with a tangible file's worth of text to deliver. Everything else here is a vibrant page meant for the screen, and `--output` is explicitly refused rather than silently ignored:

```
$ aede artist Ozzy --with Zakk Wylde -o test.txt
Error: --output writes what --csv, --json or --m3u produce; this page has none of those to give it.
Drop --output to see it on screen, or add one of the three to write it out.
```

`stats` is another deeply insightful page, offering a vital pulse check of your library, with no exportable file attached:

```
$ aede stats

Library

  Tracks                        20
  Albums                         6
    of which compilations        1
  Artists                        8
  Total duration              38 s
  Size on disk              1.3 MB

Quality

                       Count      Size
  ───────────────────  ─────  ────────  ────────────────────
  Lossless (CD)           11  399.3 kB  ████████████████████
  Hi-res                   4  664.3 kB  ███████·············
  Lossy (>= 256 kbps)      3  248.3 kB  █████···············
```

## Getting the data out

Three formats, because you ask your collection three different types of questions.

**JSON** (`aede export`) is the faithful, structural dump: ten linked tables, capturing the complete soul of the model. It is the raw material that rebuilds a catalog or feeds another program.

**CSV** (`aede export --csv`) is built for the spreadsheet, the ultimate sorting tool. It writes **one row per album** — artist, title, year, track and disc counts, duration, size, formats, sample rates, bit depths, label, catalogue number, genres, integrity, folder. This is your view from above: sort by size to pinpoint what to re-rip, filter on `lossless` to uncover what is left to upgrade. `--tracks` shifts focus to one row per track when you need microscopic precision.

Its values are **raw and honest**: `duration_ms` and `size_bytes`, not `4:20` and `31.2 MB`. A column that cannot be mathematically added is a column that fails the archivist.

Use `--separator=;` for Excel in a French or German locale, `--separator=tab` for a flawless TSV.

**M3U** (`--m3u`) transforms the cold data into music: whatever is on your screen becomes a living playlist.

```sh
aede album "To Hell With God" --m3u --output=deicide.m3u8
aede search coltrane --m3u --output=coltrane.m3u8
mpv --playlist=deicide.m3u8
```

Paths are absolute, ensuring the playlist works seamlessly wherever it is saved. `#EXTINF` carries the duration and title, letting players display them instantly without touching every file. Without `--output` it flows to standard output, ready to be piped straight into a player like `mpv --playlist=<(aede artist "Ozzy Osbourne" --m3u)`.

## Naming a folder

`check`, `spectrum`, `playlist`, `extract` and `fingerprint` all take folders, and all five operate directly from the **catalog** rather than polling the disk. A folder the catalog has never met is immediately refused, saving you from running a hollow command:

```
$ aede extract ~/Desktop/new-rips
Error: no file in the catalog is under "/Users/kcell/Desktop/new-rips".
It is on disk, so this catalog was scanned 3 days ago and has not seen it — a folder added since is not in it yet.
Add it: aede scan "/Users/kcell/Desktop/new-rips"
```

The date is provided because context is everything: you can effortlessly weigh "scanned three days ago" against the rips you've made since Tuesday.

## Paging through a result

Every listing displays a comfortable **50 rows** by default and clearly states your position:

```
  1–50 of 312 albums — --offset=50 for the next page, --all for every row
```

Three elegant options, one consistent window:

```sh
aede albums                        # the first 50
aede albums --limit 50 --offset 50 # the next 50
aede albums --all                  # every row, however many
aede albums --all --csv -o all.csv # and into a file
```

**Only the truth of the current screen is offered.** On the final page, no next step is suggested — preventing you from marching into the void:

```
  301–312 of 312 albums — these are the last; drop --offset to start from the first
```

`--offset` provides absolute predictability: the order of every listing is deterministic, so page two is undeniably the continuation of page one. `--all` uses plain language to say "everything" rather than relying on obscure encodings (like `--limit=0`, which is rightfully refused).

When your entire query fits on one screen, nothing extra is printed. Silence means completion. A window pushed past the end explicitly tells you so, never showing a blank screen that might induce a panic of a lost library.

`-o` is simply the swift companion to `--output`.
