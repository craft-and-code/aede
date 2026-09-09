# Commands, options and output

## Where each option applies

Three groups, and an option that a command cannot honour is **refused**, never ignored. So is an **argument**: `aede artists ozzy` used to list every artist and drop the word, which looks like an answer. It now says what to type instead.

`export` describes the **catalog**: `--csv` gives one row per album, `--tracks` one row per track. It takes no argument.

The **listings** — `albums`, `artists`, `genres`, `labels`, `years` — turn into a table of exactly what they show, filters included. This is how several albums land in one file:

```sh
aede albums --csv --artist="Deicide" --output=deicide.csv
aede albums --csv --year=1990
aede albums --csv --compilations --output=compilations.csv
aede artists --csv --limit=100 --output=artists.csv
```

`album`, `artist`, `track` and `search` describe a **selection**: `--csv` and `--m3u` both apply to it, as a table of tracks or as a playlist. For an artist, that means the tracks they are audible on; for a search, the track hits and not the artists or albums found.

```sh
aede album "To Hell With God" --csv --output=album.csv
aede artist "Deicide" --csv --separator=tab | sort -t$'\t' -k9,9n     # sorted by size
```

`sort -t,` on the plain comma-separated form is not safe here and elsewhere: a title with a comma in it — a reissue, a live album, anything worded "Compilation, Vol. 2" — comes back quoted, exactly as RFC 4180 asks (`"Once Upon the Cross, Reissue"`), but `sort` and `cut` know nothing about CSV quoting. They split on every comma they see, quoted or not, so a single row with one gains a column and every field after it — including the one `-k9` is asked for — lands one to the right of where it should be for that row alone. `--separator=tab` sidesteps it: nothing in a tag is ever tab-separated, so the field a title happens to hold never needs quoting in the first place.

`aede album` takes **one** title — the words are joined so a title can be typed without quotes — and says which command lists several when given more.

The same holds for an option whose value is a **name**: `--artist`, `--album`, `--with`, `--genre` and `--label` take the words that follow, up to the next option. `--limit`, `--year`, `--output` and the rest take exactly one word, because a number or a path is one word.

```sh
aede artist Ozzy --with Zakk Wylde        # no quotes needed anywhere
aede artist Ozzy --with "Zakk Wylde"      # the same thing
aede track So What --artist Miles Davis --limit 1
```

Put the positional before the option: `aede track --artist Miles Davis So What` gives the whole tail to `--artist`, and the command then says it was given no title — rather than answering a question you did not ask.

`--output <file>`, or `-o`, writes to a file instead of filling the terminal — but only alongside `--csv`, `--json` or `--m3u` on a selection or a listing, or on `export`, `sources --export` and `notes --export`: those are the only places with a file's worth of text to hand it. Everything else here is a page meant for the screen, and `--output` is refused on one rather than silently dropped — `--with` above included:

```
$ aede artist Ozzy --with Zakk Wylde -o test.txt
Error: --output writes what --csv, --json or --m3u produce; this page has none of those to give it.
Drop --output to see it on screen, or add one of the three to write it out.
```

`stats` is another page, with nothing of the kind to give it:

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

Three formats, because they answer three different questions.

**JSON** (`aede export`) is the faithful dump: ten linked tables, one per table of the model. It is what rebuilds a catalog or feeds another program.

**CSV** (`aede export --csv`) is for a spreadsheet, and a spreadsheet cannot hold a graph. It writes **one row per album** — artist, title, year, track and disc counts, duration, size, formats, sample rates, bit depths, label, catalogue number, genres, integrity, folder — which is the view from above: sort by size to find what to re-rip, filter on `lossless` to see what is left to replace. `--tracks` switches to one row per track when the album is too coarse.

Its values are **raw**: `duration_ms` and `size_bytes`, not `4:20` and `31.2 MB`. A column that cannot be added up is a column that cannot be used.

`--separator=;` for Excel in a French or German locale, `--separator=tab` for a TSV. Fields are quoted per RFC 4180, so a title carrying a comma or a quotation mark does not shift every column that follows.

**M3U** (`--m3u`) is not an export of the catalog but of a **selection**: whatever is on screen becomes a playlist.

```sh
aede album "To Hell With God" --m3u --output=deicide.m3u8
aede search coltrane --m3u --output=coltrane.m3u8
mpv --playlist=deicide.m3u8
```

Paths are absolute, so the playlist works wherever it is saved; `#EXTINF` carries the duration and the title, so a player shows them without opening every file. Without `--output` it goes to standard output, which a shell supporting process substitution can hand straight to a player — `mpv --playlist=<(aede artist "Ozzy Osbourne" --m3u)`.

## Naming a folder

`check`, `spectrum`, `playlist`, `extract` and `fingerprint` all take folders, and all five work from the **catalog** rather than from the disk. So a folder the catalog has never seen is refused, rather than producing a run with nothing to do:

```
$ aede extract ~/Desktop/new-rips
Error: no file in the catalog is under "/Users/kcell/Desktop/new-rips".
It is on disk, so this catalog was scanned 3 days ago and has not seen it — a folder added since is not in it yet.
Add it: aede scan "/Users/kcell/Desktop/new-rips"
```

It used to answer "nothing to extract", which reads as _your files already have their covers_. The date is there because it is the fact that explains it: you can weigh "scanned three days ago" against what you have been doing for three days, and you cannot weigh "run a scan".

## Paging through a result

Every listing shows **50 rows** by default and says which ones they are:

```
  1–50 of 312 albums — --offset=50 for the next page, --all for every row
```

Three options, one window, the same everywhere:

```sh
aede albums                        # the first 50
aede albums --limit 50 --offset 50 # the next 50
aede albums --all                  # every row, however many
aede albums --all --csv -o all.csv # and into a file
```

**Only what is true of that screen is offered.** On the last one there is no next page, so none is named — a line saying `--offset=312` would send you to "starts past the end" and read as a broken command:

```
  301–312 of 312 albums — these are the last; drop --offset to start from the first
```

`--all` disappears from the line for the same reason, twice over: when you have already typed it, and on a last screen, where lifting the limit shows the same rows again because what cut them was the offset.

`--offset` is what a front end needs: the order of every listing is deterministic, so page two is genuinely the rows after page one. `--all` says "everything" by name rather than by an encoding to remember — `--limit=0` is refused, since it would show nothing, and so is `--limit abc`, which used to fall back on the default and answer a question nobody asked.

Nothing is printed when everything fit, so the line keeps meaning something. A window past the end says so rather than showing an empty screen that reads as an empty library.

`-o` is short for `--output`.
